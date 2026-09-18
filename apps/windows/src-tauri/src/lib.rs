use photo_core::{
    AutomationRunner, Batch, BatchStore, ClassificationRoutingExecutor, ClassificationStore,
    JobStatus, ModelBundleManifest, ModelPlatform, PhotoGroup, RawAsset, RawCatalog,
    RawImportResult, RawImporter, RunStep,
};
use photo_inference::LocalAnalyzeExecutor;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

type AppExecutor = ClassificationRoutingExecutor<LocalAnalyzeExecutor>;
type AppRunner = AutomationRunner<AppExecutor>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerIntent {
    Running,
    Paused,
    Cancelled,
}

#[derive(Debug)]
struct BatchControlState {
    intent: WorkerIntent,
    worker_active: bool,
}

#[derive(Debug)]
struct BatchControl {
    state: Mutex<BatchControlState>,
    wake: Condvar,
}

impl Default for BatchControl {
    fn default() -> Self {
        Self {
            state: Mutex::new(BatchControlState {
                intent: WorkerIntent::Running,
                worker_active: false,
            }),
            wake: Condvar::new(),
        }
    }
}

#[derive(Clone, Serialize)]
struct BatchWorkerEvent {
    batch: Batch,
    step: Option<RunStep>,
}

#[derive(Clone, Serialize)]
struct BatchWorkerError {
    batch_id: Uuid,
    message: String,
}

#[derive(Clone, Serialize)]
struct BatchPhotoContext {
    assets: Vec<RawAsset>,
    groups: Vec<PhotoGroup>,
}

struct AppState {
    runner: Arc<Mutex<AppRunner>>,
    store: BatchStore,
    catalog: RawCatalog,
    raw_importer: RawImporter,
    controls: Arc<Mutex<HashMap<Uuid, Arc<BatchControl>>>>,
}

fn parse_batch_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| format!("invalid batch id: {error}"))
}

fn lock_runner<T>(
    runner: &Arc<Mutex<AppRunner>>,
    operation: impl FnOnce(&mut AppRunner) -> Result<T, photo_core::RunnerError>,
) -> Result<T, String> {
    let mut runner = runner
        .lock()
        .map_err(|_| "batch runner lock is poisoned".to_string())?;
    operation(&mut runner).map_err(|error| error.to_string())
}

fn control_for(
    controls: &Arc<Mutex<HashMap<Uuid, Arc<BatchControl>>>>,
    batch_id: Uuid,
) -> Result<Arc<BatchControl>, String> {
    let mut controls = controls
        .lock()
        .map_err(|_| "batch control lock is poisoned".to_string())?;
    Ok(controls
        .entry(batch_id)
        .or_insert_with(|| Arc::new(BatchControl::default()))
        .clone())
}

fn emit_batch_update(app: &AppHandle, batch: Batch, step: Option<RunStep>) {
    let _ = app.emit("photo-cake://batch-updated", BatchWorkerEvent { batch, step });
}

fn start_batch_worker(
    app: AppHandle,
    runner: Arc<Mutex<AppRunner>>,
    controls: Arc<Mutex<HashMap<Uuid, Arc<BatchControl>>>>,
    batch_id: Uuid,
) -> Result<(), String> {
    let control = control_for(&controls, batch_id)?;
    let should_spawn = {
        let mut state = control
            .state
            .lock()
            .map_err(|_| "batch control state lock is poisoned".to_string())?;
        state.intent = WorkerIntent::Running;
        control.wake.notify_all();
        if state.worker_active {
            false
        } else {
            state.worker_active = true;
            true
        }
    };

    if !should_spawn {
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || {
        let result = run_worker_loop(&app, &runner, batch_id, &control);

        if let Ok(mut state) = control.state.lock() {
            state.worker_active = false;
        }

        if let Err(message) = result {
            let _ = app.emit(
                "photo-cake://batch-worker-error",
                BatchWorkerError { batch_id, message },
            );
        }
    });

    Ok(())
}

