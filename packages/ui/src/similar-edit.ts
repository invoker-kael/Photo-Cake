import type { BackendSimilarEditCandidate } from "./batch";

export function isSimilarTargetProtected(
  candidate: BackendSimilarEditCandidate, allowOverwrite: boolean,
): boolean {
  return !allowOverwrite && (candidate.has_individual_edit || candidate.already_confirmed);
}

export function defaultSimilarSelections(
  candidates: BackendSimilarEditCandidate[], maximum = 1000,
): string[] {
  return candidates.filter((candidate) => candidate.recommended &&
    !isSimilarTargetProtected(candidate, false))
    .slice(0, maximum).map((candidate) => candidate.asset_id);
}

export type SimilarPhotoView = "ALL" | "RECOMMENDED" | "CROSS_MOMENT" | "LAST_BATCH";

export function filterSimilarCandidates(
  candidates: BackendSimilarEditCandidate[],
  view: SimilarPhotoView,
  lastEditedAssetIds: readonly string[] = [],
): BackendSimilarEditCandidate[] {
  if (view === "ALL") return candidates;
  const changed = new Set(lastEditedAssetIds);
  return candidates.filter((candidate) =>
    view === "RECOMMENDED" ? candidate.recommended
      : view === "CROSS_MOMENT" ? !candidate.in_source_group
      : changed.has(candidate.asset_id),
  );
}
