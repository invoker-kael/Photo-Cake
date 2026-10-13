//! Stable Photography Command IDs and validated high-level request/response contracts.
 //! Workflow execution must reuse ReferenceStore, canonical Recipe review and XMP writer.
use crate::{GroupReferenceBinding, PhotoGroup};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const PHOTOGRAPHY_COMMAND_IDS: [&str; 6] = [
    "reference.select", "recipe.preview", "recipe.confirm", "recipe.reopen",
    "lightroom.preflight", "lightroom.handoff",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotographyCommandId {
    ReferenceSelect, RecipePreview, RecipeConfirm, RecipeReopen,
    LightroomPreflight, LightroomHandoff,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhotographyCommandRequest {
    ReferenceSelect(GroupAssetCommandRequest),
    RecipePreview(GroupAssetCommandRequest),
    RecipeConfirm(GroupAssetCommandRequest),
    RecipeReopen(GroupAssetCommandRequest),
    LightroomPreflight(GroupCommandRequest),
    LightroomHandoff(GroupCommandRequest),
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
        })
    }
    pub const fn batch_id(&self) -> Uuid {
        match self {
            Self::ReferenceSelect(r) | Self::RecipePreview(r)
            | Self::RecipeConfirm(r) | Self::RecipeReopen(r) => r.batch_id,
            Self::LightroomPreflight(r) | Self::LightroomHandoff(r) => r.batch_id,
        }
    }
    pub const fn group_id(&self) -> Uuid {
        match self {
            Self::ReferenceSelect(r) | Self::RecipePreview(r)
            | Self::RecipeConfirm(r) | Self::RecipeReopen(r) => r.group_id,
            Self::LightroomPreflight(r) | Self::LightroomHandoff(r) => r.group_id,
        }
    }
    pub const fn asset_id(&self) -> Option<Uuid> {
        match self {
            Self::ReferenceSelect(r) | Self::RecipePreview(r)
            | Self::RecipeConfirm(r) | Self::RecipeReopen(r) => Some(r.asset_id),
            Self::LightroomPreflight(_) | Self::LightroomHandoff(_) => None,
        }
    }
    /// effective_groups must be loaded for the supplied batch by the adapter.
    pub fn validate_target(&self, effective_groups: &[PhotoGroup]) -> Result<(), String> {
        let group_id = self.group_id();
        let group = effective_groups.iter().find(|g| g.id == group_id)
            .ok_or_else(|| format!("photo group {group_id} does not belong to batch {}", self.batch_id()))?;
        if let Some(asset_id) = self.asset_id() {
            if !group.asset_ids.contains(&asset_id) {
                return Err(format!("asset {asset_id} is not part of group {group_id}"));
            }
        }
        Ok(())
    }
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
        assert_eq!(PHOTOGRAPHY_COMMAND_IDS.len(), 6);
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
}
