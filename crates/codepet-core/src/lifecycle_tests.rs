//! T1 core tests (spec §17.2): species, evolution, lifecycle engine, presence validation,
//! and the shared `tests/fixtures/lifecycle_vectors.json` vectors.

use crate::evolution::*;
use crate::lifecycle::*;
use crate::presence::{self, PresenceDoc, Reject};
use crate::species::*;
use crate::Category;
use serde_json::Value;

const H: i64 = HOUR_MS;
const VECTORS: &str = include_str!("../tests/fixtures/lifecycle_vectors.json");

fn steady() -> Rules {
    RULES_V1.steady.clone()
}
fn playtest() -> Rules {
    RULES_V1.playtest.clone()
}
fn cat(s: &str) -> Category {
    Category::parse(s).unwrap_or_else(|| panic!("unknown need {s}"))
}

// ---------------------------------------------------------------------------
// Inline LCG (no new dependency).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

fn random_rules(g: &mut Lcg) -> Rules {
    match g.below(4) {
        0 => steady(),
        1 => playtest(),
        _ => {
            let critical_after_ms = g.range(300_000, 86_400_000);
            let grace_ms = g.range(critical_after_ms.max(900_000), 172_800_000);
            let lethal = match g.below(3) {
                0 => [true, false, false, false],
                1 => [false, true, false, false],
                _ => [true, true, false, false],
            };
            let r = Rules {
                version: RULESET_V1,
                decay_per_hour: std::array::from_fn(|_| g.range(0, 25_000) as u32),
                full_hold_ms: g.range(60_000, 14_400_000),
                warning_at: g.range(1000, 5000) as u16,
                lethal,
                critical_after_ms,
                grace_ms,
                ..steady()
            };
            r.validate().expect("random rules are valid");
            r
        }
    }
}

/// A consistent state reachable by the engine.
fn random_vitals(g: &mut Lcg, r: &Rules, mode: Mode) -> Vitals {
    let mut v = Vitals::default();
    for i in 0..4 {
        v.meters[i] = match g.below(5) {
            0 => 0,
            1 => 10_000,
            _ => g.range(1, 10_000) as u16,
        };
        if v.meters[i] == 10_000 && g.chance(60) {
            v.hold_ms[i] = g.range(1, r.full_hold_ms);
        }
        if v.meters[i] > 0 {
            v.carry[i] = g.range(0, H - 1);
        }
        if r.lethal[i] && v.meters[i] == 0 {
            v.neglect_ms[i] = g.range(0, r.critical_after_ms - 1);
        }
    }
    let empty: Vec<usize> = (0..4)
        .filter(|&j| r.lethal[j] && v.meters[j] == 0)
        .collect();
    if !empty.is_empty() && g.chance(40) {
        let j = empty[g.below(empty.len() as u64) as usize];
        v.neglect_ms[j] = r.critical_after_ms + g.range(0, 10 * H);
        let fainted = mode == Mode::Gentle && g.chance(30);
        v.critical = Some(Critical {
            need: Category::ALL[j],
            grace_ms: r.grace_ms,
            elapsed_ms: if fainted {
                r.grace_ms
            } else {
                g.range(0, r.grace_ms - 1)
            },
        });
        v.fainted = fainted;
    }
    v
}

fn random_delta(g: &mut Lcg) -> i64 {
    match g.below(10) {
        0 => -g.range(1, H),
        1 => SETTLE_CAP_MS + g.range(1, 10 * H),
        2 => g.range(0, 120_000),
        _ => g.range(0, SETTLE_CAP_MS),
    }
}

// ---------------------------------------------------------------------------
// Species

