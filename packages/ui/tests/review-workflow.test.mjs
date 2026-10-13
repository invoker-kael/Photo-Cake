import assert from "node:assert/strict";
import test from "node:test";
import { recipePreviewSignature, unconfirmedDeliveryAssets } from "../src/review-workflow.ts";

const recipe = {
  id: "temporary", name: "Travel", target_asset_id: "raw-a",
  source_reference_ids: ["ref-a"],
  adjustments: { exposure: 0.3, contrast: 4, highlights: -15, shadows: 8,
    whites: 0, blacks: 0, temperature: null, tint: null,
    saturation: -2, vibrance: 5, color_mixer_saturation: null },
};
test("rematerialized UUID does not falsely invalidate preview", () => {
  assert.equal(
    recipePreviewSignature("batch", "ref-a", undefined, recipe, undefined),
    recipePreviewSignature("batch", "ref-a", undefined, { ...recipe, id: "new" }, undefined),
  );
});
test("Reference, style, Recipe, exception and project changes invalidate preview", () => {
  const before = recipePreviewSignature("batch", "ref-a", undefined, recipe, undefined);
  assert.notEqual(before, recipePreviewSignature("batch", "ref-b", undefined, recipe, undefined));
  assert.notEqual(before, recipePreviewSignature("another", "ref-a", undefined, recipe, undefined));
  assert.notEqual(before, recipePreviewSignature("batch", "ref-a", { exposure_bias_ev: 0.5 }, recipe, undefined));
  assert.notEqual(before, recipePreviewSignature("batch", "ref-a", undefined,
    { ...recipe, adjustments: { ...recipe.adjustments, highlights: -25 } }, undefined));
  assert.notEqual(before, recipePreviewSignature("batch", "ref-a", undefined, recipe,
    { asset_id: "raw-a", exposure_delta_ev: 0.1 }));
});
test("delivery blocks clear, pending, stale and unreviewed photos", () => {
  const peer = { ...recipe, target_asset_id: "raw-b" };
  assert.deepEqual(unconfirmedDeliveryAssets([recipe, peer], undefined), ["raw-a", "raw-b"]);
  assert.deepEqual(unconfirmedDeliveryAssets([recipe, peer], {
    assets: [{ asset_id: "raw-a", disposition: "CONFIRMED" },
      { asset_id: "raw-b", disposition: "CLEAR" }],
  }), ["raw-b"]);
  assert.deepEqual(unconfirmedDeliveryAssets([recipe], {
    assets: [{ asset_id: "raw-a", disposition: "NEEDS_REVIEW" }],
  }), ["raw-a"]);
  assert.deepEqual(unconfirmedDeliveryAssets([recipe], {
    assets: [{ asset_id: "raw-a", disposition: "CONFIRMED" }],
  }), []);
});
