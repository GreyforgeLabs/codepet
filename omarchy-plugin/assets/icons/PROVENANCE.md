# CodePet v0.3 icon set: provenance

These icons were generated for CodePet by Greyforge Labs on 2026-10-16 with OpenAI GPT Image 2 (ChatGPT Images 2.0), through the `imagegen` tool of the local Codex CLI on the owner's ChatGPT plan (no per-image billing). They are distributed under the repository's MIT licence (see `LICENSE` at the repository root), copyright (c) 2026 Greyforge Labs. No third-party imagery, icon font, logo or downloaded art is included. The app tiles are brand-neutral generic glyphs, not vendor logos or marks. The set has not been legally reviewed.

## Summary

- **Generation route:** OpenAI GPT Image 2 (ChatGPT Images 2.0) through the `imagegen` tool of the Codex CLI on the project owner's ChatGPT plan. Each request carried a unique job tag and was extracted only from the generation that carried it, then checked a second time against that generation's bytes. Every sheet was reviewed by eye before extraction.
- **Style:** painted sheets used a v0.2 pet care sheet as a style-only reference image; the bar badges were drawn separately (thick outlines, flat fills) with no reference, so they survive 16 to 20 px.
- **Shipped form:** `tools/petart/icons.py` keys the flat chroma background with the pet pipeline's `keying.py`, segments the grid by component centroid, trims each cell, centres it on a square canvas, scales it in premultiplied alpha (Lanczos) and writes WebP (quality 86, method 6, lossless alpha) to `<sheet>/<id>.webp`, plus `index.json`, into `qml/assets/icons/` and `omarchy-plugin/assets/icons/` (identical bytes). A sheet's `patches` replace single cells from a smaller sheet. Extraction is deterministic; `icons.py --check` rebuilds and byte-compares.
- **Review:** every icon was reviewed by eye on a checkerboard at 128 to 200 px and on dark and light backgrounds at 64, 40 and 20 px (bar badges at 64, 20 and 16 px). `qml/tests/test_icon_assets.py` checks sizes, alpha, margins, key residue, the two trees and this file.
- **Source sheets are not in Git.** The intended separate archive is `codepet-icon-sources-v0.3.tar`, with prompts and generation records. It was not present in the public release asset lists checked on 2026-10-18, including v0.3.0 and v0.4.0; icon source re-extraction is therefore not currently verified. Do not assume it is included in a release or its `SHA256SUMS`. Every selected sheet's sha256 is listed below and pinned in `tools/petart/icon_sheets.json`.

Style references (downscaled to 1024 px wide; sha256 of the reference image as sent):

- `style-cat-care.png`: `1062fa955e563fafb0594f46460474c0eae774fd7ab7582672df072a3d8dc83c`, from the v0.2 `cat/stage2-care.png` sheet (release asset `codepet-art-sources-v0.2.tar`).
- `style-goldfish-care.png`: `5d889db5d05e69562f036ba16e19d575a0bd41d5e085106c5c48486943b3d3ed`, from the v0.2 `goldfish-in-jar/stage2-care.png` sheet (release asset `codepet-art-sources-v0.2.tar`).
- `style-robot-care.png`: `7a2c159f15319aafed189dadf19b1e5bbc5521a5210baf641fb4b135b27c65ae`, from the v0.2 `robot/stage2-care.png` sheet (release asset `codepet-art-sources-v0.2.tar`).
- `style-apps-tiles.png`: `694987aaac211e82a0d5d39a3302860559e422debd9c6dd845e0cf8d3a58f498`, a crop of the selected `apps.a2.png` sheet holding only its four other tiles (terminal prompt, open book, perched bird, comet; 1196×390, not downscaled), used for the Claude Code tile patch.

The prompts below are the request text sent for each sheet. Each request also ended with one bookkeeping line holding its job tag ("never draw it"), and the wrapper prefixed a one-line instruction; both are kept with the generation records.

## care-floor

