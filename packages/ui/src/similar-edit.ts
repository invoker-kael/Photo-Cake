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
