"""CodePet v0.2 companion roster: the binding art brief data (SPEC §11.5, Amendment A4).

Pure data. `extract.py` merges this with each species' `provenance.json` to write the
manifests. Display names, personalities, care labels and fallback stage names come from
the spec; stage names in a species' `provenance.json` take precedence when present.
"""

ASSET_REVISION = "0.2.0"
SCHEMA_VERSION = 1

# Canonical order (= codepet_core::species::SPECIES_IDS).
ORDER = ("cat", "dog", "capybara", "penguin", "robot", "iguana", "spider", "snake",
         "goldfish", "rabbit", "turtle", "frog", "owl", "red_panda", "axolotl")

# Source directory names under the art root that differ from the species id.
SOURCE_DIRS = {"goldfish": "goldfish-in-jar", "red_panda": "red-panda"}

STATE_NAMES = ("idle", "locomotion", "food", "water", "play", "wash", "bathroom",
               "sleep_enter", "sleep", "sleep_exit", "celebrate", "working", "waiting",
               "needs_care", "error")
CARE_STATES = ("food", "water", "play", "wash", "bathroom")

STAGE_KEYS = ("young", "grown", "mature")
STAGE_MIN_LEVEL = (1, 2, 4)              # = RULES_V1.stage_min_level (tests read lifecycle.rs)
STAGE_FACTORS = (0.82, 0.92, 1.0)        # Amendment A3: on top of the per-stage fit scale

# Canvas (Amendment A3).
CANVAS = 192
GROUND_Y = 184
MARGIN = 2

# Amendment A4 state table: frames, frameMs, mode, reducedFrame; priority and interrupt
# from §11.2. `mode` is loop | once | hold | while_moving.
STATES = {
    "idle":        dict(frames=["b00", "b00", "b01", "b00"], frameMs=600, mode="loop", reducedFrame=0, priority=0, interrupt="any"),
    "locomotion":  dict(frames=["b08", "b09", "b10", "b11"], frameMs=200, mode="while_moving", reducedFrame=0, priority=20, interrupt="any"),
    "food":        dict(frames=["b03", "c00", "c01", "c00", "c01", "b04"], frameMs=400, mode="once", reducedFrame=1, priority=70, interrupt="higher"),
    "water":       dict(frames=["b03", "c02", "c03", "c02", "c03", "b04"], frameMs=400, mode="once", reducedFrame=1, priority=70, interrupt="higher"),
    "play":        dict(frames=["b03", "c04", "c05", "c04", "c05", "b04"], frameMs=400, mode="once", reducedFrame=1, priority=70, interrupt="higher"),
    "wash":        dict(frames=["b00", "c06", "c07", "c06", "c07", "b04"], frameMs=400, mode="once", reducedFrame=1, priority=70, interrupt="higher"),
    "bathroom":    dict(frames=["b03", "c08", "c08", "b00"], frameMs=600, mode="once", reducedFrame=1, priority=70, interrupt="higher"),
    "sleep_enter": dict(frames=["c09", "b07"], frameMs=600, mode="once", reducedFrame=1, priority=90, interrupt="none"),
    "sleep":       dict(frames=["b07"], frameMs=1000, mode="loop", reducedFrame=0, priority=85, interrupt="higher",
                        breathing={"maxScale": 0.02, "maxUpdatesPerSecond": 1, "reducedMotion": False}),
    "sleep_exit":  dict(frames=["c10", "b00"], frameMs=600, mode="once", reducedFrame=1, priority=90, interrupt="none"),
    "celebrate":   dict(frames=["c11", "b04", "c11", "b04"], frameMs=400, mode="once", reducedFrame=0, priority=60, interrupt="higher"),
    "working":     dict(frames=["b04", "b00"], frameMs=800, mode="loop", reducedFrame=0, priority=40, interrupt="any"),
    "waiting":     dict(frames=["b02", "b00"], frameMs=800, mode="loop", reducedFrame=0, priority=35, interrupt="any"),
    "needs_care":  dict(frames=["b05"], frameMs=800, mode="hold", reducedFrame=0, priority=50, interrupt="any"),
    "error":       dict(frames=["b06"], frameMs=1000, mode="hold", reducedFrame=0, priority=100, interrupt="higher", mirror=False),
}

# Sheet cell meaning (Amendment A2), for docs and contact sheets.
BASE_CELLS = ("idle front", "idle blink", "curious look", "side profile (right)", "happy/focused",
              "droopy needs-care", "confused", "asleep", "move 1", "move 2", "move 3", "move 4")
CARE_CELLS = ("eat 1", "eat 2", "drink 1", "drink 2", "play 1", "play 2", "wash 1", "wash 2",
              "privacy prop", "settle to sleep", "waking stretch", "celebrate")


def _ticks(lo_s, hi_s):
    return [int(lo_s * 5), int(hi_s * 5)]           # 200 ms scheduler ticks


