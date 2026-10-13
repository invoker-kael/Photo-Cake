//! Stable Photography Command IDs and validated high-level request/response contracts.
 //! Workflow execution must reuse ReferenceStore, canonical Recipe review and XMP writer.
use crate::{GroupReferenceBinding, PhotoGroup, RecipeReviewOverride, RecipeReviewSyncFields, StyleProfile, StyleSyncPreflight, CullingReview, CullingUserDecision, MomentQuickCullOperation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

pub const PHOTOGRAPHY_COMMAND_IDS: [&str; 21] = [
    "reference.select", "recipe.preview", "recipe.confirm", "recipe.reopen",
    "lightroom.preflight", "lightroom.handoff", "reference.selectBatch",
    "recipe.confirmBatch", "recipe.confirmClearGroups",
    "lightroom.preflightBatch", "lightroom.handoffBatch",
    "recipe.exceptionSet", "recipe.exceptionClear", "recipe.exceptionSync",
    "reference.stylePreflight", "reference.styleUpdate", "reference.styleCopyBatch",
    "cull.decide", "cull.confirmVisible", "cull.quickApply", "cull.quickUndo",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotographyCommandId {
    ReferenceSelect, RecipePreview, RecipeConfirm, RecipeReopen,
    LightroomPreflight, LightroomHandoff, ReferenceSelectBatch,
    RecipeConfirmBatch, RecipeConfirmClearGroups,
    LightroomPreflightBatch, LightroomHandoffBatch,
    RecipeExceptionSet, RecipeExceptionClear, RecipeExceptionSync,
    ReferenceStylePreflight, ReferenceStyleUpdate, ReferenceStyleCopyBatch,
    CullDecide, CullConfirmVisible, CullQuickApply, CullQuickUndo,
}
impl PhotographyCommandId {
    pub fn parse(id: &str) -> Result<Self, String> {
        match id {
            "reference.select" => Ok(Self::ReferenceSelect),
            "recipe.preview" => Ok(Self::RecipePreview),
            "recipe.confirm" => Ok(Self::RecipeConfirm),
            "recipe.reopen" => Ok(Self::RecipeReopen),
            "lightroom.preflight" => Ok(Self::LightroomPreflight),
            "lightroom.handoff" => Ok(Self::LightroomHandoff),
            "reference.selectBatch" => Ok(Self::ReferenceSelectBatch),
            "recipe.confirmBatch" => Ok(Self::RecipeConfirmBatch),
            "recipe.confirmClearGroups" => Ok(Self::RecipeConfirmClearGroups),
            "lightroom.preflightBatch" => Ok(Self::LightroomPreflightBatch),
            "lightroom.handoffBatch" => Ok(Self::LightroomHandoffBatch),
            "recipe.exceptionSet" => Ok(Self::RecipeExceptionSet),
            "recipe.exceptionClear" => Ok(Self::RecipeExceptionClear),
            "recipe.exceptionSync" => Ok(Self::RecipeExceptionSync),
            "reference.stylePreflight" => Ok(Self::ReferenceStylePreflight),
            "reference.styleUpdate" => Ok(Self::ReferenceStyleUpdate),
            "reference.styleCopyBatch" => Ok(Self::ReferenceStyleCopyBatch),
            "cull.decide" => Ok(Self::CullDecide),
            "cull.confirmVisible" => Ok(Self::CullConfirmVisible),
            "cull.quickApply" => Ok(Self::CullQuickApply),
            "cull.quickUndo" => Ok(Self::CullQuickUndo),
            _ => Err(format!("unknown photography command: {id}")),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReferenceSelect => "reference.select",
            Self::RecipePreview => "recipe.preview",
            Self::RecipeConfirm => "recipe.confirm",
            Self::RecipeReopen => "recipe.reopen",
            Self::LightroomPreflight => "lightroom.preflight",
            Self::LightroomHandoff => "lightroom.handoff",
            Self::ReferenceSelectBatch => "reference.selectBatch",
            Self::RecipeConfirmBatch => "recipe.confirmBatch",
            Self::RecipeConfirmClearGroups => "recipe.confirmClearGroups",
            Self::LightroomPreflightBatch => "lightroom.preflightBatch",
            Self::LightroomHandoffBatch => "lightroom.handoffBatch",
            Self::RecipeExceptionSet => "recipe.exceptionSet",
            Self::RecipeExceptionClear => "recipe.exceptionClear",
            Self::RecipeExceptionSync => "recipe.exceptionSync",
            Self::ReferenceStylePreflight => "reference.stylePreflight",
            Self::ReferenceStyleUpdate => "reference.styleUpdate",
            Self::ReferenceStyleCopyBatch => "reference.styleCopyBatch",
            Self::CullDecide => "cull.decide",
            Self::CullConfirmVisible => "cull.confirmVisible",
            Self::CullQuickApply => "cull.quickApply",
            Self::CullQuickUndo => "cull.quickUndo",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupAssetCommandRequest {
    pub batch_id: Uuid, pub group_id: Uuid, pub asset_id: Uuid,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupCommandRequest {
    pub batch_id: Uuid, pub group_id: Uuid,
}
/// A photographer-chosen group/asset pair; never a Recipe or file payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GroupAssetSelection {
    pub group_id: Uuid,
    pub asset_id: Uuid,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceSelectBatchRequest {
    pub batch_id: Uuid,
    pub selections: Vec<GroupAssetSelection>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeConfirmBatchRequest {
    pub batch_id: Uuid,
    pub selections: Vec<GroupAssetSelection>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeConfirmClearGroupsRequest {
    pub batch_id: Uuid,
    pub group_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LightroomPreflightBatchRequest {
    pub batch_id: Uuid,
    pub group_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LightroomHandoffBatchRequest {
    pub batch_id: Uuid,
    pub group_ids: Vec<Uuid>,
}


#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeExceptionDeltas {
    pub exposure_delta_ev: f32,
    pub highlights_delta: f32,
    pub shadows_delta: f32,
    pub whites_delta: f32,
    pub blacks_delta: f32,
    pub contrast_delta: f32,
    pub saturation_delta: f32,
    pub vibrance_delta: f32,
}
impl RecipeExceptionDeltas {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, bound) in [
            ("exposure_delta_ev", self.exposure_delta_ev, 3.0),
            ("highlights_delta", self.highlights_delta, 100.0),
            ("shadows_delta", self.shadows_delta, 100.0),
            ("whites_delta", self.whites_delta, 100.0),
            ("blacks_delta", self.blacks_delta, 100.0),
            ("contrast_delta", self.contrast_delta, 100.0),
            ("saturation_delta", self.saturation_delta, 100.0),
            ("vibrance_delta", self.vibrance_delta, 100.0),
        ] {
            if !value.is_finite() || value.abs() > bound {
                return Err(format!("invalid {name}: expected a finite value between -{bound} and {bound}"));
            }
        }
        Ok(())
    }
    pub fn for_asset(self, asset_id: Uuid) -> RecipeReviewOverride {
        RecipeReviewOverride {
            asset_id,
            exposure_delta_ev: self.exposure_delta_ev,
            highlights_delta: self.highlights_delta,
            shadows_delta: self.shadows_delta,
            whites_delta: self.whites_delta,
            blacks_delta: self.blacks_delta,
            contrast_delta: self.contrast_delta,
            saturation_delta: self.saturation_delta,
            vibrance_delta: self.vibrance_delta,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeExceptionSetRequest {
    pub batch_id: Uuid,
    pub group_id: Uuid,
    pub asset_id: Uuid,
    pub deltas: RecipeExceptionDeltas,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeExceptionSyncFields {
    pub exposure: bool,
    pub highlights: bool,
    pub shadows: bool,
    pub whites: bool,
    pub blacks: bool,
    pub contrast: bool,
    pub saturation: bool,
    pub vibrance: bool,
}
impl RecipeExceptionSyncFields {
    pub fn any(&self) -> bool {
        self.exposure || self.highlights || self.shadows || self.whites
            || self.blacks || self.contrast || self.saturation || self.vibrance
    }
}
impl From<RecipeExceptionSyncFields> for RecipeReviewSyncFields {
    fn from(fields: RecipeExceptionSyncFields) -> Self {
        Self {
            exposure: fields.exposure,
            highlights: fields.highlights,
            shadows: fields.shadows,
            whites: fields.whites,
            blacks: fields.blacks,
            contrast: fields.contrast,
            saturation: fields.saturation,
            vibrance: fields.vibrance,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeExceptionSyncRequest {
    pub batch_id: Uuid,
    pub group_id: Uuid,
    pub source_asset_id: Uuid,
    pub target_asset_ids: Vec<Uuid>,
    pub fields: RecipeExceptionSyncFields,
}
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceStyleUpdateRequest {
    pub batch_id: Uuid,
    pub group_id: Uuid,
    pub exposure_bias_ev: f32,
    pub contrast_preference: f32,
    pub saturation_preference: f32,
}
impl ReferenceStyleUpdateRequest {
    pub fn validate_values(&self) -> Result<(), String> {
        for (name, value, bound) in [
            ("exposure_bias_ev", self.exposure_bias_ev, 3.0),
            ("contrast_preference", self.contrast_preference, 100.0),
            ("saturation_preference", self.saturation_preference, 100.0),
        ] {
            if !value.is_finite() || value.abs() > bound {
                return Err(format!("invalid {name}: expected a finite value between -{bound} and {bound}"));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceStyleCopyBatchRequest {
    pub batch_id: Uuid,
    pub source_group_id: Uuid,
    pub target_group_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CullDecideRequest {
    pub batch_id: Uuid,
    pub group_id: Uuid,
    pub asset_id: Uuid,
    pub decision: Option<CullingUserDecision>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CullConfirmVisibleRequest {
    pub batch_id: Uuid,
    pub selections: Vec<GroupAssetSelection>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CullQuickApplyRequest {
    pub batch_id: Uuid,
    pub group_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CullQuickUndoRequest {
    pub batch_id: Uuid,
    pub operation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PhotographyCommandRequest {
    ReferenceSelect(GroupAssetCommandRequest),
    RecipePreview(GroupAssetCommandRequest),
    RecipeConfirm(GroupAssetCommandRequest),
    RecipeReopen(GroupAssetCommandRequest),
    LightroomPreflight(GroupCommandRequest),
    LightroomHandoff(GroupCommandRequest),
    ReferenceSelectBatch(ReferenceSelectBatchRequest),
    RecipeConfirmBatch(RecipeConfirmBatchRequest),
    RecipeConfirmClearGroups(RecipeConfirmClearGroupsRequest),
    LightroomPreflightBatch(LightroomPreflightBatchRequest),
    LightroomHandoffBatch(LightroomHandoffBatchRequest),
    RecipeExceptionSet(RecipeExceptionSetRequest),
    RecipeExceptionClear(GroupAssetCommandRequest),
    RecipeExceptionSync(RecipeExceptionSyncRequest),
    ReferenceStylePreflight(GroupCommandRequest),
    ReferenceStyleUpdate(ReferenceStyleUpdateRequest),
    ReferenceStyleCopyBatch(ReferenceStyleCopyBatchRequest),
    CullDecide(CullDecideRequest),
    CullConfirmVisible(CullConfirmVisibleRequest),
    CullQuickApply(CullQuickApplyRequest),
    CullQuickUndo(CullQuickUndoRequest),
}
impl PhotographyCommandRequest {
    pub fn parse(id: &str, payload: Value) -> Result<Self, String> {
        let id = PhotographyCommandId::parse(id)?;
        let invalid = |error: serde_json::Error| format!("invalid {} request: {error}", id.as_str());
        Ok(match id {
            PhotographyCommandId::ReferenceSelect =>
                Self::ReferenceSelect(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipePreview =>
                Self::RecipePreview(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipeConfirm =>
                Self::RecipeConfirm(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipeReopen =>
                Self::RecipeReopen(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::LightroomPreflight =>
                Self::LightroomPreflight(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::LightroomHandoff =>
                Self::LightroomHandoff(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::ReferenceSelectBatch =>
                Self::ReferenceSelectBatch(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipeConfirmBatch =>
                Self::RecipeConfirmBatch(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipeConfirmClearGroups =>
                Self::RecipeConfirmClearGroups(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::LightroomPreflightBatch =>
                Self::LightroomPreflightBatch(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::LightroomHandoffBatch =>
                Self::LightroomHandoffBatch(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipeExceptionSet =>
                Self::RecipeExceptionSet(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipeExceptionClear =>
                Self::RecipeExceptionClear(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::RecipeExceptionSync =>
                Self::RecipeExceptionSync(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::ReferenceStylePreflight =>
                Self::ReferenceStylePreflight(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::ReferenceStyleUpdate =>
                Self::ReferenceStyleUpdate(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::ReferenceStyleCopyBatch =>
                Self::ReferenceStyleCopyBatch(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::CullDecide =>
                Self::CullDecide(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::CullConfirmVisible =>
                Self::CullConfirmVisible(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::CullQuickApply =>
                Self::CullQuickApply(serde_json::from_value(payload).map_err(invalid)?),
            PhotographyCommandId::CullQuickUndo =>
                Self::CullQuickUndo(serde_json::from_value(payload).map_err(invalid)?),
        })
    }
    pub const fn batch_id(&self) -> Uuid {
        match self {
            Self::ReferenceSelect(r) | Self::RecipePreview(r)
            | Self::RecipeConfirm(r) | Self::RecipeReopen(r) => r.batch_id,
            Self::LightroomPreflight(r) | Self::LightroomHandoff(r) => r.batch_id,
            Self::ReferenceSelectBatch(r) => r.batch_id,
            Self::RecipeConfirmBatch(r) => r.batch_id,
            Self::RecipeConfirmClearGroups(r) => r.batch_id,
            Self::LightroomPreflightBatch(r) => r.batch_id,
            Self::LightroomHandoffBatch(r) => r.batch_id,
            Self::RecipeExceptionSet(r) => r.batch_id,
            Self::RecipeExceptionClear(r) => r.batch_id,
            Self::RecipeExceptionSync(r) => r.batch_id,
            Self::ReferenceStylePreflight(r) => r.batch_id,
            Self::ReferenceStyleUpdate(r) => r.batch_id,
            Self::ReferenceStyleCopyBatch(r) => r.batch_id,
            Self::CullDecide(r) => r.batch_id,
            Self::CullConfirmVisible(r) => r.batch_id,
            Self::CullQuickApply(r) => r.batch_id,
            Self::CullQuickUndo(r) => r.batch_id,
        }
    }
    /// effective_groups are loaded by the adapter exclusively for self.batch_id().
    /// All targets are validated before any mutable business operation begins.
    pub fn validate_target(&self, effective_groups: &[PhotoGroup]) -> Result<(), String> {
        let batch_id = self.batch_id();
        match self {
            Self::ReferenceSelect(r) | Self::RecipePreview(r)
            | Self::RecipeConfirm(r) | Self::RecipeReopen(r) => {
                validate_asset(batch_id, r.group_id, r.asset_id, effective_groups)
            }
            Self::LightroomPreflight(r) | Self::LightroomHandoff(r) =>
                validate_group(batch_id, r.group_id, effective_groups),
            Self::ReferenceSelectBatch(r) =>
                validate_selections(batch_id, &r.selections, effective_groups, true),
            Self::RecipeConfirmBatch(r) =>
                validate_selections(batch_id, &r.selections, effective_groups, false),
            Self::RecipeConfirmClearGroups(r) =>
                validate_groups(batch_id, &r.group_ids, effective_groups),
            Self::LightroomPreflightBatch(r) =>
                validate_groups(batch_id, &r.group_ids, effective_groups),
            Self::LightroomHandoffBatch(r) =>
                validate_groups(batch_id, &r.group_ids, effective_groups),
            Self::RecipeExceptionSet(r) => {
                validate_asset(batch_id, r.group_id, r.asset_id, effective_groups)?;
                r.deltas.validate()
            }
            Self::RecipeExceptionClear(r) =>
                validate_asset(batch_id, r.group_id, r.asset_id, effective_groups),
            Self::RecipeExceptionSync(r) => {
                validate_asset(batch_id, r.group_id, r.source_asset_id, effective_groups)?;
                if !r.fields.any() {
                    return Err("select at least one Recipe exception field to sync".to_string());
                }
                if r.target_asset_ids.is_empty() {
                    return Err("no target photos selected for Recipe exception sync".to_string());
                }
                let mut seen = HashSet::new();
                for target_id in &r.target_asset_ids {
                    if *target_id == r.source_asset_id || !seen.insert(*target_id) {
                        return Err(format!("duplicate or source Recipe exception target: {target_id}"));
                    }
                    validate_asset(batch_id, r.group_id, *target_id, effective_groups)?;
                }
                Ok(())
            }
            Self::ReferenceStylePreflight(r) =>
                validate_group(batch_id, r.group_id, effective_groups),
            Self::ReferenceStyleUpdate(r) => {
                validate_group(batch_id, r.group_id, effective_groups)?;
                r.validate_values()
            }
            Self::ReferenceStyleCopyBatch(r) => {
                let mut ids = vec![r.source_group_id];
                ids.extend_from_slice(&r.target_group_ids);
                if r.target_group_ids.is_empty() {
                    return Err("no target groups selected for look reuse".to_string());
                }
                validate_groups(batch_id, &ids, effective_groups)
            },
            Self::CullDecide(r) =>
                validate_asset(batch_id, r.group_id, r.asset_id, effective_groups),
            Self::CullConfirmVisible(r) =>
                validate_selections(batch_id, &r.selections, effective_groups, false),
            Self::CullQuickApply(r) =>
                validate_groups(batch_id, &r.group_ids, effective_groups),
            Self::CullQuickUndo(r) => {
                if r.operation_id.is_nil() {
                    return Err("invalid Quick Cull operation ID".to_string());
                }
                Ok(())
            },
        }
    }
}

fn validate_group(batch_id: Uuid, group_id: Uuid, groups: &[PhotoGroup]) -> Result<(), String> {
    if groups.iter().any(|group| group.id == group_id) {
        Ok(())
    } else {
        Err(format!("photo group {group_id} does not belong to batch {batch_id}"))
    }
}
fn validate_asset(
    batch_id: Uuid, group_id: Uuid, asset_id: Uuid, groups: &[PhotoGroup],
) -> Result<(), String> {
    validate_group(batch_id, group_id, groups)?;
    if groups.iter().any(|group| group.id == group_id && group.asset_ids.contains(&asset_id)) {
        Ok(())
    } else {
        Err(format!("asset {asset_id} is not part of group {group_id}"))
    }
}
fn validate_groups(batch_id: Uuid, ids: &[Uuid], groups: &[PhotoGroup]) -> Result<(), String> {
    if ids.is_empty() {
        return Err("batch command requires at least one group".to_string());
    }
    let mut seen = HashSet::new();
    let mut seen_assets = HashSet::new();
    for id in ids {
        if !seen.insert(*id) {
            return Err(format!("duplicate batch group: {id}"));
        }
        validate_group(batch_id, *id, groups)?;
        let group = groups.iter().find(|group| group.id == *id)
            .expect("validated group must be present");
        for asset_id in &group.asset_ids {
            if !seen_assets.insert(*asset_id) {
                return Err(format!("asset {asset_id} occurs in more than one batch group"));
            }
        }
    }
    Ok(())
}
fn validate_selections(
    batch_id: Uuid, items: &[GroupAssetSelection], groups: &[PhotoGroup],
    unique_groups: bool,
) -> Result<(), String> {
    if items.is_empty() {
        return Err("batch command requires at least one selected photo".to_string());
    }
    let mut seen_groups = HashSet::new();
    let mut seen_assets = HashSet::new();
    for item in items {
        if unique_groups && !seen_groups.insert(item.group_id) {
            return Err(format!("duplicate batch group: {}", item.group_id));
        }
        if !seen_assets.insert(item.asset_id) {
            return Err(format!("duplicate batch asset: {}", item.asset_id));
        }
        validate_asset(batch_id, item.group_id, item.asset_id, groups)?;
    }
    Ok(())
}

/// Each result is serializable, narrow and strongly typed before crossing IPC.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum PhotographyCommandResult {
    ReferenceSelected(GroupReferenceBinding),
    RecipePreview(RecipePreviewResult),
    RecipeConfirmation(RecipeConfirmationResult),
    LightroomPreflight(LightroomPreflightResult),
    LightroomHandoff(LightroomHandoffResult),
    ReferenceSelectBatch(ReferenceSelectBatchResult),
    RecipeConfirmBatch(RecipeConfirmBatchResult),
    RecipeConfirmClearGroups(RecipeConfirmClearGroupsResult),
    LightroomPreflightBatch(LightroomPreflightBatchResult),
    LightroomHandoffBatch(LightroomHandoffBatchResult),
    RecipeExceptionSet(RecipeReviewOverride),
    RecipeExceptionClear(RecipeExceptionClearResult),
    RecipeExceptionSync(RecipeExceptionSyncResult),
    ReferenceStylePreflight(StyleSyncPreflight),
    ReferenceStyleUpdate(ReferenceStyleResult),
    ReferenceStyleCopyBatch(ReferenceStyleBatchResult),
    CullDecide(CullDecideResult),
    CullConfirmVisible(CullConfirmVisibleResult),
    CullQuickApply(CullQuickApplyResult),
    CullQuickUndo(MomentQuickCullOperation),
}
#[derive(Debug, Clone, Serialize)]
pub struct RecipePreviewResult {
    pub asset_id: Uuid, pub recipe_id: Uuid,
    pub cache_path: String, pub process_version: u16,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecipeConfirmationResult {
    pub asset_id: Uuid, pub confirmed: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct LightroomPreflightResult {
    pub group_id: Uuid,
    pub target_sidecars: Vec<String>,
    pub current_sidecars: Vec<String>,
    pub missing_sidecars: Vec<String>,
    pub conflicting_sidecars: Vec<String>,
    pub hdr_source_asset_ids: Vec<Uuid>,
    pub hdr_merge_required: bool,
    pub hdr_merge_completed: bool,
    pub target_count: usize,
    pub current_count: usize,
    pub missing_count: usize,
    pub conflict_count: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct LightroomHandoffResult {
    pub group_id: Uuid,
    pub written_sidecars: Vec<String>,
    pub verified_sidecar_count: usize,
    pub target_count: usize,
    pub written_count: usize,
    pub already_current_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReferenceSelectBatchResult {
    pub bindings: Vec<GroupReferenceBinding>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecipeConfirmBatchResult {
    pub asset_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecipeConfirmClearGroupsResult {
    pub group_ids: Vec<Uuid>,
    pub asset_ids: Vec<Uuid>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LightroomPreflightBatchGroupResult {
    pub group_id: Uuid,
    pub preflight: Option<LightroomPreflightResult>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LightroomPreflightBatchResult {
    pub group_count: usize,
    pub target_count: usize,
    pub current_count: usize,
    pub missing_count: usize,
    pub conflict_count: usize,
    pub hdr_blocked_group_count: usize,
    pub failed_group_count: usize,
    pub group_results: Vec<LightroomPreflightBatchGroupResult>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LightroomHandoffBatchResult {
    pub groups: Vec<LightroomHandoffResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeExceptionClearResult {
    pub asset_id: Uuid,
    pub cleared: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct RecipeExceptionSyncResult {
    pub group_id: Uuid,
    pub source_asset_id: Uuid,
    pub overrides: Vec<RecipeReviewOverride>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceStyleResult {
    pub group_id: Uuid,
    pub reference_set_id: Uuid,
    pub style_profile: StyleProfile,
}
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceStyleBatchResult {
    pub styles: Vec<ReferenceStyleResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CullDecideResult {
    pub asset_id: Uuid,
    pub decision: Option<CullingUserDecision>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CullConfirmVisibleResult {
    pub reviews: Vec<CullingReview>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CullQuickApplyResult {
    pub group_ids: Vec<Uuid>,
    pub reviews: Vec<CullingReview>,
    pub operation: MomentQuickCullOperation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grouping::{GroupingBasis, PhotoGroupKind};

    fn fixture() -> (Uuid, Uuid, Uuid, Vec<PhotoGroup>) {
        let batch = Uuid::new_v4();
        let group = Uuid::new_v4();
        let asset = Uuid::new_v4();
        let groups = vec![PhotoGroup {
            id: group, kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::Singleton, asset_ids: vec![asset],
            manual_locked: false,
        }];
        (batch, group, asset, groups)
    }

    #[test]
    fn registered_commands_are_stable_and_unknown_is_rejected() {
        assert_eq!(PHOTOGRAPHY_COMMAND_IDS.len(), 21);
        for id in PHOTOGRAPHY_COMMAND_IDS {
            assert_eq!(PhotographyCommandId::parse(id).unwrap().as_str(), id);
        }
        assert!(PhotographyCommandId::parse("recipe.arbitraryEdit").is_err());
    }

    #[test]
    fn typed_payload_blocks_arbitrary_recipe_or_xmp_xml() {
        let (batch, group, asset, _) = fixture();
        let payload = serde_json::json!({"batch_id":batch,"group_id":group,"asset_id":asset});
        assert!(PhotographyCommandRequest::parse("recipe.preview", payload.clone()).is_ok());
        let mut injected = payload.as_object().unwrap().clone();
        injected.insert("recipe".into(), serde_json::json!({"adjustments":{"exposure":9999}}));
        assert!(PhotographyCommandRequest::parse("recipe.preview", Value::Object(injected)).is_err());
        assert!(PhotographyCommandRequest::parse("recipe.confirm", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":"not-a-uuid"
        })).is_err());
        assert!(PhotographyCommandRequest::parse("lightroom.handoff", serde_json::json!({
            "batch_id":batch,"group_id":group,"xmp_xml":"<xmp/>"
        })).is_err());
    }

    #[test]
    fn validates_group_and_asset_scope() {
        let (batch, group, asset, groups) = fixture();
        let command = PhotographyCommandRequest::parse("recipe.confirm", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset
        })).unwrap();
        assert!(command.validate_target(&groups).is_ok());
        assert!(command.validate_target(&[]).is_err());
        let foreign_asset = PhotographyCommandRequest::parse("recipe.reopen", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":Uuid::new_v4()
        })).unwrap();
        assert!(foreign_asset.validate_target(&groups).is_err());
        let foreign_group = PhotographyCommandRequest::parse("lightroom.preflight", serde_json::json!({
            "batch_id":batch,"group_id":Uuid::new_v4()
        })).unwrap();
        assert!(foreign_group.validate_target(&groups).is_err());
    }
    #[test]
    fn group_batch_rejects_overlapping_assets_between_distinct_groups() {
        let (batch, first, asset, mut groups) = fixture();
        let second = Uuid::new_v4();
        groups.push(PhotoGroup {
            id: second, kind: PhotoGroupKind::Moment, basis: GroupingBasis::Singleton,
            asset_ids: vec![asset], manual_locked: false,
        });
        for id in ["recipe.confirmClearGroups", "lightroom.preflightBatch", "lightroom.handoffBatch"] {
            let cmd = PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"group_ids":[first,second]
            })).unwrap();
            assert!(cmd.validate_target(&groups).is_err());
        }
    }

    #[test]
    fn batch_payloads_reject_duplicates_cross_batch_and_injected_edits() {
        let (batch, group, asset, groups) = fixture();
        let selection = serde_json::json!({"group_id":group,"asset_id":asset});
        for id in ["reference.selectBatch", "recipe.confirmBatch"] {
            let cmd = PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"selections":[selection]
            })).unwrap();
            assert!(cmd.validate_target(&groups).is_ok());
            let duplicate = PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"selections":[selection,selection]
            })).unwrap();
            assert!(duplicate.validate_target(&groups).is_err());
            assert!(PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"selections":[selection],"recipe":{"exposure":20}
            })).is_err());
            assert!(PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"selections":[{"group_id":group,"asset_id":asset,"fingerprint":"fake"}]
            })).is_err());
            let foreign = PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"selections":[{"group_id":Uuid::new_v4(),"asset_id":asset}]
            })).unwrap();
            assert!(foreign.validate_target(&groups).is_err());
        }
        for id in ["recipe.confirmClearGroups", "lightroom.preflightBatch", "lightroom.handoffBatch"] {
            let cmd = PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"group_ids":[group]
            })).unwrap();
            assert!(cmd.validate_target(&groups).is_ok());
            let duplicate = PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"group_ids":[group,group]
            })).unwrap();
            assert!(duplicate.validate_target(&groups).is_err());
            assert!(PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"group_ids":[group],"xmp_xml":"<xmp/>"
            })).is_err());
            let empty = PhotographyCommandRequest::parse(id, serde_json::json!({
                "batch_id":batch,"group_ids":[]
            })).unwrap();
            assert!(empty.validate_target(&groups).is_err());
        }
    }

    fn neutral_deltas() -> serde_json::Value {
        serde_json::json!({
            "exposure_delta_ev":0.0,"highlights_delta":0.0,"shadows_delta":0.0,
            "whites_delta":0.0,"blacks_delta":0.0,"contrast_delta":0.0,
            "saturation_delta":0.0,"vibrance_delta":0.0
        })
    }
    fn sync_fields() -> serde_json::Value {
        serde_json::json!({
            "exposure":true,"highlights":false,"shadows":false,"whites":false,
            "blacks":false,"contrast":false,"saturation":false,"vibrance":false
        })
    }

    #[test]
    fn exception_and_style_commands_validate_scope_values_and_payload() {
        let (batch, group, asset, groups) = fixture();
        let valid = PhotographyCommandRequest::parse("recipe.exceptionSet", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset,"deltas":neutral_deltas()
        })).unwrap();
        assert!(valid.validate_target(&groups).is_ok());
        let mut bad = neutral_deltas();
        bad["exposure_delta_ev"] = serde_json::json!(4.0);
        let invalid = PhotographyCommandRequest::parse("recipe.exceptionSet", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset,"deltas":bad
        })).unwrap();
        assert!(invalid.validate_target(&groups).is_err());
        assert!(PhotographyCommandRequest::parse("recipe.exceptionSet", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset,
            "deltas":neutral_deltas(),"recipe":{"exposure":999}
        })).is_err());
        assert!(PhotographyCommandRequest::parse("recipe.exceptionSet", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset,
            "deltas":{"fingerprint":"not-trusted"}
        })).is_err());
        let clear = PhotographyCommandRequest::parse("recipe.exceptionClear", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset
        })).unwrap();
        assert!(clear.validate_target(&groups).is_ok());
        assert!(clear.validate_target(&[]).is_err());
        let targets = [asset];
        for target_ids in [
            vec![], vec![targets[0]], vec![Uuid::new_v4()],
        ] {
            let command = PhotographyCommandRequest::parse("recipe.exceptionSync", serde_json::json!({
                "batch_id":batch,"group_id":group,"source_asset_id":asset,
                "target_asset_ids":target_ids,"fields":sync_fields()
            })).unwrap();
            assert!(command.validate_target(&groups).is_err());
        }
        let other = Uuid::new_v4();
        let mut expanded = groups.clone();
        expanded[0].asset_ids.push(other);
        let valid = PhotographyCommandRequest::parse("recipe.exceptionSync", serde_json::json!({
            "batch_id":batch,"group_id":group,"source_asset_id":asset,
            "target_asset_ids":[other],"fields":sync_fields()
        })).unwrap();
        assert!(valid.validate_target(&expanded).is_ok());
        let duplicate = PhotographyCommandRequest::parse("recipe.exceptionSync", serde_json::json!({
            "batch_id":batch,"group_id":group,"source_asset_id":asset,
            "target_asset_ids":[other,other],"fields":sync_fields()
        })).unwrap();
        assert!(duplicate.validate_target(&expanded).is_err());
        let invalid_fields = PhotographyCommandRequest::parse("recipe.exceptionSync", serde_json::json!({
            "batch_id":batch,"group_id":group,"source_asset_id":asset,
            "target_asset_ids":[other],"fields":{"exposure":true,"sql":"DROP"}
        }));
        assert!(invalid_fields.is_err());
        let style = PhotographyCommandRequest::parse("reference.styleUpdate", serde_json::json!({
            "batch_id":batch,"group_id":group,
            "exposure_bias_ev":0.0,"contrast_preference":40.0,"saturation_preference":0.0
        })).unwrap();
        assert!(style.validate_target(&groups).is_ok());
        let invalid = PhotographyCommandRequest::parse("reference.styleUpdate", serde_json::json!({
            "batch_id":batch,"group_id":group,
            "exposure_bias_ev":0.0,"contrast_preference":140.0,"saturation_preference":0.0
        })).unwrap();
        assert!(invalid.validate_target(&groups).is_err());
        let preflight = PhotographyCommandRequest::parse("reference.stylePreflight", serde_json::json!({
            "batch_id":batch,"group_id":group
        })).unwrap();
        assert!(preflight.validate_target(&groups).is_ok());
        let copy = PhotographyCommandRequest::parse("reference.styleCopyBatch", serde_json::json!({
            "batch_id":batch,"source_group_id":group,"target_group_ids":[group]
        })).unwrap();
        assert!(copy.validate_target(&groups).is_err());
    }

    #[test]
    fn style_copy_batch_all_or_nothing_scope_validation() {
        let (batch, source, _, mut groups) = fixture();
        let target = Uuid::new_v4();
        let other_asset = Uuid::new_v4();
        groups.push(PhotoGroup {
            id: target, kind: PhotoGroupKind::Moment,
            basis: GroupingBasis::Singleton, asset_ids: vec![other_asset],
            manual_locked: false,
        });
        let good = PhotographyCommandRequest::parse("reference.styleCopyBatch", serde_json::json!({
            "batch_id":batch,"source_group_id":source,"target_group_ids":[target]
        })).unwrap();
        assert!(good.validate_target(&groups).is_ok());
        let invalids = vec![
            vec![],
            vec![target, target],
            vec![source],
            vec![target, Uuid::new_v4()],
        ];
        for targets in invalids {
            let cmd = PhotographyCommandRequest::parse("reference.styleCopyBatch", serde_json::json!({
                "batch_id":batch,"source_group_id":source,"target_group_ids":targets
            })).unwrap();
            assert!(cmd.validate_target(&groups).is_err());
        }
        assert!(PhotographyCommandRequest::parse("reference.styleCopyBatch", serde_json::json!({
            "batch_id":batch,"source_group_id":source,"target_group_ids":[target],
            "style_profile":{"contrast_preference":300}
        })).is_err());
        let wrong_batch = PhotographyCommandRequest::parse("reference.styleCopyBatch", serde_json::json!({
            "batch_id":Uuid::new_v4(),"source_group_id":source,"target_group_ids":[target]
        })).unwrap();
        // Effective groups are supplied by the adapter for the requested batch:
        // the foreign batch must not expose either source or target group.
        assert!(wrong_batch.validate_target(&[]).is_err());
    }

    #[test]
    fn cull_requests_are_typed_and_batch_scoped() {
        let (batch, group, asset, groups) = fixture();
        for decision in [Some("KEEP"), Some("REVIEW"), Some("REJECT"), None] {
            let command = PhotographyCommandRequest::parse("cull.decide", serde_json::json!({
                "batch_id":batch,"group_id":group,"asset_id":asset,"decision":decision
            })).unwrap();
            assert!(command.validate_target(&groups).is_ok());
            assert!(command.validate_target(&[]).is_err());
        }
        assert!(PhotographyCommandRequest::parse("cull.decide", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset,"decision":"REJECT_SUGGESTION"
        })).is_err());
        assert!(PhotographyCommandRequest::parse("cull.decide", serde_json::json!({
            "batch_id":batch,"group_id":group,"asset_id":asset,"decision":"KEEP",
            "delete_raw":true
        })).is_err());
        let selection = serde_json::json!({"group_id":group,"asset_id":asset});
        let confirm = PhotographyCommandRequest::parse("cull.confirmVisible", serde_json::json!({
            "batch_id":batch,"selections":[selection]
        })).unwrap();
        assert!(confirm.validate_target(&groups).is_ok());
        let dup = PhotographyCommandRequest::parse("cull.confirmVisible", serde_json::json!({
            "batch_id":batch,"selections":[selection,selection]
        })).unwrap();
        assert!(dup.validate_target(&groups).is_err());
        assert!(PhotographyCommandRequest::parse("cull.confirmVisible", serde_json::json!({
            "batch_id":batch,"selections":[{"group_id":group,"asset_id":asset,"decision":"REJECT"}]
        })).is_err());
        let quick = PhotographyCommandRequest::parse("cull.quickApply", serde_json::json!({
            "batch_id":batch,"group_ids":[group]
        })).unwrap();
        assert!(quick.validate_target(&groups).is_ok());
        let duplicate = PhotographyCommandRequest::parse("cull.quickApply", serde_json::json!({
            "batch_id":batch,"group_ids":[group,group]
        })).unwrap();
        assert!(duplicate.validate_target(&groups).is_err());
        let undo = PhotographyCommandRequest::parse("cull.quickUndo", serde_json::json!({
            "batch_id":batch,"operation_id":Uuid::new_v4()
        })).unwrap();
        assert!(undo.validate_target(&groups).is_ok());
        let invalid = PhotographyCommandRequest::parse("cull.quickUndo", serde_json::json!({
            "batch_id":batch,"operation_id":Uuid::nil()
        })).unwrap();
        assert!(invalid.validate_target(&groups).is_err());
    }

}
