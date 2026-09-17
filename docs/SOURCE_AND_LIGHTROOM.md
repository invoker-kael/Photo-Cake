# Source Folder Invariant and Lightroom Handoff

## Source folders are immutable

Photo-Cake treats every imported RAW path as a read-only source reference.

During import, analysis, retouching, QA, and normal batch processing Photo-Cake MUST NOT:

- move RAW files
- rename RAW files
- copy RAW files into a managed library
- overwrite RAW files
- delete RAW files
- create XMP/ACR sidecars next to RAW files
- create thumbnails, previews, masks, checkpoints, or temporary files inside source folders

A user can therefore keep any manual folder layout such as:

```text
Photos/
├── 2026-01/
├── 2026-02/
├── 2026-05-Europe/
├── 2026-08-Family-Trip/
└── 2026-09/
```

Photo-Cake only stores references to those files.

## Managed Photo-Cake data

Application/project data lives separately:

```text
Photo-Cake data/
├── photo-cake.sqlite3
├── cache/
│   ├── thumbnails/
│   ├── previews/
│   ├── masks/
│   └── ai/
├── edits/
└── exports/
```

The physical location is platform-specific and may later be user-configurable.

## Lightroom handoff

Photo-Cake automation may finish most images, but selected images must be able to continue in Lightroom without touching the source RAW folder.

Default Lightroom handoff preset:

- TIFF
- 16 bits/channel
- ProPhoto RGB
- lossless ZIP compression
- full resolution
- metadata preserved where safe
- no output sharpening unless explicitly enabled

Default derivative name:

```text
IMG_0123.CR3
    -> IMG_0123-PC.tif
```

If that already exists:

```text
IMG_0123-PC-2.tif
IMG_0123-PC-3.tif
...
```

The handoff/output directory MUST be outside the source RAW directory by default.

Example:

```text
D:/Photos/2026-05-Europe/RAW/IMG_0123.CR3     # untouched source
D:/Photo-Cake-Output/2026-05-Europe/IMG_0123-PC.tif
```

Lightroom then imports the TIFF and continues manual edits on that derivative.

## Why TIFF rather than RAW sidecar as the default

Photo-Cake portrait operations such as blemish removal, skin retouch, geometry changes, semantic masks, and future generative repair are pixel operations that Lightroom cannot reliably reconstruct from Photo-Cake parameters alone.

Therefore the default handoff bakes Photo-Cake's result into a high-quality 16-bit TIFF while retaining the original RAW separately and untouched.

An optional XMP interoperability mode may be added later only for adjustments that map cleanly to Lightroom settings. It must remain opt-in because Adobe RAW metadata sidecars normally live next to the source asset.
