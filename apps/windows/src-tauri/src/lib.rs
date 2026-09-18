use photo_core::{
    apply_companion_patch as apply_companion_patch_core,
    build_companion_snapshot as build_companion_snapshot_core, build_group_culling_result,
    preflight_group_sidecars, refine_collection_semantic_groups, render_recipe_preview,
    write_group_sidecars, write_sidecar_batch, AnalysisCache,
    AssetMetadataEvidence, AutomationRunner, Batch, BatchStore, ClassificationRoutingExecutor,
    ClassificationStore, CompanionDecisionPatch, CompanionPatchApplyReport, CompanionSnapshot,
    CompanionSnapshotStore, CullingReview, CullingReviewStore, CullingUserDecision,
    GroupCullingResult, GroupReferenceBinding, JobStatus, ModelBundleManifest, ModelPlatform,
    PhotoGroup, PreviewArtifact, PreviewStore, RawAsset, RawCatalog, RawImportResult, RawImporter,
    RawMetadataStore, Recipe, RecipeReviewOverride, RecipeReviewStore, ReferenceStore,
    ReferenceWorkflowError, RunStep, SemanticGroupingConfig, SemanticRefinementReport,
    StyleProfile,
};
use photo_inference::LocalAnalyzeExecutor;
use serde::{Deserialize, Serialize};
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
    metadata: Vec<AssetMetadataEvidence>,
}

#[derive(Clone, Serialize)]
struct GroupReferencePreview {
    group_id: Uuid,
    selected_reference_asset_id: Uuid,
    recipes: Vec<Recipe>,
    pending_asset_id: Option<Uuid>,
    reviewed_asset_ids: Vec<Uuid>,
}

#[derive(Clone, Serialize)]
struct LightroomHandoffPreflight {
    group_id: Uuid,
    target_sidecars: Vec<String>,
    current_sidecars: Vec<String>,
    conflicting_sidecars: Vec<String>,
}

#[derive(Clone, Serialize)]
struct LightroomHandoffResult {
    group_id: Uuid,
    written_sidecars: Vec<String>,
    verified_sidecar_count: usize,
}

#[derive(Clone, Serialize)]
struct LightroomBatchHandoffResult {
    groups: Vec<LightroomHandoffResult>,
}
#[derive(Clone, Deserialize)]
struct RecipeReviewBatchItem {
    group_id: String,
    asset_id: String,
}

