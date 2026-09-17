# Photo-Cake finishing workflow

Photo-Cake is the primary batch finishing stage. Lightroom Classic and Photoshop are optional downstream tools, not mandatory steps.

## Default route

```text
RAW
  -> Photo-Cake batch analysis / correction / retouch
       -> Direct export                    (default for most photos)
       -> RAW + Adobe sidecar -> Lightroom (small manual refinement)
       -> 16-bit TIFF -> Photoshop         (rare pixel-level refinement)
```

The product should optimize for finishing the majority of a shoot inside Photo-Cake.

## Source-folder invariant

Photo-Cake must never reorganize source media.

During import and normal processing it must not:

- move RAW files
- rename RAW files
- copy RAW files into a managed library
- overwrite or delete RAW files
- create sidecars in the RAW folder unless the user explicitly requests a Lightroom handoff

The user's monthly / trip / event directory structure remains authoritative.

Project state, previews, masks, checkpoints, edit recipes and caches live in Photo-Cake application/project storage.

## Route 1: Direct Photo-Cake export

This is the normal completion path.

A photo that is satisfactory after batch correction should be exported directly without requiring Lightroom or Photoshop.

Export recipes are independent from the original RAW directory and must never overwrite source files.

## Route 2: Lightroom Classic handoff

For small manual refinements, prefer keeping the original RAW editable.

When the current Photo-Cake edit set is known to round-trip through Adobe Camera Raw/Lightroom metadata, Photo-Cake may create a same-basename XMP sidecar only after an explicit `Send to Lightroom` / `Write Sidecar` action.

Example:

```text
2026-09-Trip/
  IMG_0123.CR3
  IMG_0123.xmp
```

Existing sidecars must never be silently overwritten.

### Lightroom Classic 15.x note

Adobe documents that Lightroom Classic 15.0 and later may create an additional `.acr` sidecar for heavy edits, masks and AI settings while keeping the XMP sidecar lighter.

Photo-Cake must therefore treat Adobe compatibility as versioned behavior:

- simple verified Develop settings may use XMP-native handoff
- masks/heavy edits that Lightroom expects in an ACR sidecar are not written by Photo-Cake until a real round-trip compatible ACR implementation has been verified
- unsupported Photo-Cake masks remain internal or use rendered fallback

References:

- Adobe: Save metadata to external sidecar files — https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/create-xmp-acr-files.html
- Adobe: Metadata basics and actions — https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/metadata-basics-actions.html

## Route 3: Photoshop / rendered fallback

Pixel-changing operations cannot be represented faithfully as ordinary RAW Develop metadata.

Examples include:

- pixel-level blemish repair
- liquify / geometry warp
- inpainting or generative cleanup
- unsupported semantic-mask effects

These use a rendered master outside the RAW source directory.

Default master:

- TIFF
- 16-bit/channel
- ProPhoto RGB
- ZIP lossless compression
- full resolution
- no output sharpening by default

Adobe currently recommends 16-bit ProPhoto RGB when maximizing color detail for external editing from Lightroom Classic.

Reference:

- Adobe: External Editing preferences — https://helpx.adobe.com/lightroom-classic/desktop/work-with-external-editors/external-editing-preferences.html

## Current implementation policy

The routing contract may plan XMP-native handoff, but it does not yet write Adobe XMP/ACR payloads.

ACR writing stays disabled until tested against current Lightroom Classic round-trip behavior.

This prevents Photo-Cake from producing sidecars that look plausible but are not reliably understood by Lightroom.