#[test]
fn species_roster_is_fixed_and_aligned() {
    assert_eq!(SPECIES_IDS.len(), 15);
    assert_eq!(SPECIES_IDS[0], "cat");
    assert_eq!(SPECIES_IDS[13], "red_panda");
    assert_eq!(SPECIES_IDS[14], "axolotl");
    for (i, id) in SPECIES_IDS.iter().enumerate() {
        assert!(is_species(id));
        assert_eq!(default_name(id), Some(DEFAULT_NAMES[i]));
        assert!(
            SPECIES_IDS[..i].iter().all(|other| other != id),
            "{id} twice"
        );
        assert!(id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
    }
    assert_eq!(default_name("cat"), Some("Ginger"));
    assert_eq!(default_name("axolotl"), Some("Lotus"));
    for bad in ["", "Cat", "red-panda", "goldfish-in-jar", "dragon", " cat"] {
        assert!(!is_species(bad), "{bad}");
        assert_eq!(default_name(bad), None);
    }
}

#[test]
fn pet_names_are_normalized_and_bounded() {
    assert_eq!(normalize_pet_name("  Ginger ").unwrap(), "Ginger");
    assert_eq!(
        normalize_pet_name("Mr\t\n  Whiskers").unwrap(),
        "Mr Whiskers"
    );
    assert_eq!(
        normalize_pet_name("\u{202e}Gin\u{200b}ger\x07").unwrap(),
        "Ginger"
    );
    assert_eq!(normalize_pet_name("\x1b[31mRed\x1b[0m").unwrap(), "Red");
    assert_eq!(normalize_pet_name("猫猫").unwrap(), "猫猫");
    assert_eq!(normalize_pet_name("a").unwrap(), "a");
    assert_eq!(normalize_pet_name(&"x".repeat(24)).unwrap(), "x".repeat(24));
    assert!(normalize_pet_name(&"x".repeat(25)).is_err());
    // 24 four-byte characters = 96 bytes: allowed; the char limit is the binding one.
    assert_eq!(normalize_pet_name(&"😺".repeat(24)).unwrap().len(), 96);
    assert!(normalize_pet_name(&"😺".repeat(25)).is_err());
    for empty in ["", "   ", "\u{202e}\u{200b}", "\x07\x08", "\n\t"] {
        assert_eq!(normalize_pet_name(empty), Err("name is empty"), "{empty:?}");
    }
    assert_eq!(
        normalize_pet_name("token=abc"),
        Err("name looks like a credential")
    );
    assert!(normalize_pet_name("ghp_secret").is_err());
    // Normalization is idempotent.
    let once = normalize_pet_name("  Pip \t the  penguin ").unwrap();
    assert_eq!(normalize_pet_name(&once).unwrap(), once);
}

// ---------------------------------------------------------------------------
// Evolution

#[test]
fn level_and_stage_boundaries() {
    let r = steady();
    let table: [(u32, u8, u8); 12] = [
        (0, 1, 1),
        (79, 1, 1),
        (80, 2, 2),
        (81, 2, 2),
        (199, 2, 2),
        (200, 3, 2),
        (399, 3, 2),
        (400, 4, 3),
        (401, 4, 3),
        (699, 4, 3),
        (700, 5, 3),
        (701, 5, 3),
    ];
    for (xp, level, stage) in table {
        assert_eq!(level_for_xp(xp, &r), level, "level at {xp}");
        assert_eq!(stage_for_xp(xp, &r), stage, "stage at {xp}");
        assert_eq!(stage_for_level(level, &r), stage);
    }
    assert_eq!(level_for_xp(u32::MAX, &r), 5);
    assert_eq!(stage_for_level(0, &r), 1);
    assert_eq!(stage_for_level(1, &r), 1);
    assert_eq!(stage_for_level(2, &r), 2);
    assert_eq!(stage_for_level(3, &r), 2);
    assert_eq!(stage_for_level(4, &r), 3);
    assert_eq!(stage_for_level(5, &r), 3);
    assert_eq!(STAGE_KEYS, ["young", "grown", "mature"]);
    assert_eq!(stage_key(1), Some("young"));
    assert_eq!(stage_key(3), Some("mature"));
    assert_eq!(stage_key(0), None);
    assert_eq!(stage_key(4), None);
    // Playtest shares the curve.
    assert_eq!(playtest().level_xp, r.level_xp);
    assert_eq!(playtest().stage_min_level, r.stage_min_level);
}

#[test]
fn level_matches_legacy_xp_view_formula() {
    // care.rs::xp_view (v0.1): count of thresholds ≤ xp; next = thresholds.get(level).
    let thresholds = [0u32, 80, 200, 400, 700];
    let r = steady();
    for xp in (0..=2000).chain([u32::MAX - 1, u32::MAX]) {
        let legacy = thresholds.iter().filter(|&&t| xp >= t).count() as u8;
        assert_eq!(level_for_xp(xp, &r), legacy, "xp {xp}");
        assert_eq!(
            next_level_xp(legacy, &r),
            thresholds.get(usize::from(legacy)).copied()
        );
    }
    assert_eq!(next_level_xp(1, &r), Some(80));
    assert_eq!(next_level_xp(4, &r), Some(700));
    assert_eq!(next_level_xp(5, &r), None);
}

#[test]
fn next_stage_and_stages_crossed() {
    let r = steady();
    let ns = |stage, min_level, xp| {
        Some(NextStage {
            stage,
            min_level,
            xp,
        })
    };
    assert_eq!(next_stage(0, &r), ns(2, 2, 80));
    assert_eq!(next_stage(79, &r), ns(2, 2, 80));
    assert_eq!(next_stage(80, &r), ns(3, 4, 400));
    assert_eq!(next_stage(399, &r), ns(3, 4, 400));
    assert_eq!(next_stage(400, &r), None);
    assert_eq!(next_stage(u32::MAX, &r), None);

    assert_eq!(stages_crossed(0, 79, &r), Vec::<u8>::new());
    assert_eq!(stages_crossed(60, 80, &r), vec![2]);
    assert_eq!(stages_crossed(79, 80, &r), vec![2]);
    assert_eq!(stages_crossed(80, 399, &r), Vec::<u8>::new());
    assert_eq!(stages_crossed(380, 400, &r), vec![3]);
    assert_eq!(stages_crossed(0, 400, &r), vec![2, 3]);
    assert_eq!(stages_crossed(0, 10_000, &r), vec![2, 3]);
    assert_eq!(stages_crossed(400, 400, &r), Vec::<u8>::new());
    // A reversal lowers the derived stage and never "crosses" anything.
    assert_eq!(stages_crossed(400, 0, &r), Vec::<u8>::new());
    assert_eq!(stage_for_xp(0, &r), 1);
}

// ---------------------------------------------------------------------------
// Rules

#[test]
fn rules_v1_values_and_validation() {
    let s = steady();
    let p = playtest();
    assert_eq!(s.version, "lifecycle-v1");
    assert_eq!(s.decay_per_hour, [1000, 1250, 600, 500]);
    assert_eq!(s.full_hold_ms, 3_600_000);
    assert_eq!(s.warning_at, 3000);
    assert_eq!(s.lethal, [true, true, false, false]);
    assert_eq!(s.critical_after_ms, 7_200_000);
    assert_eq!(s.grace_ms, 21_600_000);
    assert_eq!(p.decay_per_hour, [20000, 25000, 12000, 10000]);
    assert_eq!(p.full_hold_ms, 180_000);
    assert_eq!(p.critical_after_ms, 360_000);
    assert_eq!(p.grace_ms, 1_080_000);
    for (a, b) in p.decay_per_hour.iter().zip(s.decay_per_hour) {
        assert_eq!(*a, b * 20, "playtest is 20x decay");
    }
    assert_eq!(s.validate(), Ok(()));
    assert_eq!(p.validate(), Ok(()));
    assert_eq!(RULES_V1.for_pace(Pace::Steady), &s);
    assert_eq!(RULES_V1.for_pace(Pace::Playtest), &p);
    assert!(s.is_lethal(Category::Thirst));
    assert!(!s.is_lethal(Category::Hygiene));
}

#[test]
fn rules_validate_matrix() {
    type Edit = fn(&mut Rules);
    let ok: [(&str, Edit); 12] = [
        ("decay max", |r| r.decay_per_hour = [25000; 4]),
        ("decay zero", |r| r.decay_per_hour = [0; 4]),
        ("hold min", |r| r.full_hold_ms = 60_000),
        ("hold max", |r| r.full_hold_ms = 14_400_000),
        ("warning min", |r| r.warning_at = 1000),
        ("warning max", |r| r.warning_at = 5000),
        ("only hunger lethal", |r| {
            r.lethal = [true, false, false, false]
        }),
        ("only thirst lethal", |r| {
            r.lethal = [false, true, false, false]
        }),
        ("critical_after min", |r| r.critical_after_ms = 300_000),
        ("grace max", |r| r.grace_ms = 172_800_000),
        ("grace == critical_after", |r| {
            r.critical_after_ms = 7_200_000;
            r.grace_ms = 7_200_000;
        }),
        ("equal stage levels", |r| r.stage_min_level = [1, 1, 5]),
    ];
    for (name, edit) in ok {
        let mut r = steady();
        edit(&mut r);
        assert_eq!(r.validate(), Ok(()), "{name}");
    }
    let bad: [(&str, Edit); 18] = [
        ("version", |r| r.version = "lifecycle-v2"),
        ("decay above max", |r| r.decay_per_hour[2] = 25_001),
        ("hold below", |r| r.full_hold_ms = 59_999),
        ("hold above", |r| r.full_hold_ms = 14_400_001),
        ("warning below", |r| r.warning_at = 999),
        ("warning above", |r| r.warning_at = 5001),
        ("no lethal", |r| r.lethal = [false; 4]),
        ("mood lethal", |r| r.lethal = [true, true, true, false]),
        ("hygiene lethal", |r| r.lethal = [false, false, false, true]),
        ("critical_after below", |r| r.critical_after_ms = 299_999),
        ("critical_after above", |r| {
            r.critical_after_ms = 86_400_001;
            r.grace_ms = 172_800_000;
        }),
        ("grace below", |r| {
            r.critical_after_ms = 300_000;
            r.grace_ms = 899_999;
        }),
        ("grace above", |r| r.grace_ms = 172_800_001),
        ("grace < critical_after", |r| {
            r.critical_after_ms = 7_200_000;
            r.grace_ms = 7_199_999;
        }),
        ("level_xp not from 0", |r| {
            r.level_xp = [1, 80, 200, 400, 700]
        }),
        ("level_xp not increasing", |r| {
            r.level_xp = [0, 80, 80, 400, 700]
        }),
        ("stage start not 1", |r| r.stage_min_level = [2, 2, 4]),
        ("stage decreasing", |r| r.stage_min_level = [1, 4, 2]),
    ];
    for (name, edit) in bad {
        let mut r = steady();
        edit(&mut r);
        assert!(r.validate().is_err(), "{name} must be refused");
    }
    let mut r = steady();
    r.stage_min_level = [1, 2, 6];
    assert!(r.validate().is_err(), "stage level above 5");
}

#[test]
fn leniency_truth_table() {
    let base = steady();
    let check = |edit: fn(&mut Rules), expect: bool, name: &str| {
        let mut r = base.clone();
        edit(&mut r);
        assert_eq!(r.is_more_lenient_or_equal(&base), expect, "{name}");
    };
    check(|_| {}, true, "identical");
    check(|r| r.grace_ms += 1, true, "longer grace");
    check(|r| r.grace_ms -= 1, false, "shorter grace");
    check(|r| r.critical_after_ms += 1, true, "longer critical_after");
    check(
        |r| r.critical_after_ms -= 1,
        false,
        "shorter critical_after",
    );
    check(|r| r.decay_per_hour[1] -= 1, true, "slower thirst");
    check(|r| r.decay_per_hour[3] += 1, false, "faster hygiene");
    check(|r| r.decay_per_hour = [0; 4], true, "no decay");
    check(
        |r| {
            r.grace_ms += 1;
            r.decay_per_hour[0] += 1;
        },
        false,
        "mixed",
    );
    check(|r| r.full_hold_ms += 1, false, "hold changed");
    check(|r| r.warning_at -= 1, false, "warning changed");
    check(
        |r| r.lethal = [false, true, false, false],
        false,
        "lethal changed",
    );
    check(|r| r.level_xp[4] += 1, false, "curve changed");
    check(|r| r.stage_min_level[1] = 3, false, "stages changed");
    // Steady → playtest is stricter; playtest → steady is not "only more lenient" either
    // (hold differs), so pace can never change after adoption through leniency.
    assert!(!playtest().is_more_lenient_or_equal(&base));
    assert!(!base.is_more_lenient_or_equal(&playtest()));
}

#[test]
fn with_leniency_only_moves_toward_leniency() {
    let s = steady();
    assert_eq!(s.with_leniency(None, None, None), Ok(s.clone()));
    let longer = s.with_leniency(Some(30 * H), None, None).unwrap();
    assert_eq!(longer.grace_ms, 30 * H);
    assert!(s.with_leniency(Some(s.grace_ms - 1), None, None).is_err());
    assert!(s.with_leniency(Some(48 * H + 1), None, None).is_err());
    assert_eq!(
        s.with_leniency(Some(48 * H), None, None).unwrap().grace_ms,
        48 * H
    );
    // Raising critical_after above grace raises grace in the same change.
    let raised = s.with_leniency(None, Some(3 * H), None).unwrap();
    assert_eq!(raised.critical_after_ms, 3 * H);
    assert_eq!(raised.grace_ms, s.grace_ms);
    let raised = s.with_leniency(None, Some(24 * H), None).unwrap();
    assert_eq!(
        (raised.critical_after_ms, raised.grace_ms),
        (24 * H, 24 * H)
    );
    assert_eq!(raised.validate(), Ok(()));
    assert!(s
        .with_leniency(None, Some(s.critical_after_ms - 1), None)
        .is_err());
    // Decay percent scales the current rates down, 25..=100 only.
    let half = s.with_leniency(None, None, Some(50)).unwrap();
    assert_eq!(half.decay_per_hour, [500, 625, 300, 250]);
    let quarter_again = half.with_leniency(None, None, Some(25)).unwrap();
    assert_eq!(quarter_again.decay_per_hour, [125, 156, 75, 62]);
    assert_eq!(
        s.with_leniency(None, None, Some(100))
            .unwrap()
            .decay_per_hour,
        s.decay_per_hour
    );
    for bad in [0, 24, 101, 255] {
        assert!(s.with_leniency(None, None, Some(bad)).is_err(), "{bad}");
    }
    // Every accepted result is more lenient or equal and valid.
    let mut g = Lcg(7);
    for _ in 0..500 {
        let r = random_rules(&mut g);
        let grace = g.chance(50).then(|| g.range(0, 172_800_000));
        let crit = g.chance(50).then(|| g.range(0, 86_400_000));
        let pct = g.chance(50).then(|| g.range(0, 120) as u8);
        if let Ok(next) = r.with_leniency(grace, crit, pct) {
            assert!(next.is_more_lenient_or_equal(&r));
            assert_eq!(next.validate(), Ok(()));
        }
    }
}

// ---------------------------------------------------------------------------
// advance: properties

#[test]
fn advance_properties_over_seeded_cases() {
    let mut g = Lcg(0x5eed_c0de);
    let mut composed = 0;
    for case in 0..1000 {
        let r = random_rules(&mut g);
        let mode = if g.chance(50) {
            Mode::Permadeath
        } else {
            Mode::Gentle
        };
        let v = random_vitals(&mut g, &r, mode);
        let delta = random_delta(&mut g);
        let (a, iterations) = advance_traced(&v, &r, mode, delta);
        assert!(iterations <= 16, "case {case}: {iterations} iterations");
        assert_eq!(
            a.charged_ms + a.discarded_ms,
            delta.clamp(0, SETTLE_CAP_MS),
            "case {case}"
        );
        assert!(a.charged_ms >= 0 && a.discarded_ms >= 0);
        for i in 0..4 {
            assert!(a.vitals.meters[i] <= v.meters[i], "case {case}: meter rose");
            assert!(a.vitals.hold_ms[i] >= 0 && a.vitals.hold_ms[i] <= v.hold_ms[i].max(0));
            assert!((0..H).contains(&a.vitals.carry[i]));
            assert!(a.vitals.neglect_ms[i] >= v.neglect_ms[i]);
            if !r.lethal[i] {
                assert_eq!(
                    a.vitals.neglect_ms[i], v.neglect_ms[i],
                    "non-lethal neglect moved"
                );
            }
        }
        let entered = a
            .events
            .iter()
            .any(|e| matches!(e, Event::EnteredCritical(_)));
        let died = a.events.iter().any(|e| matches!(e, Event::Died(_)));
        assert!(!(entered && died), "case {case}: died in the entering call");
        assert!(a.events.len() <= 1);
        if mode == Mode::Gentle {
            assert!(!died, "gentle never dies");
        }
        if entered || died {
            assert_eq!(a.events.len(), 1, "terminal event ends the call");
        }
        if let Some(c) = a.vitals.critical {
            assert!(r.lethal[c.need.index()], "non-lethal need became critical");
            if let Some(old) = v.critical {
                assert_eq!(c.grace_ms, old.grace_ms, "grace not frozen");
            } else {
                assert_eq!(c.grace_ms, r.grace_ms);
                assert_eq!(c.elapsed_ms, 0);
            }
        }
        // Holds delay decay.
        for i in 0..4 {
            if v.hold_ms[i] > 0 && delta.clamp(0, SETTLE_CAP_MS) <= v.hold_ms[i] {
                assert_eq!(
                    a.vitals.meters[i], v.meters[i],
                    "case {case}: held meter decayed"
                );
                assert_eq!(a.vitals.carry[i], v.carry[i]);
            }
        }
        // Composition when there are no events.
        let total = delta.clamp(0, SETTLE_CAP_MS);
        if total > 0 && a.events.is_empty() {
            let split = g.range(0, total);
            let first = advance(&v, &r, mode, split);
            if first.events.is_empty() {
                let second = advance(&first.vitals, &r, mode, total - split);
                if second.events.is_empty() {
                    assert_eq!(second.vitals, a.vitals, "case {case}: composition");
                    assert_eq!(first.charged_ms + second.charged_ms, a.charged_ms);
                    composed += 1;
                }
            }
        }
    }
    assert!(
        composed > 300,
        "composition exercised only {composed} times"
    );
}

#[test]
fn one_second_steps_equal_one_hour() {
    let mut g = Lcg(42);
    for _ in 0..20 {
        let r = random_rules(&mut g);
        let mut v = random_vitals(&mut g, &r, Mode::Permadeath);
        v.critical = None;
        v.fainted = false;
        for j in 0..4 {
            v.neglect_ms[j] = 0;
        }
        let whole = advance(&v, &r, Mode::Permadeath, H);
        if !whole.events.is_empty() {
            continue;
        }
        let mut stepped = v.clone();
        for _ in 0..3600 {
            let a = advance(&stepped, &r, Mode::Permadeath, 1000);
            assert!(a.events.is_empty());
            assert_eq!(a.charged_ms, 1000);
            stepped = a.vitals;
        }
        assert_eq!(stepped, whole.vitals);
    }
    // And a fixed fractional case: thirst 1250/h for 3600 × 1 s = exactly 1250.
    let v = Vitals {
        meters: [7000; 4],
        ..Vitals::default()
    };
    let mut s = v.clone();
    for _ in 0..3600 {
        s = advance(&s, &steady(), Mode::Permadeath, 1000).vitals;
    }
    assert_eq!(s.meters, [6000, 5750, 6400, 6500]);
    assert_eq!(s.carry, [0; 4]);
}

// ---------------------------------------------------------------------------
// advance: thresholds and humane guarantees

fn thirst_empty(neglect: i64) -> Vitals {
    Vitals {
        meters: [10_000, 0, 10_000, 10_000],
        hold_ms: [H, 0, H, H],
        neglect_ms: [0, neglect, 0, 0],
        ..Vitals::default()
    }
}

#[test]
fn warning_critical_and_death_thresholds_at_equality() {
    let r = steady();
    // meter == warning_at is warning; one above is healthy.
    let mut v = Vitals::from_meters([10_000; 4], &r);
    v.meters[1] = 3000;
    v.hold_ms[1] = 0;
    assert_eq!(status(&v, &r), LifeStatus::Warning);
    v.meters[1] = 3001;
    assert_eq!(status(&v, &r), LifeStatus::Healthy);
    // neglect == critical_after enters critical; one below does not.
    let a = advance(
        &thirst_empty(r.critical_after_ms - 2),
        &r,
        Mode::Permadeath,
        1,
    );
    assert!(a.events.is_empty());
    assert_eq!(status(&a.vitals, &r), LifeStatus::Warning);
    let a = advance(
        &thirst_empty(r.critical_after_ms - 1),
        &r,
        Mode::Permadeath,
        1,
    );
    assert_eq!(a.events, vec![Event::EnteredCritical(Category::Thirst)]);
    assert_eq!(status(&a.vitals, &r), LifeStatus::Critical);
    // elapsed == grace is death; one below is not.
    let mut c = a.vitals.clone();
    c.critical.as_mut().unwrap().elapsed_ms = r.grace_ms - 2;
    let a = advance(&c, &r, Mode::Permadeath, 1);
    assert!(a.events.is_empty());
    let a = advance(&a.vitals, &r, Mode::Permadeath, 1);
    assert_eq!(a.events, vec![Event::Died(Category::Thirst)]);
}

#[test]
fn entering_critical_discards_the_rest_and_never_dies_in_the_same_call() {
    let r = steady();
    // Even a full 8 h settlement from one millisecond before critical stops at entry.
    let a = advance(
        &thirst_empty(r.critical_after_ms - 1),
        &r,
        Mode::Permadeath,
        SETTLE_CAP_MS,
    );
    assert_eq!(a.events, vec![Event::EnteredCritical(Category::Thirst)]);
    assert_eq!(a.charged_ms, 1);
    assert_eq!(a.discarded_ms, SETTLE_CAP_MS - 1);
    assert_eq!(a.vitals.critical.unwrap().elapsed_ms, 0);
    // With the shortest possible grace, death still needs a later settlement.
    let mut short = r.clone();
    short.critical_after_ms = 900_000;
    short.grace_ms = 900_000;
    let a = advance(&thirst_empty(0), &short, Mode::Permadeath, SETTLE_CAP_MS);
    assert_eq!(a.events, vec![Event::EnteredCritical(Category::Thirst)]);
    let b = advance(&a.vitals, &short, Mode::Permadeath, SETTLE_CAP_MS);
    assert_eq!(b.events, vec![Event::Died(Category::Thirst)]);
    assert_eq!(b.charged_ms, 900_000);
}

#[test]
fn gentle_faints_instead_of_dying_and_keeps_charging() {
    let r = steady();
    let mut v = thirst_empty(r.critical_after_ms);
    v.critical = Some(Critical {
        need: Category::Thirst,
        grace_ms: r.grace_ms,
        elapsed_ms: r.grace_ms - H,
    });
    let a = advance(&v, &r, Mode::Gentle, 3 * H);
    assert_eq!(a.events, vec![Event::Fainted]);
    assert!(a.vitals.fainted);
    assert_eq!((a.charged_ms, a.discarded_ms), (3 * H, 0));
    assert_eq!(a.vitals.critical.unwrap().elapsed_ms, r.grace_ms);
    // Fainted: elapsed frozen, no further events however long.
    let b = advance(&a.vitals, &r, Mode::Gentle, SETTLE_CAP_MS);
    assert!(b.events.is_empty());
    assert_eq!(b.vitals.critical.unwrap().elapsed_ms, r.grace_ms);
    let p = advance(&v, &r, Mode::Permadeath, 3 * H);
    assert_eq!(p.events, vec![Event::Died(Category::Thirst)]);
    assert_eq!((p.charged_ms, p.discarded_ms), (H, 2 * H));
}

#[test]
fn non_lethal_needs_never_cause_critical() {
    let r = steady();
    let mut v = Vitals {
        meters: [10_000, 10_000, 0, 0],
        hold_ms: [SETTLE_CAP_MS, SETTLE_CAP_MS, 0, 0],
        ..Vitals::default()
    };
    for _ in 0..10 {
        let a = advance(&v, &r, Mode::Permadeath, SETTLE_CAP_MS);
        assert!(a.events.is_empty());
        assert_eq!(a.vitals.neglect_ms, [0; 4]);
        v = a.vitals;
        v.hold_ms[0] = SETTLE_CAP_MS;
        v.hold_ms[1] = SETTLE_CAP_MS;
    }
    assert_eq!(status(&v, &r), LifeStatus::Warning, "low mood only warns");
}

#[test]
fn critical_tie_resolves_in_category_order() {
    let r = steady();
    let v = Vitals {
        meters: [0, 0, 10_000, 10_000],
        neglect_ms: [r.critical_after_ms - 1, r.critical_after_ms - 1, 0, 0],
        ..Vitals::default()
    };
    let a = advance(&v, &r, Mode::Permadeath, H);
    assert_eq!(a.events, vec![Event::EnteredCritical(Category::Hunger)]);
}

#[test]
fn worst_case_timeline_matches_spec_tables() {
    for (rules, pace_name, zero, critical, death) in [
        (steady(), "steady", 9 * H, 11 * H, 17 * H),
        (
            playtest(),
            "playtest",
            27 * 60_000,
            33 * 60_000,
            51 * 60_000,
        ),
    ] {
        let fresh = Vitals::from_meters([10_000; 4], &rules);
        assert_eq!(fresh.hold_ms, [rules.full_hold_ms; 4]);
        // Walk in 1-minute settlements and record when things happen.
        let mut v = fresh.clone();
        let mut t = 0;
        let (mut saw_zero, mut saw_critical, mut saw_death) = (None, None, None);
        while saw_death.is_none() && t < 30 * H {
            let a = advance(&v, &rules, Mode::Permadeath, TICK_MS);
            t += a.charged_ms;
            if a.vitals.meters[1] == 0 && saw_zero.is_none() {
                saw_zero = Some(t);
            }
            for e in &a.events {
                match e {
                    Event::EnteredCritical(Category::Thirst) => saw_critical = Some(t),
                    Event::Died(Category::Thirst) => saw_death = Some(t),
                    other => panic!("{pace_name}: unexpected {other:?}"),
                }
            }
            v = a.vitals;
        }
        assert_eq!(saw_zero, Some(zero), "{pace_name} zero");
        assert_eq!(saw_critical, Some(critical), "{pace_name} critical");
        assert_eq!(saw_death, Some(death), "{pace_name} death");
    }
}

#[test]
fn advance_clamps_and_ignores_negative_time() {
    let r = steady();
    let v = Vitals::from_meters([7000; 4], &r);
    for d in [i64::MIN, -1, 0] {
        let a = advance(&v, &r, Mode::Permadeath, d);
        assert_eq!(a.vitals, v);
        assert_eq!((a.charged_ms, a.discarded_ms), (0, 0));
    }
    let a = advance(&v, &r, Mode::Permadeath, i64::MAX);
    assert_eq!(a.charged_ms + a.discarded_ms, SETTLE_CAP_MS);
}

// ---------------------------------------------------------------------------
// apply_care, debit, refund, leniency

#[test]
fn apply_care_matching_irrelevant_and_partial() {
    let r = steady();
    let mut crit = thirst_empty(r.critical_after_ms + 5);
    crit.critical = Some(Critical {
        need: Category::Thirst,
        grace_ms: r.grace_ms,
        elapsed_ms: 1234,
    });
    // Matching care resets that need and recovers.
    let (v, ev) = apply_care(&crit, &r, &[Category::Thirst]);
    assert_eq!(ev, vec![Event::Recovered]);
    assert_eq!(v.meters[1], 10_000);
    assert_eq!(v.hold_ms[1], r.full_hold_ms);
    assert_eq!((v.carry[1], v.neglect_ms[1]), (0, 0));
    assert_eq!(v.critical, None);
    assert_eq!(status(&v, &r), LifeStatus::Healthy);
    // Irrelevant care leaves critical untouched.
    let (v, ev) = apply_care(&crit, &r, &[Category::Hygiene, Category::MentalHealth]);
    assert!(ev.is_empty());
    assert_eq!(v.critical, crit.critical);
    assert_eq!(v.neglect_ms, crit.neglect_ms);
    // Two critical needs: caring for one keeps elapsed and the frozen grace.
    let mut both = crit.clone();
    both.meters[0] = 0;
    both.hold_ms[0] = 0;
    both.neglect_ms[0] = r.critical_after_ms;
    both.critical = Some(Critical {
        need: Category::Thirst,
        grace_ms: 999_999_999,
        elapsed_ms: 4321,
    });
    let (v, ev) = apply_care(&both, &r, &[Category::Thirst]);
    assert!(ev.is_empty());
    assert_eq!(
        v.critical,
        Some(Critical {
            need: Category::Hunger,
            grace_ms: 999_999_999,
            elapsed_ms: 4321
        })
    );
    let (v, ev) = apply_care(&v, &r, &[Category::Hunger]);
    assert_eq!(ev, vec![Event::Recovered]);
    assert_eq!(v.critical, None);
    // Fainted is cleared only on recovery.
    let mut fainted = both.clone();
    fainted.fainted = true;
    let (v, _) = apply_care(&fainted, &r, &[Category::Thirst]);
    assert!(v.fainted);
    let (v, ev) = apply_care(&v, &r, &[Category::Hunger]);
    assert_eq!(ev, vec![Event::Recovered]);
    assert!(!v.fainted);
    // Care outside critical journals nothing and resets lethal neglect.
    let warn = thirst_empty(600_000);
    let (v, ev) = apply_care(&warn, &r, &[Category::Thirst]);
    assert!(ev.is_empty());
    assert_eq!(v.neglect_ms, [0; 4]);
}

/// Review finding: leniency during a critical must not let unrelated care clear it (and
/// so grant a fresh grace later); the critical keeps counting under its frozen grace until
/// its own need is cared for.
#[test]
fn leniency_during_a_critical_never_lets_unrelated_care_clear_it() {
    let r = steady();
    let mut crit = thirst_empty(2 * H);
    crit.critical = Some(Critical {
        need: Category::Thirst,
        grace_ms: r.grace_ms,
        elapsed_ms: H,
    });
    let lenient = r.with_leniency(None, Some(3 * H), None).unwrap();
    assert!(crit.neglect_ms[1] < lenient.critical_after_ms);
    let v = on_leniency(&crit, &lenient);
    assert_eq!(
        v.critical, crit.critical,
        "leniency never clears a running critical"
    );
    // Unrelated care: nothing but the filled meters changes.
    for unrelated in [Category::Hygiene, Category::MentalHealth, Category::Hunger] {
        let (after, ev) = apply_care(&v, &lenient, &[unrelated]);
        assert!(ev.is_empty(), "{unrelated:?}");
        assert_eq!(after.critical, v.critical, "{unrelated:?}");
        assert_eq!(after.neglect_ms[1], v.neglect_ms[1], "{unrelated:?}");
        assert_eq!(status(&after, &lenient), LifeStatus::Critical);
    }
    // Without care the frozen grace still runs out.
    let (a, _) = advance_traced(&v, &lenient, Mode::Permadeath, 5 * H);
    assert_eq!(a.events, vec![Event::Died(Category::Thirst)]);
    // Matching care recovers as before.
    let (after, ev) = apply_care(&v, &lenient, &[Category::Thirst]);
    assert_eq!(ev, vec![Event::Recovered]);
    assert_eq!(after.critical, None);
}

#[cfg(feature = "legacy-tasks")]
#[test]
fn debit_and_refund_helpers() {
    let r = steady();
    let full = Vitals::from_meters([10_000, 10_000, 9000, 10_000], &r);
    assert_eq!(full.hold_ms, [H, H, 0, H]);
    let debited = on_debit(&full);
    assert_eq!(debited.hold_ms, [0; 4]);
    assert_eq!(debited.meters, full.meters);
    let mut refunded_meters = debited.clone();
    refunded_meters.meters = [10_000, 9000, 9000, 10_000];
    let refunded = on_refund(
        &refunded_meters,
        &r,
        &[Category::Hunger, Category::Thirst, Category::Hygiene],
    );
    // Only needs actually restored to full get a hold back.
    assert_eq!(refunded.hold_ms, [H, 0, 0, H]);
}

#[test]
fn leniency_helpers() {
    let r = steady();
    let full = Vitals::from_meters([10_000, 10_000, 9000, 10_000], &r);
    let mut crit = thirst_empty(r.critical_after_ms);
    crit.critical = Some(Critical {
        need: Category::Thirst,
        grace_ms: r.grace_ms,
        elapsed_ms: 10,
    });
    let longer = r.with_leniency(Some(30 * H), None, None).unwrap();
    assert_eq!(
        on_leniency(&crit, &longer).critical.unwrap().grace_ms,
        30 * H
    );
    // A frozen grace never shrinks.
    let mut big = crit.clone();
    big.critical.as_mut().unwrap().grace_ms = 40 * H;
    assert_eq!(
        on_leniency(&big, &longer).critical.unwrap().grace_ms,
        40 * H
    );
    assert_eq!(on_leniency(&full, &longer), full);
}

// ---------------------------------------------------------------------------
// windows

#[test]
fn windows_match_spec_worst_case_table() {
    let r = steady();
    let fresh = Vitals::from_meters([10_000; 4], &r);
    let w = windows(&fresh, &r, Mode::Permadeath);
    let hunger = w[0];
    let thirst = w[1];
    assert_eq!(thirst.until_warning_ms, Some(6 * H + 36 * 60_000)); // 6.6 h
    assert_eq!(thirst.until_critical_ms, Some(11 * H));
    assert_eq!(thirst.until_end_ms, Some(17 * H));
    assert_eq!(hunger.until_warning_ms, Some(8 * H));
    assert_eq!(hunger.until_critical_ms, Some(13 * H));
    assert_eq!(hunger.until_end_ms, Some(19 * H));
    assert_eq!(w[2].until_warning_ms, Some(45_600_000)); // ≈12.7 h
    assert_eq!(w[3].until_warning_ms, Some(15 * H));
    for nw in &w[2..] {
        assert!(!nw.lethal);
        assert_eq!((nw.until_critical_ms, nw.until_end_ms), (None, None));
    }
    assert!(w.iter().all(|n| n.state == NeedState::Ok));
    // Gentle never predicts an end.
    assert!(windows(&fresh, &r, Mode::Gentle)
        .iter()
        .all(|n| n.until_end_ms.is_none()));
    // Critical: remaining frozen grace.
    let mut crit = thirst_empty(r.critical_after_ms);
    crit.critical = Some(Critical {
        need: Category::Thirst,
        grace_ms: r.grace_ms,
        elapsed_ms: H,
    });
    let w = windows(&crit, &r, Mode::Permadeath);
    assert_eq!(w[1].state, NeedState::Critical);
    assert_eq!(w[1].until_critical_ms, Some(0));
    assert_eq!(w[1].until_end_ms, Some(r.grace_ms - H));
    assert_eq!(w[1].until_warning_ms, Some(0));
    // An empty, non-critical lethal need.
    let w = windows(&thirst_empty(H), &r, Mode::Permadeath);
    assert_eq!(w[1].state, NeedState::Empty);
    assert_eq!(w[1].until_critical_ms, Some(H));
    assert_eq!(w[1].until_end_ms, Some(H + r.grace_ms));
    // Low.
    let mut low = fresh.clone();
    low.meters[3] = 3000;
    low.hold_ms[3] = 0;
    assert_eq!(windows(&low, &r, Mode::Permadeath)[3].state, NeedState::Low);
    assert_eq!(NeedState::Low.as_str(), "low");
}

// ---------------------------------------------------------------------------
// Enums

#[test]
fn enum_strings_round_trip() {
    for m in Mode::ALL {
        assert_eq!(Mode::parse(m.as_str()), Some(m));
    }
    for p in Pace::ALL {
        assert_eq!(Pace::parse(p.as_str()), Some(p));
    }
    for s in LifeStatus::ALL {
        assert_eq!(LifeStatus::parse(s.as_str()), Some(s));
        assert_eq!(s.is_living(), !s.is_terminal());
    }
    assert_eq!(Mode::parse("hardcore"), None);
    assert_eq!(Pace::parse("fast"), None);
    assert_eq!(LifeStatus::parse("Healthy"), None);
    assert_eq!(Event::Died(Category::Hunger).kind(), "died");
    assert_eq!(Event::Recovered.need(), None);
}

// ---------------------------------------------------------------------------
// Presence

fn doc() -> PresenceDoc {
    PresenceDoc {
        v: 1,
        session: "bar-lx2k9-a1b2c3".into(),
        life_id: format!("life-{}", "0123456789abcdef01234567"),
        started_wall_ms: 1_000_000,
        updated_wall_ms: 1_000_000 + 10 * 60_000,
        attended_ms: 10 * 60_000,
        ticks: 10,
    }
}

#[test]
fn presence_accepts_valid_documents() {
    let now = 2_000_000;
    assert_eq!(presence::validate(&doc(), now), Ok(()));
    let mut d = doc();
    d.attended_ms = 0;
    d.ticks = 0;
    d.updated_wall_ms = d.started_wall_ms;
    assert_eq!(presence::validate(&d, now), Ok(()));
    // attended may exceed the wall window by one tick (the first tick's lead-in).
    let mut d = doc();
    d.ticks = 11;
    d.attended_ms = 11 * 60_000;
    assert_eq!(presence::validate(&d, now), Ok(()));
    // updated exactly 120 s ahead of the reader is tolerated.
    let mut d = doc();
    d.updated_wall_ms = now + 120_000;
    assert_eq!(presence::validate(&d, now), Ok(()));
    let mut d = doc();
    d.ticks = 525_600;
    assert_eq!(presence::validate(&d, now), Ok(()));
    for ok in [
        "bar-abcdef",
        "bar-0-0-0-",
        &format!("bar-{}", "a".repeat(58)),
    ] {
        assert!(presence::is_session_id(ok), "{ok}");
    }
}

#[test]
fn presence_rejection_matrix() {
    let now = 2_000_000;
    type Edit = fn(&mut PresenceDoc);
    let cases: [(Edit, Reject); 22] = [
        (|d| d.v = 0, Reject::Version),
        (|d| d.v = 2, Reject::Version),
        (|d| d.session = "bar-abcde".into(), Reject::Session),
        (
            |d| d.session = format!("bar-{}", "a".repeat(59)),
            Reject::Session,
        ),
        (|d| d.session = "bar-ABCDEF".into(), Reject::Session),
        (|d| d.session = "bar_abcdef".into(), Reject::Session),
        (|d| d.session = "xbar-abcdef".into(), Reject::Session),
        (|d| d.session = "bar-abc def".into(), Reject::Session),
        (|d| d.session = "bar-abcdé".into(), Reject::Session),
        (|d| d.life_id = "life-0123".into(), Reject::LifeId),
        (
            |d| d.life_id = format!("life-{}", "A".repeat(24)),
            Reject::LifeId,
        ),
        (
            |d| d.life_id = format!("life-{}", "g".repeat(24)),
            Reject::LifeId,
        ),
        (
            |d| d.life_id = format!("life-{}0", "a".repeat(24)),
            Reject::LifeId,
        ),
        (
            |d| d.life_id = format!("pet-{}", "a".repeat(25)),
            Reject::LifeId,
        ),
        (|d| d.ticks = -1, Reject::Ticks),
        (|d| d.ticks = 525_601, Reject::Ticks),
        (|d| d.attended_ms = -1, Reject::Attended),
        (|d| d.attended_ms = 10 * 60_000 + 1, Reject::Attended),
        (|d| d.started_wall_ms = d.updated_wall_ms + 1, Reject::Order),
        (
            |d| {
                d.started_wall_ms = d.updated_wall_ms - 8 * 60_000;
            },
            Reject::Window,
        ),
        (|d| d.updated_wall_ms = 2_000_000 + 120_001, Reject::Future),
        (
            |d| {
                d.started_wall_ms = i64::MIN;
                d.updated_wall_ms = i64::MAX;
            },
            Reject::Future,
        ),
    ];
    for (i, (edit, expect)) in cases.into_iter().enumerate() {
        let mut d = doc();
        edit(&mut d);
        assert_eq!(presence::validate(&d, now), Err(expect), "case {i}: {d:?}");
        assert!(!expect.as_str().is_empty());
    }
    // Window boundary: attended == (updated - started) + 60000 is allowed, +1 is not.
    let mut d = doc();
    d.started_wall_ms = d.updated_wall_ms - 9 * 60_000;
    assert_eq!(presence::validate(&d, now), Ok(()));
    d.started_wall_ms += 1;
    assert_eq!(presence::validate(&d, now), Err(Reject::Window));
    // Extreme values never overflow.
    let mut d = doc();
    d.ticks = 525_600;
    d.attended_ms = i64::MAX;
    assert_eq!(presence::validate(&d, i64::MAX), Err(Reject::Attended));
    let mut d = doc();
    d.updated_wall_ms = i64::MAX;
    assert_eq!(presence::validate(&d, i64::MAX), Ok(()));
    assert_eq!(presence::validate(&d, i64::MIN), Err(Reject::Future));
}

// ---------------------------------------------------------------------------
// Shared vectors

fn arr4(v: &Value) -> [i64; 4] {
    let a = v.as_array().expect("array");
    assert_eq!(a.len(), 4);
    std::array::from_fn(|i| a[i].as_i64().expect("integer"))
}

fn rules_from(v: &Value) -> Rules {
    assert_eq!(v["version"], RULESET_V1);
    let lethal_names: Vec<&str> = v["lethal"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    let level: Vec<u32> = v["level_xp"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as u32)
        .collect();
    let stages: Vec<u8> = v["stage_min_level"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap() as u8)
        .collect();
    let r = Rules {
        version: RULESET_V1,
        decay_per_hour: arr4(&v["decay_per_hour"]).map(|d| d as u32),
        full_hold_ms: v["full_hold_ms"].as_i64().unwrap(),
        warning_at: v["warning_at"].as_u64().unwrap() as u16,
        lethal: Category::ALL.map(|c| lethal_names.contains(&c.as_str())),
        critical_after_ms: v["critical_after_ms"].as_i64().unwrap(),
        grace_ms: v["grace_ms"].as_i64().unwrap(),
        level_xp: level.try_into().unwrap(),
        stage_min_level: stages.try_into().unwrap(),
    };
    assert_eq!(r.validate(), Ok(()), "vector rules must be valid");
    r
}

fn vitals_from(v: &Value) -> Vitals {
    let critical = match &v["critical"] {
        Value::Null => None,
        c => Some(Critical {
            need: cat(c["need"].as_str().unwrap()),
            grace_ms: c["grace_ms"].as_i64().unwrap(),
            elapsed_ms: c["elapsed_ms"].as_i64().unwrap(),
        }),
    };
    Vitals {
        meters: arr4(&v["meters"]).map(|m| m as u16),
        hold_ms: arr4(&v["hold_ms"]),
        carry: arr4(&v["carry"]),
        neglect_ms: arr4(&v["neglect_ms"]),
        critical,
        fainted: v["fainted"].as_bool().unwrap(),
    }
}

fn events_from(v: &Value) -> Vec<Event> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| match e["kind"].as_str().unwrap() {
            "entered_critical" => Event::EnteredCritical(cat(e["need"].as_str().unwrap())),
            "died" => Event::Died(cat(e["need"].as_str().unwrap())),
            "recovered" => Event::Recovered,
            "fainted" => Event::Fainted,
            other => panic!("unknown event {other}"),
        })
        .collect()
}

