use photo_core::{
    build_group_culling_result, refine_collection_semantic_groups, render_recipe_preview,
    write_group_sidecars, AnalysisCache, AutomationRunner, Batch, BatchStore,
    ClassificationRoutingExecutor, ClassificationStore,
    CullingReview,
    CullingReviewStore, CullingUserDecision, GroupCullingResult, GroupReferenceBinding, JobStatus,
    ModelBundleManifest, ModelPlatform, PhotoGroup, PreviewArtifact, PreviewStore, RawAsset,
    RawCatalog, RawImportResult, RawImporter, Recipe, RecipeReviewOverride, RecipeReviewStore,
    ReferenceStore, ReferenceWorkflowError, RunStep, SemanticGroupingConfig,
    SemanticRefinementReport, StyleProfile,
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
    previews: Vec<PreviewArtifact>,
}

#[derive(Clone, Serialize)]
struct GroupReferencePreview {
    group_id: Uuid,
    selected_reference_asset_id: Uuid,
    recipes: Vec<Recipe>,
    pending_asset_id: Option<Uuid>,
}

#[derive(Clone, Serialize)]
struct LightroomHandoffResult {
    group_id: Uuid,
    written_sidecars: Vec<String>,
}

#[derive(Clone, Serialize)]
struct GroupReferenceStyle {
    group_id: Uuid,
    reference_set_id: Uuid,
    style_profile: StyleProfile,
}

#[derive(Clone, Serialize)]
struct ReviewRenderResult {
    asset_id: Uuid,
    recipe_id: Uuid,
    cache_path: String,
}

struct AppState {
    runner: Arc<Mutex<AppRunner>>,
    store: BatchStore,
    catalog: RawCatalog,
    analysis_cache: AnalysisCache,
    preview_store: PreviewStore,
    culling_reviews: CullingReviewStore,
    reference_store: ReferenceStore,
    classifications: ClassificationStore,
    recipe_reviews: RecipeReviewStore,
    raw_importer: RawImporter,
    cache_root: PathBuf,
    controls: Arc<Mutex<HashMap<Uuid, Arc<BatchControl>>>>,
}

fn parse_batch_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| format!("invalid batch id: {error}"))
}

fn editable_group(
    group: &PhotoGroup,
    reviews: &CullingReviewStore,
) -> Result<PhotoGroup, String> {
    let mut asset_ids = Vec::with_capacity(group.asset_ids.len());
    for asset_id in &group.asset_ids {
        let rejected = reviews
            .get(*asset_id)
            .map_err(|error| error.to_string())?
            .is_some_and(|review| review.decision == CullingUserDecision::Reject);
        if !rejected {
            asset_ids.push(*asset_id);
        }
    }

    Ok(PhotoGroup {
        id: group.id,
        kind: group.kind,
        basis: group.basis,
        asset_ids,
        manual_locked: group.manual_locked,
    })
}

