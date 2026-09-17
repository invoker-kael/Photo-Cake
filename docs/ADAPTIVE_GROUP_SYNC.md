# Adaptive group color sync

Photo-Cake does not require the user to manually grade one image before a group can be processed.

## Default: AUTO_GROUP

After RAW import, preview analysis produces per-photo exposure, white-balance and visual-analysis artifacts. Photo-Cake derives a robust group target from the group itself and chooses a representative reference candidate automatically.

The target is stored separately from the values resolved for each photo.

Example:

- target exposure: +0.2 EV
- target temperature: 5600 K
- photo A currently -0.8 EV -> resolved correction +1.0 EV
- photo B currently +0.6 EV -> resolved correction -0.4 EV

Both photos approach the same visual target without copying identical slider values.

## Optional: REFERENCE_DRIVEN

The user may edit any group member and promote it to Group Reference. The edited reference defines the new group target/look. Photo-Cake then recalculates each member independently toward that target.

Changing a reference invalidates only the group color-sync revision. It does not re-import RAW files or invalidate unrelated preview, classification, embedding or grouping evidence.

## Explicit: MANUAL_COPY

Exact numeric parameter copying remains available for deterministic use cases, but it is never the default. It deliberately copies the selected resolved values to every group member.

## Semantic/local intent

Global color intent and semantic/local intent are stored separately. Person, face-skin, background and sky adjustments can therefore be adapted independently when compatible masks are available.

## Persistence

The complete group sync plan is serializable and stored in SQLite, including:

- group ID
- mode
- optional reference asset ID
- target/look descriptor
- revision
- each photo's resolved correction

This lets a group resume after restart without recomputing completed work when its dependencies are unchanged.

## Source safety

Group synchronization never changes, moves, renames or rewrites source RAW files. All state remains in Photo-Cake project storage until an explicit export or sidecar handoff is requested.
