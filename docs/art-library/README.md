# CodePet source-art library

15 AI-generated concept/pose sheets, totaling 30,327,657 PNG bytes. These are source artwork, **not tested runtime animation atlases**. They are not wired into the app.

## Catalog

Paths below are relative to this directory. The [manifest](manifest.json) records SHA-256, bytes, dimensions, measured alpha, generation/edit prompts, intended pose roles and reported QA limitations for every selected image. Pose names are prompt intentions, not certified frame semantics.

| Subject | Version | Selected source sheet |
| --- | --- | --- |
| cat | v1 | [PNG](source/cat/v1/poses-alpha.png) |
| dog | v1 | [PNG](source/dog/v1/poses.png) |
| capybara | v1 | [PNG](source/capybara/v1/poses.png) |
| penguin | v1 | [PNG](source/penguin/v1/poses.png) |
| robot | v1 | [PNG](source/robot/v1/poses.png) |
| iguana | v1 | [PNG](source/iguana/v1/iguana-source.png) |
| spider | v1 | [PNG](source/spider/v1/spider-source.png) |
| snake | v1 | [PNG](source/snake/v1/snake-source.png) |
| goldfish-in-jar | v1 | [PNG](source/goldfish-in-jar/v1/goldfish-in-jar-source.png) |
| rabbit | v1 | [PNG](source/rabbit/v1/rabbit-source.png) |
| turtle | v1 | [PNG](source/turtle/v1/source-sheet.png) |
| frog | v1 | [PNG](source/frog/v1/source-sheet.png) |
| owl | v1 | [PNG](source/owl/v1/source-sheet.png) |
| red-panda | v1 | [PNG](source/red-panda/v1/source-sheet.png) |
| axolotl | v2 | [PNG](source/axolotl/v2/source-sheet.png) |

## Provenance and version selection

Created with OpenAI image generation for CodePet. Original generation/edit prompts are retained in the manifest; no third-party game asset pack is imported here. This records generation provenance, not a legal conclusion about copyright or exclusivity.

Only the 15 preferred versions are committed: cat v1 uses the previously delivered alpha edit; axolotl v2 supersedes v1. Unselected and pre-edit originals (the cat sheet before its background edit, axolotl v1) are not in this repository; their hashes and storage location were not recorded, so each `imagegen_edit` step in the manifest records `edit_source_sha256: "not in repository; hash unavailable"`. The PNGs are byte-for-byte copies, with no resampling, optimization or pixel edits performed for publication.

## QA limitations and next steps

- Pose roles describe prompts, not certified semantic conformance.
- Grid extraction, per-pose cropping, alignment, anchor normalization and runtime atlas packing are incomplete.
- Walk/crawl/swim continuity, timing and seamless loops have not been validated.
- Evolution stages and complete care-action animation sets are not included.
- No runtime integration or 96px render validation was performed.
- Alpha was measured directly. Hidden RGB may look brown in alpha-unaware previews; zero-alpha RGB is not a solid backdrop. Semi-transparent subjects and edge fringes require compositing review.
- Selected generation/edit bytes are preserved exactly. Unselected and pre-edit originals are not in this catalog and their hashes are unavailable.

Before runtime integration: inspect each pose and anatomy, crop with custom bounds where needed, normalize scale and anchors, composite against light/dark backgrounds, remove edge artifacts, register fixed props, create missing transition frames, validate loops/timing and 96px rendering, then pack a versioned atlas with explicit frame metadata. Do not assume a uniform 4×3 crop is safe.

Specific issues include spider limb/eyelid review, owl wing bounds, crowded iguana/rabbit/penguin edges, goldfish jar registration, and axolotl vignette/gill review. Several subjects have maximum alpha 254 and require opacity/compositing review.

## Integrity check

From the repository root, with Python 3:

    python3 docs/art-library/verify.py

This checks manifest structure, relative file paths, PNG headers/dimensions, sizes and SHA-256. Every check is an explicit comparison, not an `assert`, so it still runs under `python3 -O` or `PYTHONOPTIMIZE`; any mismatch is listed and the script exits 1. It does not certify pixel-level visual quality or animation readiness.

The regression test corrupts a temporary copy of the catalog and expects failure in plain, `-O`, `-OO` and `PYTHONOPTIMIZE` runs:

    python3 -I docs/art-library/test_verify.py
