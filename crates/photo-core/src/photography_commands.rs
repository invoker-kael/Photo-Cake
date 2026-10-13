//! Stable Photography Command IDs and validated high-level request/response contracts.
 //! Workflow execution must reuse ReferenceStore, canonical Recipe review and XMP writer.
use crate::{GroupReferenceBinding, PhotoGroup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use uuid::Uuid;

pub const PHOTOGRAPHY_COMMAND_IDS: [&str; 11] = [
    "reference.select", "recipe.preview", "recipe.confirm", "recipe.reopen",
    "lightroom.preflight", "lightroom.handoff", "reference.selectBatch",
    "recipe.confirmBatch", "recipe.confirmClearGroups",
    "lightroom.preflightBatch", "lightroom.handoffBatch",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotographyCommandId {
    ReferenceSelect, RecipePreview, RecipeConfirm, RecipeReopen,
    LightroomPreflight, LightroomHandoff, ReferenceSelectBatch,
    RecipeConfirmBatch, RecipeConfirmClearGroups,
    LightroomPreflightBatch, LightroomHandoffBatch,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
        assert_eq!(PHOTOGRAPHY_COMMAND_IDS.len(), 11);
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

}
