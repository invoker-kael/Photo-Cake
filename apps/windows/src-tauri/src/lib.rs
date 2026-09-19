use photo_core::{
    apply_companion_patch as apply_companion_patch_core,
    build_companion_snapshot as build_companion_snapshot_core, build_group_culling_result,
    build_recipe_review_group_preflight, build_reference_readiness_plan,
    build_style_sync_group_context, build_style_sync_preflight, derive_workflow_status,
    preflight_group_sidecars, refine_collection_semantic_groups, render_recipe_preview,
    route_exposure_bracket_sources, write_group_sidecars, write_sidecar_batch, AnalysisCache,
    AssetMetadataEvidence, AutomationRunner, Batch, BatchStore, ClassificationRoutingExecutor,
    ClassificationStore, CompanionDecisionPatch, CompanionPatchApplyReport, CompanionSnapshot,
    CompanionSnapshotStore, CullingDecision, CullingReview, CullingReviewStore, CullingUserDecision,
    ExposureBracketMergeStore, ExposureBracketSet, GroupCullingResult, GroupReferenceBinding,
    MomentQuickCullOperation, MomentQuickCullPlan,
    JobStatus, ModelBundleManifest, ModelPlatform,
    PhotoGroup, PreviewArtifact, PreviewStore, RawAsset, RawCatalog, RawImportResult, RawImporter,
    RawMetadataStore, Recipe, RecipeReviewGroupPreflight, RecipeReviewOverride, RecipeReviewSignal,
    RecipeReviewStore, RecipeReviewSyncFields, ReferenceReadinessPlan, ReferenceReadinessStatus,
    ReferenceStore,
    ReferenceWorkflowError, RunStep, SemanticGroupingConfig, SemanticRefinementReport,
    StyleProfile, StyleSyncGroupContext, StyleSyncPreflight, WorkflowFacts, WorkflowStatus,
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
    exposure_brackets: Vec<ExposureBracketSet>,
}

#[derive(Clone, Serialize)]
struct LightroomHandoffPreflight {
    group_id: Uuid,
    target_sidecars: Vec<String>,
    current_sidecars: Vec<String>,
    missing_sidecars: Vec<String>,
    conflicting_sidecars: Vec<String>,
    hdr_source_asset_ids: Vec<Uuid>,
    hdr_merge_required: bool,
    hdr_merge_completed: bool,
}

