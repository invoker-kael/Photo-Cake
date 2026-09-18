# Photo-Cake Product Requirements

## Product Position

Photo-Cake is a personal semi-automatic photography post-processing assistant.

It is not a Lightroom replacement. It reduces repetitive editing work while preserving a professional RAW workflow.

Target workflow:

```text
RAW Photos
   |
   v
Import
   |
   v
Smart Culling
   |
   v
Photo Grouping
   |
   v
Reference Style Learning
   |
   v
Recipe Generation
   |
   v
Review
   |
   +-------------+
   |             |
   v             v
XMP to Lightroom  Direct Export
```

## Core Principles

- RAW files are always preserved.
- RAW + XMP is the primary workflow.
- Large intermediate files are not created by default.
- Direct JPEG/TIFF export is supported when requested.
- All editing decisions are stored as editable Recipes.
- The product is local-first.

## Photography Workflow

### 1. Smart Culling

Before editing, Photo-Cake reduces manual selection work.

Analyze:

- Focus quality
- Blur
- Closed eyes
- Expression quality
- Duplicate/burst photos
- Exposure problems
- Obvious failed shots

Output:

```text
Keep
Review
Reject suggestion
```

Culling assists the user and never deletes originals.

### 2. Photo Grouping

A photo group is the main editing unit.

Examples:

- Travel day
- Landscape
- Portrait
- Indoor family photos
- Night scenes

A Recipe applies to a group, not blindly to every photo.

### 3. Reference Style Workflow

Users can select preferred edited photos as references.

```text
Reference Photos
      |
      v
Style Analysis
      |
      v
Recipe
      |
      v
Apply to Similar Group
```

Analyze:

- Exposure style
- Color tone
- White balance
- Contrast
- Skin tone preference
- Lighting style

The goal is matching editing intent, not copying pixels.

## Lightroom Workflow

Primary output:

```text
IMG.CR3
IMG.XMP
```

Photo-Cake generates Lightroom-compatible XMP sidecars.

Do not initially implement:

- Lightroom plugin
- Lightroom catalog modification
- Lightroom database writing

## Editing Model

```text
RAW
 |
Edit Graph
 |
Recipe
 |
+--------+
|        |
XMP    Export
```

AI generates editable editing intent instead of destructive replacement.

## User Experience Goal

A user can process hundreds of RAW photos:

1. Import photos
2. Automatically select and group photos
3. Apply reference-based editing style
4. Review results
5. Continue in Lightroom or export final images