def _sp(display, default_name, personality, habitat, labels, stages, locomotion, hints=None,
        hopper=False, states=None):
    food, water, play, wash, bathroom, sleep = labels
    return {
        "displayName": display,
        "defaultName": default_name,
        "personality": personality,
        "habitat": habitat,
        "careLabels": {"food": food, "water": water, "play": play, "wash": wash,
                       "bathroom": bathroom, "sleep": sleep},
        "careHints": hints or {},
        "stageNamesFallback": stages,
        "locomotion": locomotion,
        "hopper": hopper,
        # Per-species frame lists replacing the A4 table's (same mode and timing), for a
        # species whose poses make the default loop read wrongly.
        "stateFrames": states or {},
    }


SPECIES = {
    "cat": _sp("Cat", "Ginger", "the curious desk inspector", "floor",
               ("Kibble", "Fresh water", "Yarn tuft", "Groom", "Litter screen", "Loaf nap"),
               ("Kitten", "Desk Cat", "Grand Inspector"),
               {"style": "walk", "pxPerTick": 4, "pauseTicks": _ticks(8, 22)}),
    "dog": _sp("Dog", "Biscuit", "the enthusiastic teammate", "floor",
               ("Kibble", "Bowl lap", "Fetch ball", "Towel shake", "Bush screen", "Cushion nap"),
               ("Puppy", "Teammate", "Pack Lead"),
               {"style": "trot", "pxPerTick": 6, "pauseTicks": _ticks(5, 14)},
               # b00 stands while b04/b02 sit (stages 2-3): alternating them made the dog
               # stand up and sit down every 800 ms. Hold the seated pose instead.
               states={"working": ["b04"], "waiting": ["b02"]}),
    "capybara": _sp("Capybara", "Yuzu", "the unhurried team lead", "floor",
                    ("Leaf", "Slow sip", "Floating orange", "Gentle rinse", "Reed screen", "Side rest"),
                    ("Pup", "Lounger", "Hot-Spring Elder"),
                    {"style": "shuffle", "pxPerTick": 2, "pauseTicks": _ticks(18, 40)}),
    "penguin": _sp("Penguin", "Pip", "the earnest little courier", "floor",
                   ("Fish snack", "Cool water", "Pebble balance", "Flipper splash", "Ice screen", "Head-tuck nap"),
                   ("Chick", "Courier", "Emperor Courier"),
                   {"style": "waddle", "pxPerTick": 3, "pauseTicks": _ticks(8, 20)}),
    "robot": _sp("Robot", "Bolt", "the meticulous helper", "floor",
                 ("Energy cell", "Coolant check", "Magnet blocks", "Self-clean brush", "Service booth", "Dock & recharge"),
                 ("Mk I", "Mk II", "Mk III"),
                 {"style": "roll", "pxPerTick": 5, "pauseTicks": _ticks(10, 25)},
                 hints={"food": "Robots don't eat; this swaps in a fresh energy cell.",
                        "water": "Robots don't drink; this checks the coolant."}),
    "iguana": _sp("Iguana", "Sol", "the patient sun-seeker", "floor",
                  ("Leaf bites", "Dew sip", "Basking stone", "Mist", "Fern screen", "Lengthwise rest"),
                  ("Hatchling", "Sun-Seeker", "Old Sun"),
                  {"style": "crawl", "pxPerTick": 2.5, "pauseTicks": _ticks(20, 45)}),
    "spider": _sp("Spider", "Lace", "the careful silk architect", "floor",
                  ("Food pellet", "Dew bead", "Silk pluck", "Leg groom", "Silk alcove", "Silk retreat"),
                  ("Spiderling", "Weaver", "Master Architect"),
                  {"style": "tetrapod", "pxPerTick": 4, "pauseTicks": _ticks(10, 30)}),
    "snake": _sp("Snake", "Ribbon", "the serene ribbon", "floor",
                 ("Meal parcel", "Shallow dish", "Hoop weave", "Damp-mat glide", "Leafy hide box", "Spiral rest"),
                 ("Snakelet", "Ribbon", "Grand Coil"),
                 {"style": "lateral_wave", "pxPerTick": 3, "pauseTicks": _ticks(15, 35)}),
    "goldfish": _sp("Goldfish", "Bubbles", "the dreamy bubble navigator", "aquatic",
                    ("Surface pellets", "Water refresh", "Plant arch", "Tank clean", "Filter clean", "Plant hover"),
                    ("Fry", "Goldfish", "Fantail"),
                    {"style": "swim_in_habitat", "pxPerTick": 0, "pauseTicks": _ticks(6, 15)},
                    hints={"water": "Fish don't drink from bowls; this refreshes the tank water.",
                           "bathroom": "Cleans the filter; the fish stays behind its plants.",
                           "sleep": "Fish rest with their eyes open."}),
    "rabbit": _sp("Rabbit", "Clover", "the alert garden scout", "floor",
                  ("Hay", "Dish sip", "Willow ball", "Face groom", "Tray screen", "Ears-down loaf"),
                  ("Kit", "Scout", "Garden Warden"),
                  {"style": "hop", "pxPerTick": 8, "pxFrames": [1, 2], "pauseTicks": _ticks(8, 20)},
                  hopper=True),
    "turtle": _sp("Turtle", "Atlas", "the steadfast planner", "floor",
                  ("Leaf tug", "Shallow sip", "Pebble push", "Shell brush", "Rock alcove", "Shell tuck"),
                  ("Hatchling", "Planner", "Ancient Shell"),
                  {"style": "diagonal_steps", "pxPerTick": 1.5, "pauseTicks": _ticks(25, 50)}),
    "frog": _sp("Frog", "Hopper", "the spring-loaded optimist", "floor",
                ("Food mote", "Damp pad", "Lily-pad hops", "Mist rinse", "Reed screen", "Leaf crouch"),
                ("Froglet", "Frog", "Bullfrog Sage"),
                {"style": "leap", "pxPerTick": 5, "pxFrames": [1, 2], "pauseTicks": _ticks(6, 18)},
                hopper=True),
    "owl": _sp("Owl", "Quill", "the thoughtful night librarian", "floor",
               ("Morsel", "Sip", "Wooden bead", "Preen", "Perch screen", "Shoulder tuck"),
               ("Owlet", "Librarian", "Great Scholar"),
               {"style": "hop", "pxPerTick": 3, "pauseTicks": _ticks(15, 35)},
               hopper=True),
    "red_panda": _sp("Red panda", "Maple", "the bashful explorer", "floor",
                     ("Bamboo sprig", "Muzzle dip", "Hanging leaf", "Paw wipe", "Bamboo screen", "Tail-nose nap"),
                     ("Cub", "Explorer", "Mountain Elder"),
                     {"style": "amble", "pxPerTick": 3.5, "pauseTicks": _ticks(10, 25)}),
    "axolotl": _sp("Axolotl", "Lotus", "the cheerful tank gardener", "aquatic",
                   ("Pellet snap", "Water refresh", "Plant tunnel", "Cleaning bubbles", "Filter clean", "Hide rest"),
                   ("Larva", "Axolotl", "Grand Gardener"),
                   {"style": "bottom_walk_glide", "pxPerTick": 0, "pauseTicks": _ticks(8, 18)},
                   hints={"water": "Axolotls don't drink from bowls; this refreshes the tank water.",
                          "bathroom": "Cleans the filter; the axolotl stays in its plant hide.",
                          "sleep": "Rests tucked inside its rock hide."}),
}