struct LightroomResolvedHandoff {
    assets: Vec<RawAsset>,
    recipes: Vec<Recipe>,
    exposure_brackets: Vec<ExposureBracketSet>,
    hdr_source_asset_ids: Vec<Uuid>,
    hdr_merge_completed: bool,
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
struct MomentQuickCullBatchResult {
    group_ids: Vec<Uuid>,
    reviews: Vec<CullingReview>,
    operation: MomentQuickCullOperation,
}

#[derive(Clone, Serialize)]
struct RecipeReviewGroupBatchResult {
    group_ids: Vec<Uuid>,
    asset_ids: Vec<Uuid>,
}

#[derive(Clone, Serialize)]
struct RecipeReviewSyncResult {
    group_id: Uuid,
    source_asset_id: Uuid,
    overrides: Vec<RecipeReviewOverride>,
}

#[derive(Clone, Deserialize)]
struct ReferenceBatchItem {
    group_id: String,
    asset_id: String,
}

#[derive(Clone, Serialize)]
struct ReferenceBatchResult {
    bindings: Vec<GroupReferenceBinding>,
}

#[derive(Clone, Serialize)]
struct GroupReferenceStyle {
    group_id: Uuid,
    reference_set_id: Uuid,
    style_profile: StyleProfile,
}

#[derive(Clone, Serialize)]
struct GroupReferenceStyleBatchResult {
    styles: Vec<GroupReferenceStyle>,
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
    bracket_merges: ExposureBracketMergeStore,
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

fn delivery_group(
    group: &PhotoGroup,
    state: &AppState,
) -> Result<(PhotoGroup, Vec<ExposureBracketSet>, Vec<Uuid>), String> {
    let routing = route_exposure_bracket_sources(&state.analysis_cache, group)
        .map_err(|error| error.to_string())?;
    let source_ids = routing
        .source_asset_ids
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut editable = editable_group(group, &state.culling_reviews)?;
    editable
        .asset_ids
        .retain(|asset_id| !source_ids.contains(asset_id));
    Ok((editable, routing.sets, routing.source_asset_ids))
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
    let (editable, exposure_brackets, _) = delivery_group(&group, state)?;
    if editable.asset_ids.is_empty() {
        if !exposure_brackets.is_empty() {
            return Err(
                "exposure-bracket source frames are reserved for HDR merge and have no standard Recipe to review"
                    .to_string(),
            );
        }
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

fn recipe_review_preflight_for_group(
    group_id: Uuid,
    state: &AppState,
) -> Result<(RecipeReviewGroupPreflight, Vec<Recipe>), String> {
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

    let culling = build_group_culling_result(&state.analysis_cache, &group, 0.98)
        .map_err(|error| error.to_string())?;
    let culling_reviews = state
        .culling_reviews
        .list_for_assets(&group.asset_ids)
        .map_err(|error| error.to_string())?;
    let context = build_style_sync_group_context(
        group.id,
        &culling.recommendations,
        &culling.pending_asset_ids,
        &culling_reviews,
    );
    let reviews_by_asset = culling_reviews
        .iter()
        .map(|review| (review.asset_id, review.decision))
        .collect::<HashMap<_, _>>();
    let recommendations_by_asset = culling
        .recommendations
        .iter()
        .map(|recommendation| (recommendation.asset_id, recommendation))
        .collect::<HashMap<_, _>>();

    let reference_set = state
        .reference_store
        .get_set(binding.reference_set_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("reference set not found: {}", binding.reference_set_id))?;
    let (editable, _, hdr_source_asset_ids) = delivery_group(&group, state)?;

    let mut recipes = if editable.asset_ids.is_empty() {
        Vec::new()
    } else {
        match reference_set.resolve_group_from_cache(
            &state.analysis_cache,
            &editable,
            binding.selected_reference_asset_id,
            1,
        ) {
            Ok(mut resolved) => {
                apply_recipe_reviews(&mut resolved.recipes, &state.recipe_reviews)?;
                resolved.recipes
            }
            Err(ReferenceWorkflowError::MissingExposureAnalysis(_)) => Vec::new(),
            Err(error) => return Err(error.to_string()),
        }
    };

    let recipes_by_asset = recipes
        .iter()
        .filter_map(|recipe| recipe.target_asset_id.map(|asset_id| (asset_id, recipe)))
        .collect::<HashMap<_, _>>();
    let mut signals = Vec::with_capacity(editable.asset_ids.len());
    for asset_id in &editable.asset_ids {
        let recipe = recipes_by_asset.get(asset_id).copied();
        let confirmed = recipe
            .map(|recipe| {
                state
                    .recipe_reviews
                    .is_recipe_confirmed(recipe)
                    .map_err(|error| error.to_string())
            })
            .transpose()?
            .unwrap_or(false);
        let user_decision = recipe
            .and_then(|_| reviews_by_asset.get(asset_id).copied());
        let ai_decision = recipe.and_then(|_| {
            recommendations_by_asset
                .get(asset_id)
                .map(|recommendation| recommendation.decision)
        });
        let evidence_pending = recipe.is_none()
            || (!confirmed
                && user_decision.is_none()
                && (culling.pending_asset_ids.contains(asset_id) || ai_decision.is_none()));
        let has_exception = if recipe.is_some() {
            state
                .recipe_reviews
                .get(*asset_id)
                .map_err(|error| error.to_string())?
                .is_some()
        } else {
            false
        };

        signals.push((
            *asset_id,
            RecipeReviewSignal {
                confirmed,
                has_exception,
                user_decision,
                ai_decision,
                evidence_pending,
            },
        ));
    }

    let plan = build_recipe_review_group_preflight(
        group.id,
        &signals,
        context.contains_people,
        context.scene_tags,
        hdr_source_asset_ids,
    );
    recipes.sort_by_key(|recipe| {
        recipe
            .target_asset_id
            .and_then(|asset_id| editable.asset_ids.iter().position(|value| *value == asset_id))
            .unwrap_or(usize::MAX)
    });
    Ok((plan, recipes))
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

fn ensure_grouping_mutation_allowed(
    batch_id: Uuid,
    state: &AppState,
) -> Result<(), String> {
    let current = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    for group in current {
        if state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(
                "grouping is locked after Reference selection; clear group references first"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn current_effective_groups(
    batch_id: Uuid,
    state: &AppState,
) -> Result<Vec<PhotoGroup>, String> {
    state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn keep_batch_moment_together(
    batch_id: String,
    group_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<PhotoGroup>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    ensure_grouping_mutation_allowed(batch_id, &state)?;
    state
        .catalog
        .keep_moment_together(batch_id, group_id)
        .map_err(|error| error.to_string())?;
    current_effective_groups(batch_id, &state)
}

#[tauri::command]
fn allow_batch_group_refinement(
    batch_id: String,
    group_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<PhotoGroup>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    ensure_grouping_mutation_allowed(batch_id, &state)?;
    state
        .catalog
        .allow_parent_refinement(batch_id, group_id)
        .map_err(|error| error.to_string())?;
    current_effective_groups(batch_id, &state)
}

#[tauri::command]
fn merge_batch_groups(
    batch_id: String,
    group_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<PhotoGroup>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    ensure_grouping_mutation_allowed(batch_id, &state)?;
    let group_ids = group_ids
        .into_iter()
        .map(|value| {
            Uuid::parse_str(&value).map_err(|error| format!("invalid group id: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    state
        .catalog
        .merge_parent_groups(batch_id, &group_ids)
        .map_err(|error| error.to_string())?;
    current_effective_groups(batch_id, &state)
}

#[tauri::command]
fn split_batch_group(
    batch_id: String,
    group_id: String,
    split_before_asset_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<PhotoGroup>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let split_before_asset_id = Uuid::parse_str(&split_before_asset_id)
        .map_err(|error| format!("invalid split asset id: {error}"))?;
    ensure_grouping_mutation_allowed(batch_id, &state)?;
    state
        .catalog
        .split_parent_group(batch_id, group_id, split_before_asset_id)
        .map_err(|error| error.to_string())?;
    current_effective_groups(batch_id, &state)
}

#[tauri::command]
fn refine_batch_groups(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<SemanticRefinementReport, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    ensure_grouping_mutation_allowed(batch_id, &state)?;

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
fn batch_workflow_status(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<WorkflowStatus, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let batch = state
        .store
        .load_batch(batch_id)
        .map_err(|error| error.to_string())?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let assets_by_id = state
        .catalog
        .list_assets()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|asset| (asset.id, asset))
        .collect::<HashMap<_, _>>();

    let mut facts = WorkflowFacts {
        preparation_active: batch
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item.status,
                    JobStatus::Pending | JobStatus::Running | JobStatus::Paused | JobStatus::Cancelled
                )
            })
            .count(),
        preparation_failed: batch
            .items
            .iter()
            .filter(|item| item.status == JobStatus::Failed)
            .count(),
        groups_total: groups.len(),
        ..WorkflowFacts::default()
    };

    for group in groups {
        let culling = build_group_culling_result(&state.analysis_cache, &group, 0.98)
            .map_err(|error| error.to_string())?;

        for asset_id in &culling.pending_asset_ids {
            if state
                .culling_reviews
                .get(*asset_id)
                .map_err(|error| error.to_string())?
                .is_none()
            {
                facts.cull_pending += 1;
            }
        }

        for recommendation in &culling.recommendations {
            let user_review = state
                .culling_reviews
                .get(recommendation.asset_id)
                .map_err(|error| error.to_string())?;
            if user_review.is_none() && recommendation.decision != CullingDecision::Keep {
                facts.cull_attention += 1;
            }
        }

        let (editable, exposure_brackets, _) = delivery_group(&group, &state)?;
        let hdr_merge_completed = state
            .bracket_merges
            .is_currently_merged(group.id, &exposure_brackets)
            .map_err(|error| error.to_string())?;
        if !exposure_brackets.is_empty() && !hdr_merge_completed {
            facts.lightroom_hdr_merge_groups += 1;
        }
        if editable.asset_ids.is_empty() {
            if exposure_brackets.is_empty() {
                facts.reference_attention_groups += 1;
                facts.lightroom_unresolved_groups += 1;
            } else if hdr_merge_completed {
                facts.lightroom_current_groups += 1;
            }
            continue;
        }

        let Some(binding) = state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
        else {
            facts.reference_attention_groups += 1;
            facts.lightroom_unresolved_groups += 1;
            continue;
        };

        if state
            .culling_reviews
            .get(binding.selected_reference_asset_id)
            .map_err(|error| error.to_string())?
            .is_some_and(|review| review.decision == CullingUserDecision::Reject)
        {
            facts.reference_attention_groups += 1;
            facts.lightroom_unresolved_groups += 1;
            continue;
        }

        let (review_plan, _) = recipe_review_preflight_for_group(group.id, &state)?;
        facts.review_attention += review_plan.attention_asset_ids.len();
        if !review_plan.pending_asset_ids.is_empty() {
            facts.review_pending_groups += 1;
        }

        let reference_set = state
            .reference_store
            .get_set(binding.reference_set_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("reference set not found: {}", binding.reference_set_id))?;

        let mut resolved = match reference_set.resolve_group_from_cache(
            &state.analysis_cache,
            &editable,
            binding.selected_reference_asset_id,
            1,
        ) {
            Ok(value) => value,
            Err(ReferenceWorkflowError::MissingExposureAnalysis(_)) => {
                facts.lightroom_unresolved_groups += 1;
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        apply_recipe_reviews(&mut resolved.recipes, &state.recipe_reviews)?;

        if resolved.recipes.is_empty() {
            facts.lightroom_unresolved_groups += 1;
            continue;
        }

        let group_assets = editable
            .asset_ids
            .iter()
            .filter_map(|asset_id| assets_by_id.get(asset_id).cloned())
            .collect::<Vec<_>>();
        if group_assets.len() != editable.asset_ids.len() {
            facts.lightroom_unresolved_groups += 1;
            continue;
        }

        let preflight = match preflight_group_sidecars(&group_assets, &resolved.recipes) {
            Ok(value) => value,
            Err(_) => {
                facts.lightroom_unresolved_groups += 1;
                continue;
            }
        };
        let conflict_count = preflight
            .iter()
            .filter(|target| {
                target.existing_sidecar.is_some() && !target.existing_matches_recipe
            })
            .count();
        let missing_count = preflight
            .iter()
            .filter(|target| target.existing_sidecar.is_none())
            .count();

        if conflict_count > 0 {
            facts.lightroom_conflict_groups += 1;
        }
        facts.lightroom_missing_sidecars += missing_count;
        if conflict_count == 0
            && missing_count == 0
            && (exposure_brackets.is_empty() || hdr_merge_completed)
        {
            facts.lightroom_current_groups += 1;
        }
    }

    Ok(derive_workflow_status(facts))
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
fn batch_latest_moment_quick_cull(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Option<MomentQuickCullOperation>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    state
        .culling_reviews
        .latest_moment_quick_cull(batch_id)
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
fn confirm_moment_quick_cull(
    batch_id: String,
    group_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<MomentQuickCullBatchResult, String> {
    if group_ids.is_empty() {
        return Err("no Moment groups selected for quick cull".to_string());
    }

    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let groups_by_id = groups
        .into_iter()
        .map(|group| (group.id, group))
        .collect::<HashMap<_, _>>();

    let mut seen_groups = HashSet::with_capacity(group_ids.len());
    let mut seen_assets = HashSet::new();
    let mut resolved_group_ids = Vec::with_capacity(group_ids.len());
    let mut reviews = Vec::new();

    for value in group_ids {
        let group_id = Uuid::parse_str(&value)
            .map_err(|error| format!("invalid group id: {error}"))?;
        if !seen_groups.insert(group_id) {
            return Err(format!("duplicate Moment quick-cull group: {group_id}"));
        }

        let group = groups_by_id
            .get(&group_id)
            .ok_or_else(|| format!("photo group is not part of batch {batch_id}: {group_id}"))?;
        if state
            .reference_store
            .group_binding(group_id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(format!(
                "group {group_id} already has a Reference; quick cull must finish before Reference"
            ));
        }
        if !state
            .culling_reviews
            .list_for_assets(&group.asset_ids)
            .map_err(|error| error.to_string())?
            .is_empty()
        {
            return Err(format!(
                "group {group_id} already has photographer Cull decisions; quick cull will not overwrite them"
            ));
        }

        let result = build_group_culling_result(&state.analysis_cache, group, 0.98)
            .map_err(|error| error.to_string())?;
        let MomentQuickCullPlan {
            keep_asset_ids,
            review_asset_ids,
            reject_asset_ids,
            ..
        } = result.moment_quick_cull.ok_or_else(|| {
            format!(
                "group {group_id} is not eligible for Moment quick cull; finish evidence or review it individually"
            )
        })?;

        for asset_id in keep_asset_ids
            .iter()
            .chain(review_asset_ids.iter())
            .chain(reject_asset_ids.iter())
        {
            if !seen_assets.insert(*asset_id) {
                return Err(format!(
                    "asset {asset_id} appears in more than one selected quick-cull group"
                ));
            }
        }

        reviews.extend(keep_asset_ids.into_iter().map(|asset_id| CullingReview {
            asset_id,
            decision: CullingUserDecision::Keep,
        }));
        reviews.extend(review_asset_ids.into_iter().map(|asset_id| CullingReview {
            asset_id,
            decision: CullingUserDecision::Review,
        }));
        reviews.extend(reject_asset_ids.into_iter().map(|asset_id| CullingReview {
            asset_id,
            decision: CullingUserDecision::Reject,
        }));
        resolved_group_ids.push(group_id);
    }

    let operation = state
        .culling_reviews
        .set_many_as_moment_quick_cull(batch_id, &resolved_group_ids, &reviews)
        .map_err(|error| error.to_string())?;

    Ok(MomentQuickCullBatchResult {
        group_ids: resolved_group_ids,
        reviews,
        operation,
    })
}

#[tauri::command]
fn undo_moment_quick_cull(
    batch_id: String,
    operation_id: String,
    state: State<'_, AppState>,
) -> Result<MomentQuickCullOperation, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let operation_id = Uuid::parse_str(&operation_id)
        .map_err(|error| format!("invalid quick-cull operation id: {error}"))?;

    let latest = state
        .culling_reviews
        .latest_moment_quick_cull(batch_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("no Moment quick-cull operation exists for batch {batch_id}"))?;
    if latest.operation_id != operation_id {
        return Err(format!(
            "only the latest Moment quick-cull operation can be undone; latest is {}",
            latest.operation_id
        ));
    }

    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let groups_by_id = groups
        .into_iter()
        .map(|group| (group.id, group))
        .collect::<HashMap<_, _>>();
    let mut operation_assets = HashSet::new();

    for group_id in &latest.group_ids {
        let group = groups_by_id.get(group_id).ok_or_else(|| {
            format!(
                "Moment quick-cull group {group_id} changed since the operation; undo is blocked"
            )
        })?;
        if state
            .reference_store
            .group_binding(*group_id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(format!(
                "group {group_id} already has a Reference; clear downstream Reference work before undoing Quick Cull"
            ));
        }
        operation_assets.extend(group.asset_ids.iter().copied());
    }

    if latest
        .reviews
        .iter()
        .any(|review| !operation_assets.contains(&review.asset_id))
    {
        return Err(
            "Moment quick-cull group membership changed since the operation; undo is blocked"
                .to_string(),
        );
    }

    state
        .culling_reviews
        .undo_moment_quick_cull(operation_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_reference_readiness(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReferenceReadinessPlan>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let mut plans = Vec::new();

    for group in groups {
        if state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            continue;
        }

        let culling = build_group_culling_result(&state.analysis_cache, &group, 0.98)
            .map_err(|error| error.to_string())?;
        let reviews = state
            .culling_reviews
            .list_for_assets(&group.asset_ids)
            .map_err(|error| error.to_string())?;
        plans.push(build_reference_readiness_plan(
            group.id,
            &group.asset_ids,
            &culling.recommendations,
            &culling.pending_asset_ids,
            &culling.exposure_brackets,
            &reviews,
        ));
    }

    Ok(plans)
}

#[tauri::command]
fn set_group_references(
    batch_id: String,
    items: Vec<ReferenceBatchItem>,
    state: State<'_, AppState>,
) -> Result<ReferenceBatchResult, String> {
    if items.is_empty() {
        return Err("no groups selected for batch Reference setup".to_string());
    }

    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let groups_by_id = groups
        .into_iter()
        .map(|group| (group.id, group))
        .collect::<HashMap<_, _>>();

    let mut requests = Vec::with_capacity(items.len());
    let mut seen_groups = HashSet::with_capacity(items.len());
    for item in items {
        let group_id = Uuid::parse_str(&item.group_id)
            .map_err(|error| format!("invalid group id: {error}"))?;
        let asset_id = Uuid::parse_str(&item.asset_id)
            .map_err(|error| format!("invalid asset id: {error}"))?;
        if !seen_groups.insert(group_id) {
            return Err(format!("duplicate Reference group: {group_id}"));
        }

        let group = groups_by_id
            .get(&group_id)
            .ok_or_else(|| format!("photo group is not part of batch {batch_id}: {group_id}"))?;
        if !group.asset_ids.contains(&asset_id) {
            return Err(format!("asset {asset_id} is not part of group {group_id}"));
        }
        if state
            .reference_store
            .group_binding(group_id)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(format!(
                "group {group_id} already has a Reference; change it individually instead"
            ));
        }

        let culling = build_group_culling_result(&state.analysis_cache, group, 0.98)
            .map_err(|error| error.to_string())?;
        let reviews = state
            .culling_reviews
            .list_for_assets(&group.asset_ids)
            .map_err(|error| error.to_string())?;
        let readiness = build_reference_readiness_plan(
            group_id,
            &group.asset_ids,
            &culling.recommendations,
            &culling.pending_asset_ids,
            &culling.exposure_brackets,
            &reviews,
        );
        match readiness.status {
            ReferenceReadinessStatus::HdrMergeFirst => {
                return Err(format!(
                    "group {group_id} contains exposure-bracket sources; merge HDR before batch Reference setup"
                ));
            }
            ReferenceReadinessStatus::NeedsCullReview => {
                return Err(format!(
                    "group {group_id} still needs Cull review before batch Reference setup"
                ));
            }
            ReferenceReadinessStatus::Ready => {}
        }
        if readiness.suggested_asset_id != Some(asset_id) {
            return Err(format!(
                "group {group_id} suggested Reference changed; refresh Reference preflight before applying the batch"
            ));
        }

        requests.push((
            group_id,
            asset_id,
            format!("Group {group_id} reference"),
        ));
    }

    let created = state
        .reference_store
        .set_single_photo_references_many(&requests)
        .map_err(|error| error.to_string())?;

    Ok(ReferenceBatchResult {
        bindings: created
            .into_iter()
            .map(|(_, binding)| binding)
            .collect(),
    })
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
fn sync_recipe_review_exception(
    group_id: String,
    source_asset_id: String,
    target_asset_ids: Vec<String>,
    fields: RecipeReviewSyncFields,
    state: State<'_, AppState>,
) -> Result<RecipeReviewSyncResult, String> {
    if !fields.any() {
        return Err("select at least one Recipe exception field to sync".to_string());
    }
    if target_asset_ids.is_empty() {
        return Err("no target photos selected for Recipe exception sync".to_string());
    }

    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let source_asset_id = Uuid::parse_str(&source_asset_id)
        .map_err(|error| format!("invalid source asset id: {error}"))?;

    let (editable, recipes) = resolve_reviewed_group_recipes(group_id, &state)?;
    let recipe_asset_ids = recipes
        .iter()
        .filter_map(|recipe| recipe.target_asset_id)
        .collect::<HashSet<_>>();
    if !editable.asset_ids.contains(&source_asset_id) || !recipe_asset_ids.contains(&source_asset_id) {
        return Err("exception source is not an editable Recipe in this group".to_string());
    }
    let source = state
        .recipe_reviews
        .get(source_asset_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "save a per-photo exception before using it as a sync source".to_string())?;

    let mut seen = HashSet::with_capacity(target_asset_ids.len());
    let mut updates = Vec::with_capacity(target_asset_ids.len());
    for value in target_asset_ids {
        let target_asset_id = Uuid::parse_str(&value)
            .map_err(|error| format!("invalid target asset id: {error}"))?;
        if target_asset_id == source_asset_id {
            return Err("exception source cannot also be a sync target".to_string());
        }
        if !seen.insert(target_asset_id) {
            return Err(format!("duplicate Recipe exception sync target: {target_asset_id}"));
        }
        if !editable.asset_ids.contains(&target_asset_id) || !recipe_asset_ids.contains(&target_asset_id) {
            return Err(format!(
                "photo {target_asset_id} is not an editable Recipe in group {group_id}"
            ));
        }

        let existing = state
            .recipe_reviews
            .get(target_asset_id)
            .map_err(|error| error.to_string())?;
        let target = existing
            .clone()
            .unwrap_or_else(|| RecipeReviewOverride::neutral(target_asset_id));
        let synced = source.copy_selected_to(&target, fields);
        if existing.as_ref() != Some(&synced) && !(existing.is_none() && synced.is_neutral()) {
            updates.push(synced);
        }
    }

    let overrides = state
        .recipe_reviews
        .set_many(&updates)
        .map_err(|error| error.to_string())?;

    Ok(RecipeReviewSyncResult {
        group_id,
        source_asset_id,
        overrides,
    })
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

fn style_sync_context_for_group(
    group: &PhotoGroup,
    state: &AppState,
) -> Result<StyleSyncGroupContext, String> {
    let culling = build_group_culling_result(&state.analysis_cache, group, 0.98)
        .map_err(|error| error.to_string())?;
    let reviews = state
        .culling_reviews
        .list_for_assets(&group.asset_ids)
        .map_err(|error| error.to_string())?;
    Ok(build_style_sync_group_context(
        group.id,
        &culling.recommendations,
        &culling.pending_asset_ids,
        &reviews,
    ))
}

#[tauri::command]
fn preflight_reference_style_sync(
    batch_id: String,
    source_group_id: String,
    state: State<'_, AppState>,
) -> Result<StyleSyncPreflight, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let source_group_id = Uuid::parse_str(&source_group_id)
        .map_err(|error| format!("invalid source group id: {error}"))?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let groups_by_id = groups
        .into_iter()
        .map(|group| (group.id, group))
        .collect::<HashMap<_, _>>();

    let source_group = groups_by_id
        .get(&source_group_id)
        .ok_or_else(|| format!("source photo group is not part of batch {batch_id}: {source_group_id}"))?;
    if state
        .reference_store
        .group_binding(source_group_id)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("select a source group Reference before syncing its look".to_string());
    }

    let source_context = style_sync_context_for_group(source_group, &state)?;
    let mut target_contexts = Vec::new();
    for group in groups_by_id.values() {
        if group.id == source_group_id {
            continue;
        }
        if state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            continue;
        }
        target_contexts.push(style_sync_context_for_group(group, &state)?);
    }

    Ok(build_style_sync_preflight(source_context, target_contexts))
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
fn copy_reference_style_to_groups(
    source_group_id: String,
    target_group_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<GroupReferenceStyleBatchResult, String> {
    if target_group_ids.is_empty() {
        return Err("no target groups selected for look reuse".to_string());
    }

    let source_group_id = Uuid::parse_str(&source_group_id)
        .map_err(|error| format!("invalid source group id: {error}"))?;
    state
        .catalog
        .find_group(source_group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("source photo group not found: {source_group_id}"))?;

    let mut target_ids = Vec::with_capacity(target_group_ids.len());
    for value in target_group_ids {
        let target_group_id = Uuid::parse_str(&value)
            .map_err(|error| format!("invalid target group id: {error}"))?;
        state
            .catalog
            .find_group(target_group_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("target photo group not found: {target_group_id}"))?;
        target_ids.push(target_group_id);
    }

    let copied = state
        .reference_store
        .copy_group_style_profile_many(source_group_id, &target_ids)
        .map_err(|error| error.to_string())?;

    Ok(GroupReferenceStyleBatchResult {
        styles: copied
            .into_iter()
            .map(|(group_id, set)| GroupReferenceStyle {
                group_id,
                reference_set_id: set.id,
                style_profile: set.style_profile,
            })
            .collect(),
    })
}

#[tauri::command]
fn batch_recipe_review_preflight(
    batch_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<RecipeReviewGroupPreflight>, String> {
    let batch_id = parse_batch_id(&batch_id)?;
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let mut plans = Vec::new();

    for group in groups {
        if state
            .reference_store
            .group_binding(group.id)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            continue;
        }
        let (plan, _) = recipe_review_preflight_for_group(group.id, &state)?;
        plans.push(plan);
    }
    Ok(plans)
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

        let (editable, exposure_brackets, _) = delivery_group(&group, &state)?;
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

        if editable.asset_ids.is_empty() && !exposure_brackets.is_empty() {
            previews.push(GroupReferencePreview {
                group_id: group.id,
                selected_reference_asset_id: binding.selected_reference_asset_id,
                recipes: Vec::new(),
                pending_asset_id: None,
                reviewed_asset_ids: Vec::new(),
                exposure_brackets,
            });
            continue;
        }
        if editable.asset_ids.is_empty() {
            continue;
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
                    exposure_brackets,
                });
            }
            Err(ReferenceWorkflowError::MissingExposureAnalysis(asset_id)) => {
                previews.push(GroupReferencePreview {
                    group_id: group.id,
                    selected_reference_asset_id: binding.selected_reference_asset_id,
                    recipes: Vec::new(),
                    pending_asset_id: Some(asset_id),
                    reviewed_asset_ids: Vec::new(),
                    exposure_brackets,
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
fn confirm_recipe_review_groups(
    group_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<RecipeReviewGroupBatchResult, String> {
    if group_ids.is_empty() {
        return Err("no clear Recipe groups selected for confirmation".to_string());
    }

    let mut seen_groups = HashSet::with_capacity(group_ids.len());
    let mut parsed_group_ids = Vec::with_capacity(group_ids.len());
    for value in group_ids {
        let group_id = Uuid::parse_str(&value)
            .map_err(|error| format!("invalid group id: {error}"))?;
        if !seen_groups.insert(group_id) {
            return Err(format!("duplicate clear Recipe group: {group_id}"));
        }
        parsed_group_ids.push(group_id);
    }

    let mut recipes_to_confirm = Vec::new();
    let mut changed_groups = Vec::new();
    for group_id in parsed_group_ids {
        let (plan, recipes) = recipe_review_preflight_for_group(group_id, &state)?;
        if plan.clear_asset_ids.is_empty()
            && plan.attention_asset_ids.is_empty()
            && plan.pending_asset_ids.is_empty()
        {
            continue;
        }
        if !plan.can_confirm_clear_group {
            return Err(format!(
                "group {group_id} still has Recipe attention or pending evidence"
            ));
        }

        let clear_ids = plan.clear_asset_ids.iter().copied().collect::<HashSet<_>>();
        let current_clear = recipes
            .into_iter()
            .filter(|recipe| {
                recipe
                    .target_asset_id
                    .is_some_and(|asset_id| clear_ids.contains(&asset_id))
            })
            .collect::<Vec<_>>();
        if current_clear.is_empty() {
            continue;
        }
        changed_groups.push(group_id);
        recipes_to_confirm.extend(current_clear);
    }

    let confirmations = state
        .recipe_reviews
        .confirm_recipes(&recipes_to_confirm)
        .map_err(|error| error.to_string())?;
    Ok(RecipeReviewGroupBatchResult {
        group_ids: changed_groups,
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
) -> Result<LightroomResolvedHandoff, String> {
    let group = state
        .catalog
        .find_group(group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("photo group not found: {group_id}"))?;
    let (editable, exposure_brackets, hdr_source_asset_ids) = delivery_group(&group, state)?;
    let hdr_merge_completed = state
        .bracket_merges
        .is_currently_merged(group.id, &exposure_brackets)
        .map_err(|error| error.to_string())?;

    if editable.asset_ids.is_empty() {
        if !hdr_source_asset_ids.is_empty() {
            return Ok(LightroomResolvedHandoff {
                assets: Vec::new(),
                recipes: Vec::new(),
                exposure_brackets,
                hdr_source_asset_ids,
                hdr_merge_completed,
            });
        }
        return Err("all photos in this group are explicitly rejected".to_string());
    }

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

    Ok(LightroomResolvedHandoff {
        assets,
        recipes: resolved.recipes,
        exposure_brackets,
        hdr_source_asset_ids,
        hdr_merge_completed,
    })
}
#[tauri::command]
fn set_group_hdr_merged(
    group_id: String,
    merged: bool,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let group = state
        .catalog
        .find_group(group_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("photo group not found: {group_id}"))?;
    let routing = route_exposure_bracket_sources(&state.analysis_cache, &group)
        .map_err(|error| error.to_string())?;
    if routing.sets.is_empty() {
        return Err("photo group has no detected exposure bracket to mark merged".to_string());
    }

    if merged {
        state
            .bracket_merges
            .mark_merged(group_id, &routing.sets)
            .map_err(|error| error.to_string())?;
    } else {
        state
            .bracket_merges
            .clear(group_id)
            .map_err(|error| error.to_string())?;
    }

    state
        .bracket_merges
        .is_currently_merged(group_id, &routing.sets)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn preflight_group_reference_xmp(
    group_id: String,
    state: State<'_, AppState>,
) -> Result<LightroomHandoffPreflight, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let resolved = resolve_lightroom_handoff(group_id, &state)?;
    let targets = if resolved.recipes.is_empty() {
        Vec::new()
    } else {
        preflight_group_sidecars(&resolved.assets, &resolved.recipes)
            .map_err(|error| error.to_string())?
    };

    Ok(LightroomHandoffPreflight {
        group_id,
        target_sidecars: targets
            .iter()
            .map(|target| target.sidecar_path.to_string_lossy().into_owned())
            .collect(),
        current_sidecars: targets
            .iter()
            .filter(|target| target.is_current())
            .filter_map(|target| {
                target
                    .existing_sidecar
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .collect(),
        missing_sidecars: targets
            .iter()
            .filter(|target| target.is_missing())
            .map(|target| target.sidecar_path.to_string_lossy().into_owned())
            .collect(),
        conflicting_sidecars: targets
            .iter()
            .filter(|target| target.is_conflict())
            .filter_map(|target| {
                target
                    .existing_sidecar
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
            })
            .collect(),
        hdr_source_asset_ids: resolved.hdr_source_asset_ids.clone(),
        hdr_merge_required: !resolved.exposure_brackets.is_empty() && !resolved.hdr_merge_completed,
        hdr_merge_completed: resolved.hdr_merge_completed,
    })
}

#[tauri::command]
fn write_group_reference_xmp(
    group_id: String,
    state: State<'_, AppState>,
) -> Result<LightroomHandoffResult, String> {
    let group_id = Uuid::parse_str(&group_id)
        .map_err(|error| format!("invalid group id: {error}"))?;
    let resolved = resolve_lightroom_handoff(group_id, &state)?;
    if resolved.recipes.is_empty() && !resolved.hdr_source_asset_ids.is_empty() {
        return Err(
            if resolved.hdr_merge_completed {
                "HDR merge is already marked complete; this source group has no standard XMP targets"
                    .to_string()
            } else {
                "HDR merge is required for this exposure-bracket source set; Photo-Cake will not write normalization XMP to the bracket RAWs"
                    .to_string()
            },
        );
    }
    let verified_sidecar_count = resolved.recipes.len();
    let written = write_group_sidecars(&resolved.assets, &resolved.recipes)
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

        let resolved = resolve_lightroom_handoff(group_id, &state)?;
        if !resolved.hdr_source_asset_ids.is_empty() && !resolved.hdr_merge_completed {
            return Err(format!(
                "group {group_id} contains pending HDR exposure-bracket sources and is excluded from safe batch handoff"
            ));
        }
        if resolved.recipes.is_empty() {
            return Err(format!(
                "group {group_id} has no standard XMP targets for batch handoff"
            ));
        }
        resolved_ids.push(group_id);
        resolved_groups.push((resolved.assets, resolved.recipes));
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
            let bracket_merges = ExposureBracketMergeStore::open(&database)?;
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
                bracket_merges,
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
            keep_batch_moment_together,
            allow_batch_group_refinement,
            merge_batch_groups,
            split_batch_group,
            batch_workflow_status,
            batch_culling,
            batch_culling_reviews,
            batch_latest_moment_quick_cull,
            set_culling_review,
            set_culling_reviews,
            confirm_moment_quick_cull,
            undo_moment_quick_cull,
            batch_reference_bindings,
            batch_reference_readiness,
            set_group_references,
            set_group_reference,
            clear_group_reference,
            batch_recipe_reviews,
            set_recipe_review,
            sync_recipe_review_exception,
            clear_recipe_review,
            batch_reference_styles,
            preflight_reference_style_sync,
            update_group_reference_style,
            copy_group_reference_style,
            copy_reference_style_to_groups,
            batch_recipe_review_preflight,
            batch_reference_previews,
            set_recipe_reviewed,
            confirm_recipe_reviews,
            confirm_recipe_review_groups,
            clear_recipe_reviewed,
            render_group_recipe_preview,
            set_group_hdr_merged,
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
