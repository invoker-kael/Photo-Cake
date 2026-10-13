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

import { defaultSimilarSelections, isSimilarTargetProtected } from "../src/similar-edit.ts";

test("batch similar defaults never overwrite individual or approved photos", () => {
  const candidate = (asset_id, recommended, individual = false, approved = false) => ({
    asset_id, group_id: "group", similarity: 0.96, recommended, in_source_group: true,
    has_individual_edit: individual, already_confirmed: approved,
  });
  const values = [
    candidate("near", true), candidate("manual", false),
    candidate("custom", true, true), candidate("approved", true, false, true),
    candidate("other", true),
  ];
  assert.deepEqual(defaultSimilarSelections(values), ["near", "other"]);
  assert.deepEqual(defaultSimilarSelections(values, 1), ["near"]);
  assert.equal(isSimilarTargetProtected(values[2], false), true);
  assert.equal(isSimilarTargetProtected(values[3], false), true);
  assert.equal(isSimilarTargetProtected(values[2], true), false);
});

import { filterSimilarCandidates } from "../src/similar-edit.ts";

test("photo filters separate recommended, optional cross-Moment and last edited", () => {
  const candidates = [
    { asset_id: "same", in_source_group: true, recommended: true },
    { asset_id: "other", in_source_group: false, recommended: true },
    { asset_id: "optional", in_source_group: false, recommended: false },
    { asset_id: "manual", in_source_group: true, recommended: false },
  ];
  assert.deepEqual(filterSimilarCandidates(candidates, "ALL"), candidates);
  assert.deepEqual(filterSimilarCandidates(candidates, "RECOMMENDED").map((c) => c.asset_id),
    ["same", "other"]);
  assert.deepEqual(filterSimilarCandidates(candidates, "CROSS_MOMENT").map((c) => c.asset_id),
    ["other", "optional"]);
  assert.deepEqual(filterSimilarCandidates(candidates, "LAST_BATCH", ["optional", "manual"]).map((c) => c.asset_id),
    ["optional", "manual"]);
});

import { readFileSync } from "node:fs";

test("Windows exposes dedicated Batch Edit navigation without adding it to companion", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
  assert.match(app, /"batch-edit"\s*\|\s*"review"/);
  assert.match(app, /\["batch-edit", "Batch Edit"\]/);
  assert.match(app, /activeView === "batch-edit" && renderBatchEdit\(\)/);
  assert.match(app, /const renderBatchEdit = \(\) =>/);
  assert.match(app, /const renderSimilarWorkspace = \(\) =>/);
  assert.match(app, /className="batch-editor-source-grid"/);
  assert.match(app, /className="batch-editor-adjust-grid"/);
  assert.match(app, /className="batch-editor-preview-pair"/);
  assert.match(app, /className="similar-edit-grid"/);
  assert.match(app, /selectSimilarSource\(sourceGroup!\.id, assetId\)/);
  assert.match(app, /onClick=\{\(\) => void applySimilarEdits\(\)\}/);
  assert.match(app, /onClick=\{\(\) => void undoSimilarEdits\(\)\}/);
  assert.match(styles, /\.batch-editor-source\.selected/);
  assert.match(styles, /\.batch-editor-adjust-row/);
});

test("Source controls expose all eight real persisted Recipe correction channels", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  for (const key of ["exposure_delta_ev", "highlights_delta", "shadows_delta",
    "whites_delta", "blacks_delta", "contrast_delta",
    "saturation_delta", "vibrance_delta"]) {
    assert.match(app, new RegExp(`\\b${key}\\b`));
  }
  assert.match(app, /adjustRecipeReview\(activeSource\.assetId, field, step\)/);
  assert.match(app, /setActiveView\("batch-edit"\)/);
});