assert tuple(SPECIES) == ORDER

# v0.3 (SPEC-v0.3 §4.1). careIconSet picks the care icon sheet and the copy overrides
# (floor | aquatic | robot); every species states it explicitly. careWords are the plain
# kid words used as {word} in the care bubbles ("Yum! {word}!"): at most two words per
# need, no digits, no jargon (K1, K2, K8). They never fall back to careLabels.
CARE_ICON_SETS = ("floor", "aquatic", "robot")
CARE_ICON_SET = {sid: ("robot" if sid == "robot" else
                       "aquatic" if SPECIES[sid]["habitat"] == "aquatic" else "floor") for sid in ORDER}

CARE_WORDS = {
    "cat":       {"food": "Kibble", "water": "Fresh water", "play": "Yarn", "wash": "Brushing"},
    "dog":       {"food": "Kibble", "water": "Cool water", "play": "Fetch", "wash": "Bubble bath"},
    "capybara":  {"food": "Sweet grass", "water": "Cool water", "play": "Splash", "wash": "Warm bath"},
    "penguin":   {"food": "Fish", "water": "Icy water", "play": "Pebbles", "wash": "Splash"},
    "robot":     {"food": "Battery", "water": "Cool breeze", "play": "Magnets", "wash": "Polish"},
    "iguana":    {"food": "Leaves", "water": "Dew drops", "play": "Sunshine", "wash": "Mist"},
    "spider":    {"food": "Bug snack", "water": "Dew drop", "play": "Silk swing", "wash": "Brushing"},
    "snake":     {"food": "Dinner", "water": "Cool water", "play": "Hoops", "wash": "Mist"},
    "goldfish":  {"food": "Fish flakes", "water": "Fresh water", "play": "Bubbles", "wash": "Clean tank"},
    "rabbit":    {"food": "Hay", "water": "Cool water", "play": "Ball", "wash": "Brushing"},
    "turtle":    {"food": "Lettuce", "water": "Cool water", "play": "Pebbles", "wash": "Shell scrub"},
    "frog":      {"food": "Bugs", "water": "Puddle", "play": "Lily pads", "wash": "Rain"},
    "owl":       {"food": "Dinner", "water": "Cool water", "play": "Beads", "wash": "Fluffy feathers"},
    "red_panda": {"food": "Bamboo", "water": "Cool water", "play": "Leaf swing", "wash": "Clean paws"},
    "axolotl":   {"food": "Pellets", "water": "Fresh water", "play": "Tunnels", "wash": "Clean tank"},
}

assert tuple(CARE_WORDS) == ORDER and set(CARE_ICON_SET.values()) <= set(CARE_ICON_SETS)


def source_dir(species_id):
    return SOURCE_DIRS.get(species_id, species_id)


def species_for_source_dir(name):
    for sid in ORDER:
        if source_dir(sid) == name:
            return sid
    return None