fn run_worker_loop(
    app: &AppHandle,
    runner: &Arc<Mutex<AppRunner>>,
    batch_id: Uuid,
    control: &Arc<BatchControl>,
) -> Result<(), String> {
    loop {
        let intent = {
            let state = control
                .state
                .lock()
                .map_err(|_| "batch control state lock is poisoned".to_string())?;
            state.intent
        };

        match intent {
            WorkerIntent::Cancelled => {
                let batch = lock_runner(runner, |runner| runner.cancel_batch(batch_id))?;
                emit_batch_update(app, batch, None);
                return Ok(());
            }
            WorkerIntent::Paused => {
                let batch = lock_runner(runner, |runner| runner.pause_batch(batch_id))?;
                emit_batch_update(app, batch, None);

                let mut state = control
                    .state
                    .lock()
                    .map_err(|_| "batch control state lock is poisoned".to_string())?;
                while state.intent == WorkerIntent::Paused {
                    state = control
                        .wake
                        .wait(state)
                        .map_err(|_| "batch control state lock is poisoned".to_string())?;
                }
                continue;
            }
            WorkerIntent::Running => {}
        }

        let (step, batch) = {
            let mut runner = runner
                .lock()
                .map_err(|_| "batch runner lock is poisoned".to_string())?;
            let current = runner
                .load_batch(batch_id)
                .map_err(|error| error.to_string())?;
            if current
                .items
                .iter()
                .any(|item| item.status == JobStatus::Paused)
            {
                runner
                    .resume_batch(batch_id)
                    .map_err(|error| error.to_string())?;
            }
            let step = runner
                .run_next(batch_id)
                .map_err(|error| error.to_string())?;
            let batch = runner
                .load_batch(batch_id)
                .map_err(|error| error.to_string())?;
            (step, batch)
        };

        emit_batch_update(app, batch.clone(), Some(step.clone()));

        if matches!(step, RunStep::Idle) {
            return Ok(());
        }
        if batch.stop_on_error
            && batch
                .items
                .iter()
                .any(|item| item.status == JobStatus::Failed)
        {
            return Ok(());
        }
    }
}

fn set_worker_intent(
    controls: &Arc<Mutex<HashMap<Uuid, Arc<BatchControl>>>>,
    batch_id: Uuid,
    intent: WorkerIntent,
) -> Result<bool, String> {
    let control = control_for(controls, batch_id)?;
    let active = {
        let mut state = control
            .state
            .lock()
            .map_err(|_| "batch control state lock is poisoned".to_string())?;
        state.intent = intent;
        let active = state.worker_active;
        control.wake.notify_all();
        active
    };
    Ok(active)
}

#[tauri::command]
fn list_batches(state: State<'_, AppState>) -> Result<Vec<Batch>, String> {
    state.store.list_batches().map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_photo_context(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<BatchPhotoContext, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let batch = state
        .store
        .load_batch(batch_id)
        .map_err(|error| error.to_string())?;
    let asset_ids = batch
        .items
        .iter()
        .filter_map(|item| item.asset_id)
        .collect::<HashSet<_>>();
    let assets = state
        .catalog
        .list_assets()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|asset| asset_ids.contains(&asset.id))
        .collect();
    let groups = state
        .catalog
        .list_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    Ok(BatchPhotoContext { assets, groups })
}

#[tauri::command]
fn create_batch(
    name: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Batch, String> {
    lock_runner(&state.runner, |runner| runner.create_batch(name, paths))
}

#[tauri::command]
async fn import_raw_paths(
    name: String,
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RawImportResult, String> {
    let importer = state.raw_importer.clone();
    let runner = state.runner.clone();
    let controls = state.controls.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        importer.import_paths(name, paths.into_iter().map(PathBuf::from))
    })
    .await
    .map_err(|error| format!("RAW import worker failed: {error}"))?
    .map_err(|error| error.to_string())?;

    if let Some(batch) = &result.batch {
        start_batch_worker(app, runner, controls, batch.id)?;
    }
    Ok(result)
}