#[derive(Clone, Serialize)]
struct RecipeReviewBatchResult {
    asset_ids: Vec<Uuid>,
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
    metadata_store: RawMetadataStore,
    culling_reviews: CullingReviewStore,
    reference_store: ReferenceStore,
    companion_snapshots: CompanionSnapshotStore,
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

fn confirmed_recipe_assets(
    recipes: &[Recipe],
    reviews: &RecipeReviewStore,
) -> Result<Vec<Uuid>, String> {
    let mut asset_ids = Vec::new();
    for recipe in recipes {
        let Some(asset_id) = recipe.target_asset_id else {
            continue;
        };
        if reviews
            .is_recipe_confirmed(recipe)
            .map_err(|error| error.to_string())?
        {
            asset_ids.push(asset_id);
        }
    }
    Ok(asset_ids)
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
fn build_companion_snapshot(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<CompanionSnapshot, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let snapshot = build_companion_snapshot_core(
        batch_id,
        &state.store,
        &state.catalog,
        &state.analysis_cache,
        &state.preview_store,
        &state.metadata_store,
        &state.culling_reviews,
        &state.reference_store,
        0.98,
    )
    .map_err(|error| error.to_string())?;
    state
        .companion_snapshots
        .save(&snapshot)
        .map_err(|error| error.to_string())?;
    Ok(snapshot)
}

#[tauri::command]
fn apply_companion_decision_patch(
    patch: CompanionDecisionPatch,
    state: State<'_, AppState>,
) -> Result<CompanionPatchApplyReport, String> {
    let snapshot = state
        .companion_snapshots
        .get(patch.batch_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("no exported companion snapshot for batch {}", patch.batch_id))?;
    let report = apply_companion_patch_core(
        &snapshot,
        &patch,
        &state.culling_reviews,
        &state.reference_store,
    )
    .map_err(|error| error.to_string())?;
    state
        .companion_snapshots
        .clear(patch.batch_id)
        .map_err(|error| error.to_string())?;
    Ok(report)
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
    let assets: Vec<RawAsset> = state
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
    let metadata = state
        .metadata_store
        .list_for_assets(&preview_ids)
        .map_err(|error| error.to_string())?;
    Ok(BatchPhotoContext {
        assets,
        groups,
        previews,
        metadata,
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
fn set_culling_reviews(
    reviews: Vec<CullingReview>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .culling_reviews
        .set_many(&reviews)
        .map_err(|error| error.to_string())
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
        .find_group(group_id)
        .map_err(|error| error.to_string())?
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
fn copy_group_reference_style(
    source_group_id: String,
    target_group_id: String,
    state: State<'_, AppState>,
) -> Result<GroupReferenceStyle, String> {
    let source_group_id = Uuid::parse_str(&source_group_id)
        .map_err(|error| format!("invalid source group id: {error}"))?;
    let target_group_id = Uuid::parse_str(&target_group_id)
        .map_err(|error| format!("invalid target group id: {error}"))?;
    if source_group_id == target_group_id {
        return Err("source and target groups must be different".to_string());
    }

    let target_binding = state
        .reference_store
        .group_binding(target_group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "select a target group reference before copying a look".to_string())?;
    let set = state
        .reference_store
        .copy_group_style_profile(source_group_id, target_group_id)
        .map_err(|error| error.to_string())?;

    Ok(GroupReferenceStyle {
        group_id: target_group_id,
        reference_set_id: target_binding.reference_set_id,
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
                let reviewed_asset_ids =
                    confirmed_recipe_assets(&result.recipes, &state.recipe_reviews)?;
                previews.push(GroupReferencePreview {
                    group_id: group.id,
                    selected_reference_asset_id: binding.selected_reference_asset_id,
                    recipes: result.recipes,
                    pending_asset_id: None,
                    reviewed_asset_ids,
                });
            }
            Err(ReferenceWorkflowError::MissingExposureAnalysis(asset_id)) => {
                previews.push(GroupReferencePreview {
                    group_id: group.id,
                    selected_reference_asset_id: binding.selected_reference_asset_id,
                    recipes: Vec::new(),
                    pending_asset_id: Some(asset_id),
                    reviewed_asset_ids: Vec::new(),
                });
            }
            Err(error) => return Err(error.to_string()),
        }
    }

    Ok(previews)
}

#[tauri::command]
fn set_recipe_reviewed(
    group_id: String,
    asset_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let asset_id = Uuid::parse_str(&asset_id)
        .map_err(|error| format!("invalid asset id: {error}"))?;
    let (editable, recipes) = resolve_reviewed_group_recipes(group_id, &state)?;
    if !editable.asset_ids.contains(&asset_id) {
        return Err("photo is not an editable member of this group".to_string());
    }
    let recipe = recipes
        .iter()
        .find(|recipe| recipe.target_asset_id == Some(asset_id))
        .ok_or_else(|| format!("adaptive Recipe not found for asset {asset_id}"))?;
    state
        .recipe_reviews
        .confirm_recipe(recipe)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn confirm_recipe_reviews(
    items: Vec<RecipeReviewBatchItem>,
    state: State<'_, AppState>,
) -> Result<RecipeReviewBatchResult, String> {
    if items.is_empty() {
        return Err("no Recipe reviews selected for batch confirmation".to_string());
    }

    let mut seen_assets = HashSet::with_capacity(items.len());
    let mut requested: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for item in items {
        let group_id = Uuid::parse_str(&item.group_id)
            .map_err(|error| format!("invalid group id: {error}"))?;
        let asset_id = Uuid::parse_str(&item.asset_id)
            .map_err(|error| format!("invalid asset id: {error}"))?;
        if !seen_assets.insert(asset_id) {
            return Err(format!("duplicate Recipe review asset: {asset_id}"));
        }
        requested.entry(group_id).or_default().push(asset_id);
    }

    let mut recipes_to_confirm = Vec::with_capacity(seen_assets.len());
    for (group_id, asset_ids) in requested {
        let (editable, recipes) = resolve_reviewed_group_recipes(group_id, &state)?;
        for asset_id in asset_ids {
            if !editable.asset_ids.contains(&asset_id) {
                return Err(format!(
                    "photo {asset_id} is not an editable member of group {group_id}"
                ));
            }
            let recipe = recipes
                .iter()
                .find(|recipe| recipe.target_asset_id == Some(asset_id))
                .ok_or_else(|| format!("adaptive Recipe not found for asset {asset_id}"))?;
            recipes_to_confirm.push(recipe.clone());
        }
    }

    let confirmations = state
        .recipe_reviews
        .confirm_recipes(&recipes_to_confirm)
        .map_err(|error| error.to_string())?;
    Ok(RecipeReviewBatchResult {
        asset_ids: confirmations
            .into_iter()
            .map(|confirmation| confirmation.asset_id)
            .collect(),
    })
}

#[tauri::command]
fn clear_recipe_reviewed(
    asset_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let asset_id = Uuid::parse_str(&asset_id)
        .map_err(|error| format!("invalid asset id: {error}"))?;
    state
        .recipe_reviews
        .clear_confirmation(asset_id)
        .map_err(|error| error.to_string())
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

fn resolve_lightroom_handoff(
    group_id: Uuid,
    state: &AppState,
) -> Result<(Vec<RawAsset>, Vec<Recipe>), String> {
    let group = state
        .catalog
        .find_group(group_id)
        .map_err(|error| error.to_string())?
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

    Ok((assets, resolved.recipes))
}

#[tauri::command]
fn preflight_group_reference_xmp(
    group_id: String,
    state: State<'_, AppState>,
) -> Result<LightroomHandoffPreflight, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let (assets, recipes) = resolve_lightroom_handoff(group_id, &state)?;
    let targets = preflight_group_sidecars(&assets, &recipes)
        .map_err(|error| error.to_string())?;

    Ok(LightroomHandoffPreflight {
        group_id,
        target_sidecars: targets
            .iter()
            .map(|target| target.sidecar_path.to_string_lossy().into_owned())
            .collect(),
        current_sidecars: targets
            .iter()
            .filter(|target| target.existing_matches_recipe)
            .filter_map(|target| {
                target
                    .existing_sidecar
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .collect(),
        conflicting_sidecars: targets
            .into_iter()
            .filter(|target| {
                target.existing_sidecar.is_some() && !target.existing_matches_recipe
            })
            .filter_map(|target| {
                target
                    .existing_sidecar
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .collect(),
    })
}

#[tauri::command]
fn write_group_reference_xmp(
    group_id: String,
    state: State<'_, AppState>,
) -> Result<LightroomHandoffResult, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let (assets, recipes) = resolve_lightroom_handoff(group_id, &state)?;
    let verified_sidecar_count = recipes.len();
    let written = write_group_sidecars(&assets, &recipes)
        .map_err(|error| error.to_string())?;

    Ok(LightroomHandoffResult {
        group_id,
        written_sidecars: written
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        verified_sidecar_count,
    })
}

#[tauri::command]
fn write_reference_xmp_batch(
    group_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<LightroomBatchHandoffResult, String> {
    if group_ids.is_empty() {
        return Err("no Lightroom groups selected for batch handoff".to_string());
    }

    let mut seen = HashSet::new();
    let mut resolved_ids = Vec::with_capacity(group_ids.len());
    let mut resolved_groups = Vec::with_capacity(group_ids.len());

    for value in group_ids {
        let group_id = Uuid::parse_str(&value)
            .map_err(|error| format!("invalid group id: {error}"))?;
        if !seen.insert(group_id) {
            return Err(format!("duplicate Lightroom group in batch handoff: {group_id}"));
        }

        resolved_ids.push(group_id);
        resolved_groups.push(resolve_lightroom_handoff(group_id, &state)?);
    }

    let verified_counts = resolved_groups
        .iter()
        .map(|(_, recipes)| recipes.len())
        .collect::<Vec<_>>();
    let written = write_sidecar_batch(&resolved_groups)
        .map_err(|error| error.to_string())?;

    Ok(LightroomBatchHandoffResult {
        groups: resolved_ids
            .into_iter()
            .zip(written)
            .zip(verified_counts)
            .map(|((group_id, paths), verified_sidecar_count)| LightroomHandoffResult {
                group_id,
                written_sidecars: paths
                    .into_iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                verified_sidecar_count,
            })
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
            let metadata_store = RawMetadataStore::open(&database)?;
            let culling_reviews = CullingReviewStore::open(&database)?;
            let reference_store = ReferenceStore::open(&database)?;
            let companion_snapshots = CompanionSnapshotStore::open(&database)?;
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
                metadata_store,
                culling_reviews,
                reference_store,
                companion_snapshots,
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
            build_companion_snapshot,
            apply_companion_decision_patch,
            batch_photo_context,
            refine_batch_groups,
            batch_culling,
            batch_culling_reviews,
            set_culling_review,
            set_culling_reviews,
            batch_reference_bindings,
            set_group_reference,
            clear_group_reference,
            batch_recipe_reviews,
            set_recipe_review,
            clear_recipe_review,
            batch_reference_styles,
            update_group_reference_style,
            copy_group_reference_style,
            batch_reference_previews,
            set_recipe_reviewed,
            confirm_recipe_reviews,
            clear_recipe_reviewed,
            render_group_recipe_preview,
            preflight_group_reference_xmp,
            write_group_reference_xmp,
            write_reference_xmp_batch,
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
