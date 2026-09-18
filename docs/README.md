# Photo-Cake Documentation

The `docs/` directory intentionally contains only five authoritative files.

Read order for Luna:

1. `LUNA_EXECUTION_SPEC.md` — execution authority and reuse rules.
2. `PRODUCT_REQUIREMENTS.md` — the user's photography workflow and success criteria.
3. `ARCHITECTURE.md` — how the current code implements that workflow.
4. `PRODUCT_ROADMAP.md` — remaining gaps and later evolution.

`README.md` is navigation only.

Do not create additional requirement/plan/workflow documents when one of these files already owns the subject. Update the existing owner instead.

Core product direction:

```text
RAW collection
 -> smart selection + meaningful groups
 -> photographer reference/style
 -> adaptive per-photo Recipes
 -> review
 -> same-basename XMP for Lightroom
      or
    explicit direct export
```

Original RAWs remain unchanged; large intermediate files are not the default.
