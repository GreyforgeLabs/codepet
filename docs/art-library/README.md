# CodePet source-art library

15 AI-generated concept/pose sheets, totaling 30,327,657 PNG bytes. These are source artwork, **not tested runtime animation atlases**. They are not wired into the app.

## Catalog

Paths below are relative to this directory. The [manifest](manifest.json) records SHA-256, bytes, dimensions, measured alpha, generation/edit prompts, intended pose roles and reported QA limitations for every selected image. Every provenance step uses the tool label `built-in image_gen.imagegen` and every asset records `model: gpt-image` plus a `c2pa` block (claim generator, model label, C2PA manifest id and earliest signing time, 2026-10-15T03:15–03:22Z) read from the PNG's embedded Content Credentials. `visual_qa_notes` holds reviewer observations made after inspecting each file, not generator output. Pose names are prompt intentions, not certified frame semantics.

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

**Licence.** The project owner distributes these sheets under the repository [MIT licence](../../LICENSE), like the rest of CodePet. They are AI-generated with OpenAI image generation and include no third-party asset. Each PNG carries OpenAI-signed C2PA Content Credentials, preserved by the byte-for-byte copy policy. Copyright and distribution suitability have not been legally reviewed. The manifest's `license` field records the same statement.

Only the 15 preferred versions are committed: cat v1 uses the previously delivered alpha edit; axolotl v2 supersedes v1. Unselected and pre-edit originals (the cat sheet before its background edit, axolotl v1) are not in this repository; their hashes and storage location were not recorded, so each `imagegen_edit` step in the manifest records `edit_source_sha256: "not in repository; hash unavailable"`. The PNGs are byte-for-byte copies, with no resampling, optimization or pixel edits performed for publication.

## Storage decision

The 15 sheets (30,327,657 bytes) stay in Git as ordinary blobs, without LFS. Decided for v0.2.0: they are the canonical style anchor for CodePet's pet art in this private repository, and the v0.2.0 runtime art is generated from new sheets anchored on them. Every clone and the history carry about 30 MB that only a history rewrite could remove. The PNGs are already compressed; lossless re-encoding saves about 6 % and would break the byte-for-byte copy policy, so it is not done. The larger v0.2.0 runtime source sheets are planned to stay out of Git: the plan records their hashes in the repository and attaches the files to the release as an asset.

## QA limitations and next steps

- Pose roles describe prompts, not certified semantic conformance.
- Grid extraction, per-pose cropping, alignment, anchor normalization and runtime atlas packing are incomplete.
- Walk/crawl/swim continuity, timing and seamless loops have not been validated.
- Evolution stages and complete care-action animation sets are not included.
- No runtime integration or 96px render validation was performed.
- Alpha was measured directly. Hidden RGB may look brown in alpha-unaware previews; zero-alpha RGB is not a solid backdrop. Semi-transparent subjects and edge fringes require compositing review.
- Selected generation/edit bytes are preserved exactly. Unselected and pre-edit originals are not in this catalog and their hashes are unavailable.

Before runtime integration: inspect each pose and anatomy, crop with custom bounds where needed, normalize scale and anchors, composite against light/dark backgrounds, remove edge artifacts, register fixed props, create missing transition frames, validate loops/timing and 96px rendering, then pack a versioned atlas with explicit frame metadata. Do not assume a uniform 4×3 crop is safe.

Specific issues include spider limb/eyelid review, owl wing bounds, crowded iguana/rabbit/penguin edges, goldfish jar registration, and axolotl vignette/gill review. Five subjects (cat, dog, capybara, red-panda and axolotl) have maximum alpha 254 and no fully opaque pixels, so every subject pixel is at most 99.6% opaque; they require opacity/compositing review. Each of their manifest QA notes says so.

## Integrity check

From the repository root, with Python 3:

    python3 docs/art-library/verify.py

This checks, with the Python standard library only: manifest structure; relative paths of the form `source/<species>/<version>/<file>` that match each entry's species and version; PNG signature, IHDR dimensions, bit depth 8 and the colour type implied by `mode` (6 for RGBA); sizes and SHA-256; that each grid's roles, `columns × rows` and `pose_count` agree and sum to `intended_pose_count`; provenance on every edit step; and a whole-directory inventory, compared case-insensitively, so any unlisted file (a stray `.PNG`, `.webp` or other binary) fails. The manifest's alpha statistics are not re-verified by default; `python3 docs/art-library/verify.py --alpha` recomputes them with Pillow and fails if Pillow is missing. Every check is an explicit comparison, not an `assert`, so it still runs under `python3 -O` or `PYTHONOPTIMIZE`; any mismatch is listed and the script exits 1. It does not certify pixel-level visual quality or animation readiness.

The regression test corrupts a temporary copy of the catalog and expects failure in plain, `-O`, `-OO` and `PYTHONOPTIMIZE` runs:

    python3 -I docs/art-library/test_verify.py