- Icons (4×3, 256 px): `food`, `water`, `play`, `wash`, `treat_jar`, `bed`, `sun_wake`, `wand_change`, `gear_more`, `star`, `power`, `heart`
- Selected source: attempt 1, `care-floor.a1.png`, sha256 `8460746af3d236b1d5a947b8c6ee119e96884c3dc71181334c79489e9a009cea`
- Chroma key `#00FF00`; style reference: `style-cat-care.png`; generated 2026-10-16
- Patch for `play`: attempt 1, `care-floor-play.a1.png`, sha256 `d2b787045c4b294eacfc7b259634d02668822192566557b63b6be35e20f522c0`
- Chroma key `#00FF00`; style reference: `style-cat-care.png`; generated 2026-10-16
- Patch reason: a generic red striped ball that matches bar/badge_ball, so every floor species shows one generic toy (the cat's play word is Ball, owner decision 2026-10-16); the base sheet's yarn ball is not shipped.
- Notes: Painted like the v0.2 pet props (the cat's food bowl and water bowl). The shipped `play` icon is the patch's red striped ball, the painted twin of `bar/badge_ball`, so every floor species shows one generic toy.

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
```

Patch prompt (`play`):

```text
Create a CodePet desktop pet care icon sheet.
LAYOUT: a square 1:1 (1024x1024) image holding exactly ONE object, centred, with generous empty space all around it so it never touches the image edge. Any small sparkle or bubble must touch its object.
BACKGROUND: one perfectly flat, uniform, solid pure chroma green #00FF00 filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards, tiles or panels behind the objects. Never use any green anywhere on the objects.
STYLE: premium 2D hand-painted cute game art that matches the attached reference image exactly: the same soft painterly rendering, rich but restrained shading, soft glossy highlights, clean soft edges and a warm, friendly palette, like the food bowl, water bowl, yarn ball and folding screen props painted in the reference. Slightly chunky, rounded, toy-like proportions. Use the reference ONLY for painting style: do NOT draw the animal from the reference and do not copy its layout. Each icon is one single object (no animals, no people unless a cell says so), drawn large so it fills about 70 percent of its cell, in a gentle three-quarter front view. Every icon must read clearly at 64 px: a bold, simple silhouette, strong value contrast, few small details.
CELL CONTENTS (in exact order):
(1) [row 1] a round, bouncy, glossy red toy ball with one wide white stripe band around its middle, a soft painted highlight on the top left.
```

## care-aquatic

- Icons (2×2, 256 px): `food`, `water`, `play`, `wash`
- Selected source: attempt 1, `care-aquatic.a1.png`, sha256 `c5c40d910143297d5d5e46a42cb5e05e7a5762bfc636ca3107655347930d0269`
- Chroma key `#FF00FF`; style reference: `style-goldfish-care.png`; generated 2026-10-16
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
```

## care-robot

- Icons (2×2, 256 px): `food`, `water`, `play`, `wash`
- Selected source: attempt 1, `care-robot.a1.png`, sha256 `37e5bebcc4560c8e1e62dbf07bc78855ed2bdca0c3953c20f6c5b9a9cc16f785`
- Chroma key `#00FF00`; style reference: `style-robot-care.png`; generated 2026-10-16
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
```

## status

- Icons (4×3, 256 px): `laptop`, `bubble_wait`, `bandage`, `moon`, `screen`, `leaf_calm`, `flame_hardcore`, `robot_helpers`, `help`, `memories`, `wave_goodbye`, `confetti`
- Selected source: attempt 1, `status.a1.png`, sha256 `9f6df0547c4bf3ee5f72514f4abcf05542cdfeb44c5c6384d8ab0c0fd0a74016`
- Chroma key `#FF00FF`; style reference: `style-cat-care.png`; generated 2026-10-16
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
```

## apps

- Icons (5×1, 256 px): `app_claude_code`, `app_codex`, `app_opencode`, `app_hermes`, `app_grok`
- Selected source: attempt 2, `apps.a2.png`, sha256 `990361b6a2c9d9c3db0913fc4329b07b743c038f0f9fb385914ebad686e2a7c1`
- Chroma key `#00FF00`; style reference: `style-cat-care.png`; generated 2026-10-16
- Rejected attempt 1 (sha256 `a5bededd22e2199a277ad33c230b41c8a646e1146f0fd2498d16c2c6ee34a123`): the Hermes tile's white bird in flight, wings up, read too much like a well-known social-network logo; regenerated with a perched songbird so every tile stays brand-neutral
- Patch for `app_claude_code`: attempt 1, `apps-claude-code.a1.png`, sha256 `e86b67fc0e2d0943d1a4f9600423851c1d18454e7c66e85f7677a711ecb3562a`
- Chroma key `#00FF00`; style reference: `style-apps-tiles.png`; generated 2026-10-16
- Patch reason: the base sheet's orange tile with a white four-point sparkle was too close to a vendor mark (owner review 2026-10-16); replaced by a rose-pink tile with a white light bulb, with no star, sparkle or asterisk shape and no orange.
- Notes: Brand-neutral tiles: a distinct colour and one generic white glyph each (light bulb, terminal prompt, open book, perched bird, comet). No vendor logo, mark or lettering, and no star, sparkle or asterisk shape; the app name is always shown next to the tile. The Claude Code tile is a single-cell patch drawn against a crop of the other four selected tiles as its style reference, so it keeps their shape, bevel and highlight.

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
```

Patch prompt (`app_claude_code`):

```text
Create a CodePet desktop pet app tile icon.
LAYOUT: a square 1:1 (1024x1024) image holding exactly ONE glossy rounded-square app tile, centred, seen straight from the front, about 70 percent of the image width, with generous empty space all around it so it never touches the image edge.
BACKGROUND: one perfectly flat, uniform, solid pure chroma green #00FF00 filling the entire image. No gradients, no floor, no ground line, no cast shadows, no drop shadows, no glow halos, no vignette, no text, no letters, no words, no numbers, no labels, no grid lines, no borders, no frames, no cards or panels behind the tile. Never use any green or any teal anywhere on the tile.
STYLE: match the four app tiles in the attached reference image exactly: the same rounded-square shape and corner radius, the same thick soft painted bevel, the same glossy highlight in the top left corner, the same thin darker outline of the tile colour, and the same soft painterly rendering of a chunky white symbol. Use the reference ONLY for style: do NOT copy any of its four tiles or symbols. This is a brand-neutral generic tile: no logo, no trademark, no letters. It must read clearly at 64 px and at 20 px: a bold, simple silhouette with strong contrast.
TILE: a warm rose-pink tile (raspberry pink, clearly pink, not orange, not red, not purple) with ONE simple, bold, chunky white light bulb in the middle: a round glass bulb with a short ribbed screw base, upright, with one small soft highlight on the glass. No rays, no lines around the bulb, no stars, no sparkles, no asterisks, no glints and no plus signs anywhere.
```

## ui

- Icons (2×1, 256 px): `egg`, `pencil`
- Selected source: attempt 1, `ui.a1.png`, sha256 `da25c97564cdbbc0291aec00956b9b0fe7682aec7a133a5cf10815d4c31987fd`
- Chroma key `#00FF00`; style reference: `style-cat-care.png`; generated 2026-10-16
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
```

## bar

- Icons (5×2, 64 px): `badge_hand`, `badge_drop`, `badge_bowl`, `badge_ball`, `badge_tub`, `badge_star`, `badge_bandage`, `badge_moon`, `badge_egg`, `badge_sparkle`
- Selected source: attempt 1, `bar.a1.png`, sha256 `1eee87f79e2f2f438caf4a0d21a5507533e6bc34230401186316c8dbb9e8a45b`
- Chroma key `#00FF00`; style reference: none; generated 2026-10-16
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
```