fn opt(v: &Value) -> Option<i64> {
    v.as_i64()
}

fn check_windows(name: &str, got: &[NeedWindow; 4], want: &Value) {
    let want = want.as_array().unwrap();
    assert_eq!(want.len(), 4);
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.need.as_str(), w["need"], "{name}");
        assert_eq!(g.lethal, w["lethal"].as_bool().unwrap(), "{name}");
        assert_eq!(g.state.as_str(), w["state"], "{name} {:?}", g.need);
        assert_eq!(
            g.until_warning_ms,
            opt(&w["until_warning_ms"]),
            "{name} {:?}",
            g.need
        );
        assert_eq!(
            g.until_critical_ms,
            opt(&w["until_critical_ms"]),
            "{name} {:?}",
            g.need
        );
        assert_eq!(
            g.until_end_ms,
            opt(&w["until_end_ms"]),
            "{name} {:?}",
            g.need
        );
    }
}

#[test]
fn shared_vectors_pass() {
    let doc: Value = serde_json::from_str(VECTORS).expect("vectors parse");
    assert_eq!(doc["format"], "codepet.lifecycle-vectors.v1");
    assert_eq!(doc["ruleset"], RULESET_V1);
    let cases = doc["cases"].as_array().unwrap();
    assert!(cases.len() >= 40, "only {} vectors", cases.len());
    let (mut advances, mut cares, mut steady_seen, mut playtest_seen) = (0, 0, false, false);
    let mut names = std::collections::BTreeSet::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        assert!(names.insert(name), "duplicate vector {name}");
        let r = rules_from(&case["rules"]);
        steady_seen |= r == RULES_V1.steady;
        playtest_seen |= r == RULES_V1.playtest;
        let mode = Mode::parse(case["mode"].as_str().unwrap()).unwrap();
        let v = vitals_from(&case["vitals"]);
        let expect = &case["expect"];
        let want_vitals = vitals_from(&expect["vitals"]);
        let want_events = events_from(&expect["events"]);
        let got_vitals = match case["op"].as_str().unwrap() {
            "advance" => {
                advances += 1;
                let delta = case["delta"].as_i64().unwrap();
                let (a, iterations) = advance_traced(&v, &r, mode, delta);
                assert!(iterations <= 16, "{name}");
                assert_eq!(a.events, want_events, "{name} events");
                assert_eq!(
                    a.charged_ms,
                    expect["charged"].as_i64().unwrap(),
                    "{name} charged"
                );
                assert_eq!(
                    a.discarded_ms,
                    expect["discarded"].as_i64().unwrap(),
                    "{name} discarded"
                );
                a.vitals
            }
            "apply_care" => {
                cares += 1;
                let filled: Vec<Category> = case["filled"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| cat(c.as_str().unwrap()))
                    .collect();
                let (got, events) = apply_care(&v, &r, &filled);
                assert_eq!(events, want_events, "{name} events");
                got
            }
            other => panic!("{name}: unknown op {other}"),
        };
        assert_eq!(got_vitals, want_vitals, "{name} vitals");
        assert_eq!(
            status(&got_vitals, &r).as_str(),
            expect["status"],
            "{name} status"
        );
        check_windows(name, &windows(&got_vitals, &r, mode), &expect["windows"]);
    }
    assert!(
        advances >= 30 && cares >= 10,
        "{advances} advance, {cares} care"
    );
    assert!(steady_seen && playtest_seen, "both presets must be covered");
}