#[tauri::command]
async fn import_raw_directory(
    name: String,
    directory: String,
    recursive: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RawImportResult, String> {
    let importer = state.raw_importer.clone();
    let runner = state.runner.clone();
    let controls = state.controls.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        importer.import_directory(name, PathBuf::from(directory), recursive)
    })
    .await
    .map_err(|error| format!("RAW import worker failed: {error}"))?
    .map_err(|error| error.to_string())?;

    if let Some(batch) = &result.batch {
        start_batch_worker(app, runner, controls, batch.id)?;
    }
    Ok(result)
}

#[tauri::command]
fn bundled_models() -> Result<ModelBundleManifest, String> {
    ModelBundleManifest::bundled().map_err(|error| error.to_string())
}

#[tauri::command]
fn run_batch(
    batch_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let batch = state
        .store
        .load_batch(batch_id)
        .map_err(|error| error.to_string())?;
    start_batch_worker(app, state.runner.clone(), state.controls.clone(), batch_id)?;
    Ok(batch)
}

#[tauri::command]
fn retry_failed(
    batch_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let batch = lock_runner(&state.runner, |runner| runner.retry_failed(batch_id))?;
    start_batch_worker(app, state.runner.clone(), state.controls.clone(), batch_id)?;
    Ok(batch)
}

#[tauri::command]
fn pause_batch(batch_id: String, state: State<'_, AppState>) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let active = set_worker_intent(&state.controls, batch_id, WorkerIntent::Paused)?;
    if active {
        state
            .store
            .load_batch(batch_id)
            .map_err(|error| error.to_string())
    } else {
        lock_runner(&state.runner, |runner| runner.pause_batch(batch_id))
    }
}

#[tauri::command]
fn resume_batch(
    batch_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let active = set_worker_intent(&state.controls, batch_id, WorkerIntent::Running)?;
    let batch = state
        .store
        .load_batch(batch_id)
        .map_err(|error| error.to_string())?;
    if !active {
        start_batch_worker(app, state.runner.clone(), state.controls.clone(), batch_id)?;
    }
    Ok(batch)
}

#[tauri::command]
fn cancel_batch(batch_id: String, state: State<'_, AppState>) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let active = set_worker_intent(&state.controls, batch_id, WorkerIntent::Cancelled)?;
    if active {
        state
            .store
            .load_batch(batch_id)
            .map_err(|error| error.to_string())
    } else {
        lock_runner(&state.runner, |runner| runner.cancel_batch(batch_id))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database = data_dir.join("photo-cake.sqlite3");
            let cache_root = data_dir.join("cache");
            let model_root = std::env::var_os("PHOTO_CAKE_MODEL_DIR")
                .map(PathBuf::from)
                .unwrap_or(app.path().resource_dir()?.join("models"));

            let store = BatchStore::open(&database)?;
            let catalog = RawCatalog::open(&database)?;
            let classification_store = ClassificationStore::open(&database)?;
            let analyze_executor = LocalAnalyzeExecutor::new(
                &database,
                &cache_root,
                &model_root,
                ModelPlatform::Windows,
            )?;
            let executor =
                ClassificationRoutingExecutor::new(analyze_executor, classification_store);
            let runner = AutomationRunner::new(store.clone(), executor);
            runner.recover_interrupted()?;
            let raw_importer = RawImporter::open(&database)?;
            app.manage(AppState {
                runner: Arc::new(Mutex::new(runner)),
                store,
                catalog,
                raw_importer,
                controls: Arc::new(Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_batches,
            batch_photo_context,
            create_batch,
            import_raw_paths,
            import_raw_directory,
            bundled_models,
            run_batch,
            retry_failed,
            pause_batch,
            resume_batch,
            cancel_batch
        ])
        .run(tauri::generate_context!())
        .expect("error while running Photo-Cake");
}
