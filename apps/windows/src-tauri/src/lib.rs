use photo_core::{AutomationRunner, Batch, BatchStore, NoopStageExecutor};
use std::sync::Mutex;
use tauri::{Manager, State};
use uuid::Uuid;

struct AppState {
    runner: Mutex<AutomationRunner<NoopStageExecutor>>,
}

fn parse_batch_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| format!("invalid batch id: {error}"))
}

fn with_runner<T>(
    state: &State<'_, AppState>,
    operation: impl FnOnce(&mut AutomationRunner<NoopStageExecutor>) -> Result<T, photo_core::RunnerError>,
) -> Result<T, String> {
    let mut runner = state
        .runner
        .lock()
        .map_err(|_| "batch runner lock is poisoned".to_string())?;
    operation(&mut runner).map_err(|error| error.to_string())
}

#[tauri::command]
fn list_batches(state: State<'_, AppState>) -> Result<Vec<Batch>, String> {
    with_runner(&state, |runner| runner.list_batches())
}

#[tauri::command]
fn create_batch(
    name: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Batch, String> {
    with_runner(&state, |runner| runner.create_batch(name, paths))
}

#[tauri::command]
fn run_batch(batch_id: String, state: State<'_, AppState>) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    with_runner(&state, |runner| runner.run_until_idle(batch_id))
}

#[tauri::command]
fn retry_failed(batch_id: String, state: State<'_, AppState>) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    with_runner(&state, |runner| runner.retry_failed(batch_id))
}

#[tauri::command]
fn pause_batch(batch_id: String, state: State<'_, AppState>) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    with_runner(&state, |runner| runner.pause_batch(batch_id))
}

#[tauri::command]
fn resume_batch(batch_id: String, state: State<'_, AppState>) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    with_runner(&state, |runner| runner.resume_batch(batch_id))
}

#[tauri::command]
fn cancel_batch(batch_id: String, state: State<'_, AppState>) -> Result<Batch, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    with_runner(&state, |runner| runner.cancel_batch(batch_id))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let store = BatchStore::open(data_dir.join("photo-cake.sqlite3"))?;
            let runner = AutomationRunner::new(store, NoopStageExecutor);
            runner.recover_interrupted()?;
            app.manage(AppState {
                runner: Mutex::new(runner),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_batches,
            create_batch,
            run_batch,
            retry_failed,
            pause_batch,
            resume_batch,
            cancel_batch
        ])
        .run(tauri::generate_context!())
        .expect("error while running Photo-Cake");
}
