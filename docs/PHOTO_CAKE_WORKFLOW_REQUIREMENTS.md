# Photo-Cake Processing Workflow Requirements

## 1. Product Goal

Photo-Cake is a personal semi-automatic photography post-processing assistant.

The goal is not to replace Lightroom. The target workflow is:

RAW photos → Photo-Cake analysis and intelligent processing → Lightroom continuation or direct export.

Core principles:

- Preserve original RAW files
- Avoid unnecessary storage duplication
- Maintain professional editing flexibility
- Provide optional final export

---

# 2. Output Modes

Photo-Cake must support two output modes.

## Mode A: Lightroom Compatible Workflow (Primary)

Purpose:

Generate editing instructions without creating large image copies.

Flow:

RAW
↓
Photo-Cake Analysis
↓
Recipe Generation
↓
XMP Sidecar Generation
↓
Lightroom Classic reads XMP
↓
User continues editing

Example:

```
IMG_0001.CR3
IMG_0001.XMP
```

Requirements:

- Never modify original RAW
- Generate Adobe compatible XMP sidecar
- Store editing parameters only
- Keep storage overhead minimal

Supported XMP parameters should include:

- Exposure
- Contrast
- Highlights
- Shadows
- Whites
- Blacks
- Temperature
- Tint
- Texture
- Clarity
- Dehaze
- Vibrance
- Saturation
- Tone Curve
- HSL
- Color Grading

---

## Mode B: Direct Export Workflow (Secondary)

Purpose:

Allow users to directly obtain final images without Lightroom.

Supported output:

- JPEG
- TIFF

Requirements:

- Keep RAW untouched
- Apply recipe settings
- Preserve metadata
- Support quality settings
- Support resize settings

Do not generate large intermediate files by default.

---

# 3. Processing Pipeline

```
Import
 |
 v
Metadata Extraction
 |
 v
Photo Analysis
 |
 v
Quality Assessment
 |
 v
Grouping
 |
 v
Recipe Generation
 |
 +----------------+
 |                |
 v                v
XMP Output     Direct Export
 |
 v
Lightroom
```

---

# 4. Analysis Requirements

Analyze:

## Technical Quality

- Exposure
- Blur
- Noise
- Sharpness
- Dynamic range

## Content

- Scene type
- Human detection
- Face quality
- Closed eyes
- Similar photos

## Grouping

Support grouping by:

- Same event
- Same location
- Same lighting condition

---

# 5. Recipe System

All processing decisions must be represented as Recipe.

Example:

```json
{
 "exposure":0.35,
 "highlights":-40,
 "shadows":25,
 "temperature":600,
 "skin_tone":"warm"
}
```

Recipe must be independent from output format.

The same Recipe can generate:

- XMP
- JPEG
- TIFF

---

# 6. Lightroom Integration

Implement:

- XMP sidecar generation

Do not implement initially:

- Lightroom plugin
- Lightroom catalog modification
- Lightroom database writing

Reason:

Avoid Adobe catalog dependency and keep architecture simple.

---

# 7. Storage Design

Priority: minimum additional storage.

Preferred:

```
RAW
+
small XMP
```

Avoid by default:

```
RAW
+
TIFF
+
JPEG
+
cache
```

Large exports only happen when explicitly requested.

---

# 8. Development Priority

## Phase 1

Foundation:

- Import
- Metadata extraction
- Asset database
- Recipe schema
- XMP generation
- Direct export framework

## Phase 2

Smart processing:

- AI photo analysis
- Quality scoring
- Grouping
- Best photo selection

## Phase 3

Advanced automation:

- Reference look matching
- AI Retouch Plan
- Batch style application

## Phase 4

Advanced RAW processing:

- RAW decoder
- GPU acceleration
- Full render engine

---

# 9. Luna Execution Rules

- Implement functionality directly.
- Avoid unnecessary architecture expansion.
- Prioritize personal photography workflow.
- RAW + XMP is the default workflow.
- Direct export remains available.
- Do not generate large intermediate files by default.
- Complete each phase with tests before expanding.

Target result:

A user can import hundreds of RAW photos, let Photo-Cake analyze and generate edits, open Lightroom with XMP adjustments, or directly export finished images when needed.
