# Photo-Cake Photography Workflow

## Goal

Photo-Cake follows a photographer-first workflow.

The target is not replacing Lightroom. The target is reducing repetitive work while keeping RAW ownership and Lightroom compatibility.

## Primary Workflow

```text
RAW Files
  |
  v
Import
  |
  v
Catalog
  |
  v
Preview + Metadata
  |
  v
Smart Culling
  |
  v
Photo Group
  |
  v
Reference Photos
  |
  v
Recipe Generation
  |
  v
Review
  |
  +----------------+
  |                |
  v                v
 XMP            Direct Export
 Lightroom
```

## Storage Strategy

Preferred:

```text
RAW + XMP
```

Avoid:

- unnecessary TIFF intermediates
- duplicate full-size working copies
- cloud upload dependency

## Editing Model

Recipe is the single editing decision model.

Example:

```text
Reference Style
      |
      v
Recipe
      |
      v
Photo Group
      |
      v
XMP / Export
```

## AI Usage

AI assists decisions:

- selecting photos
- grouping scenes
- understanding style
- suggesting adjustments

AI does not:

- modify RAW files
- silently delete photos
- replace photographer review

## Platform

Windows:

- main RAW processing workstation
- batch processing
- Lightroom workflow

Android:

- selection assistant
- reference photo management
- preview

Both platforms share the same core workflow model.
