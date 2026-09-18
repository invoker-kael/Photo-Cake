use photo_core::{
    build_group_culling_result, AnalysisCache, AssetMetadataEvidence, Batch, BatchStore,
    CullingReview, CullingReviewStore, CullingUserDecision, GroupCullingResult,
    GroupReferenceBinding, PhotoGroup, PreviewArtifact, PreviewStore, RawAsset, RawCatalog,
    RawMetadataStore, ReferenceStore,
};
use serde::Serialize;
use std::collections::HashSet;
use tauri::{Manager, State};
use uuid::Uuid;

#[derive(Clone, Serialize)]
struct BatchPhotoContext {
    assets: Vec<RawAsset>,
    groups: Vec<PhotoGroup>,
    previews: Vec<PreviewArtifact>,
    metadata: Vec<AssetMetadataEvidence>,
}

struct AppState {
    store: BatchStore,
    catalog: RawCatalog,
    analysis_cache: AnalysisCache,
    preview_store: PreviewStore,
    metadata_store: RawMetadataStore,
    culling_reviews: CullingReviewStore,
    reference_store: ReferenceStore,
}

fn parse_batch_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|error| format!("invalid batch id: {error}"))
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
        .collect::<Vec<_>>();
    let groups = state
        .catalog
        .list_effective_groups_for_collection(batch_id)
        .map_err(|error| error.to_string())?;
    let ids = assets.iter().map(|asset| asset.id).collect::<Vec<_>>();
    let previews = state
        .preview_store
        .list_for_assets(&ids)
        .map_err(|error| error.to_string())?;
    let metadata = state
        .metadata_store
        .list_for_assets(&ids)
        .map_err(|error| error.to_string())?;

    Ok(BatchPhotoContext {
        assets,
        groups,
        previews,
        metadata,
    })
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
    let ids = batch
        .items
        .iter()
        .filter_map(|item| item.asset_id)
        .collect::<Vec<_>>();

    state
        .culling_reviews
        .list_for_assets(&ids)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database = data_dir.join("photo-cake.sqlite3");

            app.manage(AppState {
                store: BatchStore::open(&database)?,
                catalog: RawCatalog::open(&database)?,
                analysis_cache: AnalysisCache::open(&database)?,
                preview_store: PreviewStore::open(&database)?,
                metadata_store: RawMetadataStore::open(&database)?,
                culling_reviews: CullingReviewStore::open(&database)?,
                reference_store: ReferenceStore::open(&database)?,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_batches,
            batch_photo_context,
            batch_culling,
            batch_culling_reviews,
            set_culling_review,
            batch_reference_bindings,
            set_group_reference,
            clear_group_reference
        ])
        .run(tauri::generate_context!())
        .expect("error while running Photo-Cake Android");
}