fn apply_recipe_reviews(
    recipes: &mut [Recipe],
    reviews: &RecipeReviewStore,
) -> Result<(), String> {
    for recipe in recipes {
        let Some(asset_id) = recipe.target_asset_id else {
            continue;
        };
        if let Some(review) = reviews
            .get(asset_id)
            .map_err(|error| error.to_string())?
        {
            review
                .apply_to_recipe(recipe)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn resolve_reviewed_group_recipes(
    group_id: Uuid,
    state: &AppState,
) -> Result<(PhotoGroup, Vec<Recipe>), String> {
    let group = state
        .catalog
        .find_group(group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("photo group not found: {group_id}"))?;
    let binding = state
        .reference_store
        .group_binding(group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "select a reference photo before reviewing edits".to_string())?;

    if state
        .culling_reviews
        .get(binding.selected_reference_asset_id)
        .map_err(|error| error.to_string())?
        .is_some_and(|review| review.decision == CullingUserDecision::Reject)
    {
        return Err("selected reference is explicitly rejected; choose another reference".to_string());
    }

    let reference_set = state
        .reference_store
        .get_set(binding.reference_set_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("reference set not found: {}", binding.reference_set_id))?;
    let editable = editable_group(&group, &state.culling_reviews)?;
    if editable.asset_ids.is_empty() {
        return Err("all photos in this group are explicitly rejected".to_string());
    }

    let mut resolved = reference_set
        .resolve_group_from_cache(
            &state.analysis_cache,
            &editable,
            binding.selected_reference_asset_id,
            1,
        )
        .map_err(|error| error.to_string())?;
    apply_recipe_reviews(&mut resolved.recipes, &state.recipe_reviews)?;
    Ok((editable, resolved.recipes))
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
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let preview_ids = assets.iter().map(|asset| asset.id).collect::<Vec<_>>();
    let previews = state
        .preview_store
        .list_for_assets(&preview_ids)
        .map_err(|error| error.to_string())?;
    Ok(BatchPhotoContext {
        assets,
        groups,
        previews,
    })
}

#[tauri::command]
fn refine_batch_groups(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<SemanticRefinementReport, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let current = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;

    for group in &current {
        if state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(
                "semantic refinement is locked after Reference selection; clear group references first"
                    .to_string(),
            );
        }
    }

    refine_collection_semantic_groups(
        &state.catalog,
        &state.classifications,
        &state.analysis_cache,
        batch_id,
        SemanticGroupingConfig::default(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_culling(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<GroupCullingResult>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;

    groups
        .iter()
        .map(|group| build_group_culling_result(&state.analysis_cache, group, 0.98))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_culling_reviews(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<CullingReview>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let batch = state
        .store
        .load_batch(batch_id)
        .map_err(|error| error.to_string())?;
    let asset_ids = batch
        .items
        .iter()
        .filter_map(|item| item.asset_id)
        .collect::<Vec<_>>();

    state
        .culling_reviews
        .list_for_assets(&asset_ids)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_culling_review(
    asset_id: String,
    decision: Option<CullingUserDecision>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let asset_id = Uuid::parse_str(&asset_id)
        .map_err(|error| format!("invalid asset id: {error}"))?;
    match decision {
        Some(decision) => state
            .culling_reviews
            .set(asset_id, decision)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        None => state
            .culling_reviews
            .clear(asset_id)
            .map_err(|error| error.to_string()),
    }
}

#[tauri::command]
fn batch_reference_bindings(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<GroupReferenceBinding>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let mut bindings = Vec::new();
    for group in groups {
        if let Some(binding) = state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
        {
            bindings.push(binding);
        }
    }
    Ok(bindings)
}

#[tauri::command]
fn set_group_reference(
    group_id: String,
    asset_id: String,
    state: State<'_, AppState>,
) -> Result<GroupReferenceBinding, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let asset_id = Uuid::parse_str(&asset_id)
        .map_err(|error| format!("invalid asset id: {error}"))?;
    let group = state
        .catalog
        .list_groups()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|group| group.id == group_id)
        .ok_or_else(|| format!("photo group not found: {group_id}"))?;
    if !group.asset_ids.contains(&asset_id) {
        return Err(format!("asset {asset_id} is not part of group {group_id}"));
    }
    if state
        .culling_reviews
        .get(asset_id)
        .map_err(|error| error.to_string())?
        .is_some_and(|review| review.decision == CullingUserDecision::Reject)
    {
        return Err("an explicitly rejected photo cannot be used as the group reference".to_string());
    }

    state
        .reference_store
        .set_single_photo_reference(group_id, asset_id, format!("Group {group_id} reference"))
        .map(|(_, binding)| binding)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_group_reference(
    group_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    state
        .reference_store
        .clear_group_binding(group_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_recipe_reviews(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<RecipeReviewOverride>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let batch = state
        .store
        .load_batch(batch_id)
        .map_err(|error| error.to_string())?;
    let asset_ids = batch
        .items
        .iter()
        .filter_map(|item| item.asset_id)
        .collect::<Vec<_>>();
    state
        .recipe_reviews
        .list_for_assets(&asset_ids)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_recipe_review(
    asset_id: String,
    exposure_delta_ev: f32,
    contrast_delta: f32,
    saturation_delta: f32,
    state: State<'_, AppState>,
) -> Result<RecipeReviewOverride, String> {
    let asset_id = Uuid::parse_str(&asset_id)
        .map_err(|error| format!("invalid asset id: {error}"))?;
    state
        .recipe_reviews
        .set(RecipeReviewOverride {
            asset_id,
            exposure_delta_ev,
            contrast_delta,
            saturation_delta,
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_recipe_review(
    asset_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let asset_id = Uuid::parse_str(&asset_id)
        .map_err(|error| format!("invalid asset id: {error}"))?;
    state
        .recipe_reviews
        .clear(asset_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_reference_styles(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<GroupReferenceStyle>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let mut styles = Vec::new();

    for group in groups {
        let Some(binding) = state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let set = state
            .reference_store
            .get_set(binding.reference_set_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("reference set not found: {}", binding.reference_set_id))?;
        styles.push(GroupReferenceStyle {
            group_id: group.id,
            reference_set_id: binding.reference_set_id,
            style_profile: set.style_profile,
        });
    }

    Ok(styles)
}

#[tauri::command]
fn update_group_reference_style(
    group_id: String,
    exposure_bias_ev: f32,
    contrast_preference: f32,
    saturation_preference: f32,
    state: State<'_, AppState>,
) -> Result<GroupReferenceStyle, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let binding = state
        .reference_store
        .group_binding(group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "select a reference photo before editing the group style".to_string())?;

    let exposure_bias_ev = exposure_bias_ev.clamp(-3.0, 3.0);
    let contrast_preference = contrast_preference.clamp(-100.0, 100.0);
    let saturation_preference = saturation_preference.clamp(-100.0, 100.0);
    let set = state
        .reference_store
        .update_group_style_profile(group_id, |profile| {
            profile.exposure_bias_ev = Some(exposure_bias_ev);
            profile.contrast_preference = Some(contrast_preference);
            profile.saturation_preference = Some(saturation_preference);
        })
        .map_err(|error| error.to_string())?;

    Ok(GroupReferenceStyle {
        group_id,
        reference_set_id: binding.reference_set_id,
        style_profile: set.style_profile,
    })
}

#[tauri::command]
fn batch_reference_previews(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<GroupReferencePreview>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let mut previews = Vec::new();

    for group in groups {
        let Some(binding) = state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let set = state
            .reference_store
            .get_set(binding.reference_set_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("reference set not found: {}", binding.reference_set_id))?;

        let editable = editable_group(&group, &state.culling_reviews)?;
        if state
            .culling_reviews
            .get(binding.selected_reference_asset_id)
            .map_err(|error| error.to_string())?
            .is_some_and(|review| review.decision == CullingUserDecision::Reject)
        {
            return Err(format!(
                "selected reference {} is explicitly rejected",
                binding.selected_reference_asset_id
            ));
        }

        match set.resolve_group_from_cache(
            &state.analysis_cache,
            &editable,
            binding.selected_reference_asset_id,
            1,
        ) {
            Ok(mut result) => {
                apply_recipe_reviews(&mut result.recipes, &state.recipe_reviews)?;
                previews.push(GroupReferencePreview {
                    group_id: group.id,
                    selected_reference_asset_id: binding.selected_reference_asset_id,
                    recipes: result.recipes,
                    pending_asset_id: None,
                });
            }
            Err(ReferenceWorkflowError::MissingExposureAnalysis(asset_id)) => {
                previews.push(GroupReferencePreview {
                    group_id: group.id,
                    selected_reference_asset_id: binding.selected_reference_asset_id,
                    recipes: Vec::new(),
                    pending_asset_id: Some(asset_id),
                });
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(previews)
}

#[tauri::command]
fn render_group_recipe_preview(
    group_id: String,
    asset_id: String,
    state: State<'_, AppState>,
) -> Result<ReviewRenderResult, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let asset_id = Uuid::parse_str(&asset_id)
        .map_err(|error| format!("invalid asset id: {error}"))?;

    let (editable, recipes) = resolve_reviewed_group_recipes(group_id, &state)?;
    if !editable.asset_ids.contains(&asset_id) {
        return Err("photo is not an editable member of this group".to_string());
    }
    let recipe = recipes
        .into_iter()
        .find(|recipe| recipe.target_asset_id == Some(asset_id))
        .ok_or_else(|| format!("adaptive Recipe not found for asset {asset_id}"))?;
    let source = state
        .preview_store
        .get(asset_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "RAW preview is not ready yet".to_string())?;
    let destination = state
        .cache_root
        .join("review-previews")
        .join(format!("{asset_id}.jpg"));

    render_recipe_preview(
        &PathBuf::from(source.cache_path),
        &destination,
        &recipe,
    )
    .map_err(|error| error.to_string())?;

    Ok(ReviewRenderResult {
        asset_id,
        recipe_id: recipe.id,
        cache_path: destination.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
fn write_group_reference_xmp(
    group_id: String,
    state: State<'_, AppState>,
) -> Result<LightroomHandoffResult, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let group = state
        .catalog
        .list_groups()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|group| group.id == group_id)
        .ok_or_else(|| format!("photo group not found: {group_id}"))?;
    let binding = state
        .reference_store
        .group_binding(group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "select a reference photo before Lightroom handoff".to_string())?;
    if state
        .culling_reviews
        .get(binding.selected_reference_asset_id)
        .map_err(|error| error.to_string())?
        .is_some_and(|review| review.decision == CullingUserDecision::Reject)
    {
        return Err("selected reference is explicitly rejected; choose another reference".to_string());
    }

    let reference_set = state
        .reference_store
        .get_set(binding.reference_set_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("reference set not found: {}", binding.reference_set_id))?;
    let editable = editable_group(&group, &state.culling_reviews)?;
    if editable.asset_ids.is_empty() {
        return Err("all photos in this group are explicitly rejected".to_string());
    }

    let mut resolved = reference_set
        .resolve_group_from_cache(
            &state.analysis_cache,
            &editable,
            binding.selected_reference_asset_id,
            1,
        )
        .map_err(|error| error.to_string())?;
    apply_recipe_reviews(&mut resolved.recipes, &state.recipe_reviews)?;
    let editable_ids = editable.asset_ids.iter().copied().collect::<HashSet<_>>();
    let assets = state
        .catalog
        .list_assets()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|asset| editable_ids.contains(&asset.id))
        .collect::<Vec<_>>();
    let written = write_group_sidecars(&assets, &resolved.recipes)
        .map_err(|error| error.to_string())?;

    Ok(LightroomHandoffResult {
        group_id,
        written_sidecars: written
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    })
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
            let analysis_cache = AnalysisCache::open(&database)?;
            let preview_store = PreviewStore::open(&database)?;
            let culling_reviews = CullingReviewStore::open(&database)?;
            let reference_store = ReferenceStore::open(&database)?;
            let recipe_reviews = RecipeReviewStore::open(&database)?;
            let classification_store = ClassificationStore::open(&database)?;
            let analyze_executor = LocalAnalyzeExecutor::new(
                &database,
                &cache_root,
                &model_root,
                ModelPlatform::Windows,
            )?;
            let executor = ClassificationRoutingExecutor::new(
                analyze_executor,
                classification_store.clone(),
            );
            let runner = AutomationRunner::new(store.clone(), executor);
            runner.recover_interrupted()?;
            let raw_importer = RawImporter::open(&database)?;
            app.manage(AppState {
                runner: Arc::new(Mutex::new(runner)),
                store,
                catalog,
                analysis_cache,
                preview_store,
                culling_reviews,
                reference_store,
                classifications: classification_store,
                recipe_reviews,
                raw_importer,
                cache_root,
                controls: Arc::new(Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_batches,
            batch_photo_context,
            refine_batch_groups,
            batch_culling,
            batch_culling_reviews,
            set_culling_review,
            batch_reference_bindings,
            set_group_reference,
            clear_group_reference,
            batch_recipe_reviews,
            set_recipe_review,
            clear_recipe_review,
            batch_reference_styles,
            update_group_reference_style,
            batch_reference_previews,
            render_group_recipe_preview,
            write_group_reference_xmp,
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
