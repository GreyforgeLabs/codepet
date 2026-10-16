# CodePet v0.3 icon set: provenance

These icons were generated for CodePet by Greyforge Labs on 2026-10-16 with OpenAI GPT Image 2 (ChatGPT Images 2.0), through the `imagegen` tool of the local Codex CLI on the owner's ChatGPT plan (no per-image billing). They are distributed under the repository's MIT licence (see `LICENSE` at the repository root), copyright (c) 2026 Greyforge Labs. No third-party imagery, icon font, logo or downloaded art is included. The app tiles are brand-neutral generic glyphs, not vendor logos or marks. The set has not been legally reviewed.

Pipeline (SPEC-v0.3 §4.2):

1. Each sheet was generated once per attempt through the concurrency-safe wrapper `~/Work/runs/codepet--2026-10-15--v0.2/tools/gen-safe.sh`, which tags the request with a unique `cpjob-…` nonce and extracts only from the Codex rollout carrying it. A second check (`icons-src/gen.sh`) required exactly one new rollout holding the request's own tag, read the wrapper nonce from it and re-extracted that rollout alone to confirm the bytes. Every sheet was then reviewed by eye before extraction.
2. Painted sheets used a v0.2 pet care sheet as a style-only reference image (`--ref`); the bar sheet used none.
3. `tools/petart/icons.py` keys the flat chroma background with the pet pipeline's `keying.py`, segments the grid by component centroid, trims each cell, centres it on a square canvas, scales it in premultiplied alpha (Lanczos) and writes WebP (quality 86, method 6, lossless alpha) to `<sheet>/<id>.webp`, plus `index.json`, into `qml/assets/icons/` and `omarchy-plugin/assets/icons/` (identical bytes). `tools/petart/icon_sheets.json` pins each selected source by sha256.
4. Every icon was reviewed by eye on a checkerboard at 128 to 200 px and on dark and light backgrounds at 64, 40 and 20 px (bar badges at 64, 20 and 16 px). `qml/tests/test_icon_assets.py` checks sizes, alpha, margins, key residue, the two trees and this file.

Source sheets, prompts, generation records and review sheets are kept outside Git in the run folder `~/Work/runs/codepet--2026-10-16--v0.3/icons-src/` (`raw/`, `prompts/`, `refs/`). The prompts below are the request text passed to the wrapper; the wrapper adds its own one-line instruction and nonce line.

Style references (sha256 of the downscaled copies in `icons-src/refs/`):

- `style-cat-care.png`: `1062fa955e563fafb0594f46460474c0eae774fd7ab7582672df072a3d8dc83c`, the v0.2 `cat/stage2-care.png` sheet scaled to 1024 px wide.
- `style-goldfish-care.png`: `5d889db5d05e69562f036ba16e19d575a0bd41d5e085106c5c48486943b3d3ed`, the v0.2 `goldfish-in-jar/stage2-care.png` sheet scaled to 1024 px wide.
- `style-robot-care.png`: `7a2c159f15319aafed189dadf19b1e5bbc5521a5210baf641fb4b135b27c65ae`, the v0.2 `robot/stage2-care.png` sheet scaled to 1024 px wide.

## care-floor

