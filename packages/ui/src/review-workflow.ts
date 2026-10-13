import type {
  BackendRecipe, BackendRecipeReviewGroupPreflight,
  BackendRecipeReviewOverride, BackendStyleProfile,
} from "./batch";

// Ephemeral materialization UUIDs are intentionally not part of visual identity.
export function recipePreviewSignature(
  batchId: string | null,
  referenceAssetId: string | undefined,
  style: BackendStyleProfile | undefined,
  recipe: BackendRecipe,
  override: BackendRecipeReviewOverride | undefined,
): string {
  return JSON.stringify([
    batchId, referenceAssetId ?? null, style ?? null,
    recipe.target_asset_id, recipe.source_reference_ids, recipe.adjustments,
    override ?? null,
  ]);
}

// Unknown, clear, pending and exception Recipes are all unapproved for XMP.
export function unconfirmedDeliveryAssets(
  recipes: BackendRecipe[],
  preflight: BackendRecipeReviewGroupPreflight | undefined,
): string[] {
  const dispositions = new Map(
    (preflight?.assets ?? []).map((asset) => [asset.asset_id, asset.disposition]),
  );
  return recipes.flatMap((recipe) =>
    recipe.target_asset_id && dispositions.get(recipe.target_asset_id) !== "CONFIRMED"
      ? [recipe.target_asset_id]
      : [],
  );
}