- Icons (4×3, 256 px): `food`, `water`, `play`, `wash`, `treat_jar`, `bed`, `sun_wake`, `wand_change`, `gear_more`, `star`, `power`, `heart`
- Selected source: attempt 1, `care-floor.a1.png`, sha256 `8460746af3d236b1d5a947b8c6ee119e96884c3dc71181334c79489e9a009cea`
- Chroma key `#00FF00`; style reference: `style-cat-care.png`
- Wrapper nonce `cpjob-1792132667824361107-1003086-17821`; request tag `cpicons-care-floor-a1-1792132667-24012`; Codex rollout `rollout-2026-10-16T02-37-48-01a1436e-5cef-7ad2-a81b-30fd7cf2d9a7.jsonl`; finished 2026-10-16T06:40:29+00:00
- Notes: Painted like the v0.2 pet props (the cat's food bowl, water bowl and yarn ball).

Prompt:

```text
Create a CodePet desktop pet care icon sheet.
LAYOUT: a landscape 3:2 (1536x1024) image that is an icon sheet of exactly FOUR columns by THREE rows (12 equal cells), read left to right, top to bottom. Exactly one object per cell, centred in its cell, with generous empty gutters so nothing crosses into another cell or touches the image edge. The same visual scale in every cell. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma green #00FF00 filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any green anywhere on the objects.
STYLE: premium 2D hand-painted cute game art that matches the attached reference image exactly: the same soft painterly rendering, rich but restrained shading, soft glossy highlights, clean soft edges and a warm, friendly palette, like the food bowl, water bowl, yarn ball and folding screen props painted in the reference. Slightly chunky, rounded, toy-like proportions. Use the reference ONLY for painting style: do NOT draw the animal from the reference and do not copy its layout. Each icon is one single object (no animals, no people unless a cell says so), drawn large so it fills about 70 percent of its cell, in a gentle three-quarter front view. Every icon must read clearly at 64 px: a bold, simple silhouette, strong value contrast, few small details.
CELL CONTENTS (in exact order):
(1) [row 1] a round pale-blue pet food bowl heaped high with brown kibble.
(2) [row 1] a white ceramic pet water bowl filled with clear sparkling blue water, with one big glossy blue water droplet floating just above it.
(3) [row 1] a fluffy pink yarn ball with one loose curly strand.
(4) [row 1] a small white bathtub on little gold feet, overflowing with white soap bubbles, a yellow rubber duck on top.
(5) [row 2] a round glass treat jar with a red cloth lid tied with a ribbon, full of golden star-shaped and bone-shaped cookies.
(6) [row 2] a plump round cushioned pet bed in soft lavender with a cream pillow inside.
(7) [row 2] a cheerful smiling golden-yellow sun with short rounded rays and rosy cheeks.
(8) [row 2] a magic wand: a dark blue stick with a big glossy golden star on the tip and two tiny sparkles touching the star.
(9) [row 3] one chunky rounded silver-blue cog gear with a hole in the middle.
(10) [row 3] one big plump glossy golden star.
(11) [row 3] a round glossy sky-blue push button with a white power symbol (a circle broken at the top with a short vertical line) on its face.
(12) [row 3] one plump glossy red heart.
(Request tag cpicons-care-floor-a1-1792132667-24012: bookkeeping only, never draw it.)
```

## care-aquatic

- Icons (2×2, 256 px): `food`, `water`, `play`, `wash`
- Selected source: attempt 1, `care-aquatic.a1.png`, sha256 `c5c40d910143297d5d5e46a42cb5e05e7a5762bfc636ca3107655347930d0269`
- Chroma key `#FF00FF`; style reference: `style-goldfish-care.png`
- Wrapper nonce `cpjob-1792132673830741690-1004335-31226`; request tag `cpicons-care-aquatic-a1-1792132673-19502`; Codex rollout `rollout-2026-10-16T02-41-00-01a14371-4960-7013-a094-c8405c052b5a.jsonl`; finished 2026-10-16T06:42:18+00:00
- Notes: Magenta key: the plant arch is green. The water jug is glass, so it keeps partial alpha by design.

Prompt:

```text
Create a CodePet desktop pet care icon sheet.
LAYOUT: a square 1:1 (1024x1024) image that is an icon sheet of exactly TWO columns by TWO rows (4 equal cells), read left to right, top to bottom. Exactly one object per cell, centred in its cell, with generous empty gutters so nothing crosses into another cell or touches the image edge. The same visual scale in every cell. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma magenta #FF00FF filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any pink, magenta, purple or violet anywhere on the objects.
STYLE: premium 2D hand-painted cute game art that matches the attached reference image exactly: the same soft painterly rendering, rich but restrained shading, soft glossy highlights, clean soft edges and a warm, friendly palette, like the glass fish bowl, pebbles, water plants and bubbles painted in the reference. Slightly chunky, rounded, toy-like proportions. Use the reference ONLY for painting style: do NOT draw the animal from the reference and do not copy its layout. Each icon is one single object (no animals, no people unless a cell says so), drawn large so it fills about 70 percent of its cell, in a gentle three-quarter front view. Every icon must read clearly at 64 px: a bold, simple silhouette, strong value contrast, few small details. These are care icons for a pet goldfish and a pet axolotl living in a tank.
CELL CONTENTS (in exact order):
(1) [row 1] a small round tin of fish food tipped over, with a little pile of orange, red and yellow fish flakes spilling out.
(2) [row 1] a clear glass jug pouring fresh, sparkling light-blue water, with a few round bubbles touching the stream.
(3) [row 2] an aquarium plant arch: a smooth grey stone arch with bright green leafy water plants growing over it.
(4) [row 2] an aquarium cleaning sponge on a short blue handle, with a cluster of white soap bubbles touching the sponge.
(Request tag cpicons-care-aquatic-a1-1792132673-19502: bookkeeping only, never draw it.)
```

## care-robot

- Icons (2×2, 256 px): `food`, `water`, `play`, `wash`
- Selected source: attempt 1, `care-robot.a1.png`, sha256 `37e5bebcc4560c8e1e62dbf07bc78855ed2bdca0c3953c20f6c5b9a9cc16f785`
- Chroma key `#00FF00`; style reference: `style-robot-care.png`
- Wrapper nonce `cpjob-1792132675833270297-1004418-20603`; request tag `cpicons-care-robot-a1-1792132675-14939`; Codex rollout `rollout-2026-10-16T02-40-03-01a14370-6a0b-7af3-8935-284110422724.jsonl`; finished 2026-10-16T06:41:08+00:00
- Notes: Follows the robot pet's own look (cream casing, dark navy trim, cyan lights), which is a little more outlined than the furry pets.

Prompt:

```text
Create a CodePet desktop pet care icon sheet.
LAYOUT: a square 1:1 (1024x1024) image that is an icon sheet of exactly TWO columns by TWO rows (4 equal cells), read left to right, top to bottom. Exactly one object per cell, centred in its cell, with generous empty gutters so nothing crosses into another cell or touches the image edge. The same visual scale in every cell. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma green #00FF00 filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any green anywhere on the objects.
STYLE: premium 2D hand-painted cute game art that matches the attached reference image exactly: the same soft painterly rendering, rich but restrained shading, soft glossy highlights, clean soft edges and a warm, friendly palette, like the small white-and-teal robot and its props painted in the reference. Slightly chunky, rounded, toy-like proportions. Use the reference ONLY for painting style: do NOT draw the animal from the reference and do not copy its layout. Each icon is one single object (no animals, no people unless a cell says so), drawn large so it fills about 70 percent of its cell, in a gentle three-quarter front view. Every icon must read clearly at 64 px: a bold, simple silhouette, strong value contrast, few small details. These are care icons for a cute pet robot.
CELL CONTENTS (in exact order):
(1) [row 1] a chunky energy cell battery, white casing with a glowing cyan-blue core and a yellow lightning bolt on its side.
(2) [row 1] a cute round desk cooling fan with light-blue blades and a small frosty white snowflake on its centre cap.
(3) [row 2] a red horseshoe magnet with silver tips holding up a small stack of three colourful toy blocks (blue, yellow, orange).
(4) [row 2] a round polishing brush with a wooden handle and soft white bristles, with a bright four-point shine glint touching the brush.
(Request tag cpicons-care-robot-a1-1792132675-14939: bookkeeping only, never draw it.)
```

## status

- Icons (4×3, 256 px): `laptop`, `bubble_wait`, `bandage`, `moon`, `screen`, `leaf_calm`, `flame_hardcore`, `robot_helpers`, `help`, `memories`, `wave_goodbye`, `confetti`
- Selected source: attempt 1, `status.a1.png`, sha256 `9f6df0547c4bf3ee5f72514f4abcf05542cdfeb44c5c6384d8ab0c0fd0a74016`
- Chroma key `#FF00FF`; style reference: `style-cat-care.png`
- Wrapper nonce `cpjob-1792132669828639137-1003537-6815`; request tag `cpicons-status-a1-1792132669-29898`; Codex rollout `rollout-2026-10-16T02-37-50-01a1436e-62e2-7520-bee2-ea339a0c7049.jsonl`; finished 2026-10-16T06:40:09+00:00
- Notes: Magenta key: the calm leaf is green.

Prompt:

```text
Create a CodePet desktop pet care icon sheet.
LAYOUT: a landscape 3:2 (1536x1024) image that is an icon sheet of exactly FOUR columns by THREE rows (12 equal cells), read left to right, top to bottom. Exactly one object per cell, centred in its cell, with generous empty gutters so nothing crosses into another cell or touches the image edge. The same visual scale in every cell. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma magenta #FF00FF filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any pink, magenta, purple or violet anywhere on the objects.
STYLE: premium 2D hand-painted cute game art that matches the attached reference image exactly: the same soft painterly rendering, rich but restrained shading, soft glossy highlights, clean soft edges and a warm, friendly palette, like the food bowl, water bowl, yarn ball and folding screen props painted in the reference. Slightly chunky, rounded, toy-like proportions. Use the reference ONLY for painting style: do NOT draw the animal from the reference and do not copy its layout. Each icon is one single object (no animals, no people unless a cell says so), drawn large so it fills about 70 percent of its cell, in a gentle three-quarter front view. Every icon must read clearly at 64 px: a bold, simple silhouette, strong value contrast, few small details.
CELL CONTENTS (in exact order):
(1) [row 1] a cute open laptop in silver-white with a glowing light-blue screen showing three simple rounded colour bars (no text).
(2) [row 1] a white rounded thought bubble with three big blue dots inside it in a row.
(3) [row 1] two beige adhesive bandage strips crossed in an X, with soft white pads in the middle.
(4) [row 1] a golden crescent moon with a sleepy, peaceful closed-eye face.
(5) [row 2] a small desktop computer monitor on a stand, its light-blue screen showing a big orange paw print.
(6) [row 2] one soft rounded bright green leaf with a gentle curl and a light vein.
(7) [row 2] one bold orange and red fire flame with a yellow core.
(8) [row 2] the head and shoulders of a cute round helper robot, white with sky-blue panels and a happy face on a dark visor.
(9) [row 3] a round glossy blue button with a big white question mark on its face.
(10) [row 3] a small wooden picture frame holding a sunny painted picture of a golden paw print, with a tiny red heart on the frame corner.
(11) [row 3] one friendly waving cartoon hand, warm peach-tan colour, open palm, with two short curved motion lines touching the fingers.
(12) [row 3] a party popper cone (blue and yellow stripes) bursting with red, yellow, orange and blue confetti and streamers that all stay close to the popper.
(Request tag cpicons-status-a1-1792132669-29898: bookkeeping only, never draw it.)
```

## apps

- Icons (5×1, 256 px): `app_claude_code`, `app_codex`, `app_opencode`, `app_hermes`, `app_grok`
- Selected source: attempt 2, `apps.a2.png`, sha256 `990361b6a2c9d9c3db0913fc4329b07b743c038f0f9fb385914ebad686e2a7c1`
- Chroma key `#00FF00`; style reference: `style-cat-care.png`
- Wrapper nonce `cpjob-1792132956734835707-1016066-20818`; request tag `cpicons-apps-a2-1792132956-28597`; Codex rollout `rollout-2026-10-16T02-42-36-01a14372-c323-7ca1-9582-4510bf4e6975.jsonl`; finished 2026-10-16T06:43:32+00:00
- Rejected attempt 1 (sha256 `a5bededd22e2199a277ad33c230b41c8a646e1146f0fd2498d16c2c6ee34a123`, nonce `cpjob-1792132677843285206-1004520-27217`): the Hermes tile's white bird in flight, wings up, read too much like a well-known social-network logo; regenerated with a perched songbird so every tile stays brand-neutral
- Notes: Brand-neutral tiles: a distinct colour and one generic white glyph each (spark, terminal prompt, open book, perched bird, comet). No vendor logo, mark or lettering; the app name is always shown next to the tile.

Prompt:

```text
Create a CodePet desktop pet care icon sheet.
LAYOUT: a landscape 3:2 (1536x1024) image that is an icon sheet of exactly FIVE columns by ONE row (5 equal cells), read left to right, top to bottom. Exactly one object per cell, centred in its cell, with generous empty gutters so nothing crosses into another cell or touches the image edge. The same visual scale in every cell. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma green #00FF00 filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any green or any teal anywhere on the objects.
STYLE: premium 2D hand-painted cute game art that matches the attached reference image exactly: the same soft painterly rendering, rich but restrained shading, soft glossy highlights, clean soft edges and a warm, friendly palette, like the food bowl, water bowl, yarn ball and folding screen props painted in the reference. Slightly chunky, rounded, toy-like proportions. Use the reference ONLY for painting style: do NOT draw the animal from the reference and do not copy its layout. Each icon is one glossy rounded-square app tile, seen straight from the front, about 85 percent of its cell width, with a soft painted bevel and highlight. Each tile is a different solid colour with ONE simple, bold white generic symbol in the middle. These are brand-neutral generic tiles: no logos, no trademarks, no letters. Every icon must read clearly at 64 px: a bold, simple silhouette, strong value contrast, few small details. All five tiles are the same size, sitting in one horizontal row across the middle of the image.
CELL CONTENTS (in exact order):
(1) [row 1] a warm orange tile with a white four-point sparkle star.
(2) [row 1] a dark charcoal-grey tile with a white terminal prompt symbol: a right-pointing chevron followed by a short underscore line.
(3) [row 1] a sky-blue tile with a white open book.
(4) [row 1] a golden-yellow tile with a white little bird in flight, wings up.
(5) [row 1] a deep violet tile with a white comet: a round head with a long curved tail.
CHANGE for cell (4): the bird is a chubby round little songbird PERCHED and sitting still, seen from the side, wings folded, a small tail; not flying, not a logo silhouette.
(Request tag cpicons-apps-a2-1792132956-28597: bookkeeping only, never draw it.)
```

## ui

- Icons (2×1, 256 px): `egg`, `pencil`
- Selected source: attempt 1, `ui.a1.png`, sha256 `da25c97564cdbbc0291aec00956b9b0fe7682aec7a133a5cf10815d4c31987fd`
- Chroma key `#00FF00`; style reference: `style-cat-care.png`
- Wrapper nonce `cpjob-1792132679854023180-1004616-17727`; request tag `cpicons-ui-a1-1792132679-16951`; Codex rollout `rollout-2026-10-16T02-40-10-01a14370-854e-7371-8f72-a9d9e729cd5c.jsonl`; finished 2026-10-16T06:40:59+00:00
- Notes: The first-run egg (shown at 160 px) and the name-button pencil (24 to 32 px, always inside the 48 px name button).

Prompt:

```text
Create a CodePet desktop pet care icon sheet.
LAYOUT: a landscape 3:2 (1536x1024) image that is an icon sheet of exactly TWO columns by ONE row (2 equal cells), read left to right, top to bottom. Exactly one object per cell, centred in its cell, with generous empty gutters so nothing crosses into another cell or touches the image edge. The same visual scale in every cell. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma green #00FF00 filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any green anywhere on the objects.
STYLE: premium 2D hand-painted cute game art that matches the attached reference image exactly: the same soft painterly rendering, rich but restrained shading, soft glossy highlights, clean soft edges and a warm, friendly palette, like the food bowl, water bowl, yarn ball and folding screen props painted in the reference. Slightly chunky, rounded, toy-like proportions. Use the reference ONLY for painting style: do NOT draw the animal from the reference and do not copy its layout. Each icon is one single object (no animals, no people unless a cell says so), drawn large so it fills about 70 percent of its cell, in a gentle three-quarter front view. Every icon must read clearly at 64 px: a bold, simple silhouette, strong value contrast, few small details.
CELL CONTENTS (in exact order):
(1) [row 1] a cute large pet egg standing upright: warm cream shell with soft orange and sky-blue spots, a glossy highlight, as if a baby pet will hatch from it soon.
(2) [row 1] a short chunky yellow pencil with a pink eraser and silver band, tilted diagonally, tip pointing down-left.
(Request tag cpicons-ui-a1-1792132679-16951: bookkeeping only, never draw it.)
```

## bar

- Icons (5×2, 64 px): `badge_hand`, `badge_drop`, `badge_bowl`, `badge_ball`, `badge_tub`, `badge_star`, `badge_bandage`, `badge_moon`, `badge_egg`, `badge_sparkle`
- Selected source: attempt 1, `bar.a1.png`, sha256 `1eee87f79e2f2f438caf4a0d21a5507533e6bc34230401186316c8dbb9e8a45b`
- Chroma key `#00FF00`; style reference: none
- Wrapper nonce `cpjob-1792132671830907934-1003939-15648`; request tag `cpicons-bar-a1-1792132671-16065`; Codex rollout `rollout-2026-10-16T02-37-52-01a1436e-6aa4-7ea1-9186-38061da79581.jsonl`; finished 2026-10-16T06:40:02+00:00
- Notes: Drawn separately from the painted sheets: thick dark navy outlines, flat fills, one highlight, so the badges survive 16 to 20 px. Shipped at their 64 px source size.

Prompt:

```text
Create a CodePet status-bar badge sheet.
LAYOUT: a landscape 3:2 (1536x1024) image that is an icon sheet of exactly FIVE columns by TWO rows (10 equal cells), read left to right, top to bottom. Exactly one object per cell, centred in its cell, with generous empty gutters so nothing crosses into another cell or touches the image edge. The same visual scale in every cell. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma green #00FF00 filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any green anywhere on the objects.
STYLE: bold, simplified sticker-like badge icons for a tiny 16 to 20 pixel status bar. Each icon is a chunky, very simple shape with a THICK dark navy outline (about 6 percent of the icon width), flat bright fills, at most one small white highlight, no painted texture, no gradients, no fine details, no thin lines. Shapes must stay recognisable when shrunk to 16 pixels. Each icon fills about 75 percent of its cell, seen straight from the front.
CELL CONTENTS (in exact order):
(1) [row 1] a raised open hand (palm facing us, fingers together, thumb out), sunny yellow.
(2) [row 1] a water drop, bright blue.
(3) [row 1] a pet food bowl seen from the side, orange bowl with a brown kibble mound on top.
(4) [row 1] a round ball, red with one white stripe band.
(5) [row 1] a bathtub seen from the side, white tub with three round white bubbles on top and blue water line.
(6) [row 2] a five-point star, golden yellow.
(7) [row 2] one beige adhesive bandage strip laid diagonally, with a white centre pad.
(8) [row 2] a crescent moon, pale yellow.
(9) [row 2] an upright egg, cream with two orange spots.
(10) [row 2] a four-point sparkle star, bright yellow.
(Request tag cpicons-bar-a1-1792132671-16065: bookkeeping only, never draw it.)
```
