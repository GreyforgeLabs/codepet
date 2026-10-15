// Node tests for qml/shared/LifeText.js (life UI copy and attended-time math).
// Run: node --test qml/tests/test_life_text.mjs
//
// The engine half is checked against the shared Rust vectors
// (crates/codepet-core/tests/fixtures/lifecycle_vectors.json); predictions must never
// produce a passing/death string; every duration says "active time"; copy stays calm.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

process.env.TZ = 'UTC';

function loadLibrary(rel) {
    const src = readFileSync(new URL(rel, import.meta.url), 'utf8');
    assert.match(src, /^\.pragma library\n/, `${rel} must stay a .pragma library`);
    assert.doesNotMatch(src, /^\s*(import|\.import)\b/m, `${rel} must not import anything`);
    const ctx = {};
    vm.createContext(ctx);
    vm.runInContext(src.replace(/^\.pragma library$/m, ''), ctx, { filename: rel });
    return ctx;
}

const L = loadLibrary('../shared/LifeText.js');
const VECTORS = JSON.parse(readFileSync(new URL('../../crates/codepet-core/tests/fixtures/lifecycle_vectors.json', import.meta.url), 'utf8'));
const plain = (x) => JSON.parse(JSON.stringify(x));
const DEATHLY = /pass(ed|es)? away|\bdie[sd]?\b|\bdeath\b|\bdead\b/i;
const LIFE = 'life-0123456789abcdef01234567';

test('vectors file is the expected format and large enough', () => {
    assert.equal(VECTORS.format, 'codepet.lifecycle-vectors.v1');
    assert.equal(VECTORS.ruleset, 'lifecycle-v1');
    assert.ok(VECTORS.cases.length >= 40, 'at least 40 shared vectors');
});

for (const c of VECTORS.cases) {
    test(`vector ${c.name}`, () => {
        if (c.op === 'advance') {
            const got = plain(L.advance(c.vitals, c.rules, c.mode, c.delta));
            assert.deepEqual(got.vitals, c.expect.vitals, 'vitals');
            assert.deepEqual(got.events, c.expect.events, 'events');
            assert.equal(got.charged, c.expect.charged, 'charged');
            assert.equal(got.discarded, c.expect.discarded, 'discarded');
        } else if (c.op === 'apply_care') {
            const got = plain(L.applyCare(c.vitals, c.rules, c.filled));
            assert.deepEqual(got.vitals, c.expect.vitals, 'vitals');
            assert.deepEqual(got.events, c.expect.events, 'events');
        } else {
            assert.fail(`unknown op ${c.op}`);
        }
        if (c.expect.status !== undefined)
            assert.equal(L.status(c.expect.vitals, c.rules), c.expect.status, 'status');
        if (c.expect.windows !== undefined)
            assert.deepEqual(plain(L.windows(c.expect.vitals, c.rules, c.mode)), c.expect.windows, 'windows');
    });
}

test('built-in RULES_V1 equals the presets used by the shared vectors', () => {
    const presets = { steady: null, playtest: null };
    for (const c of VECTORS.cases) {
        const key = c.rules.decay_per_hour[0] === 1000 && c.rules.grace_ms === 21600000 ? 'steady'
            : c.rules.decay_per_hour[0] === 20000 && c.rules.grace_ms === 1080000 ? 'playtest' : null;
        if (key && presets[key] === null) presets[key] = c.rules;
    }
    assert.ok(presets.steady && presets.playtest, 'vectors cover both presets');
    assert.deepEqual(plain(L.RULES_V1.steady), presets.steady);
    assert.deepEqual(plain(L.RULES_V1.playtest), presets.playtest);
});

test('advance charge plus discard equals the clamped delta', () => {
    const r = L.RULES_V1.steady;
    const v = L.vitalsFromMeters([10000, 10000, 10000, 10000], r);
    for (const d of [-1, 0, 1, 59999, 3600000, 28800000, 40000000]) {
        const a = L.advance(v, r, 'permadeath', d);
        assert.equal(a.charged + a.discarded, Math.min(Math.max(d, 0), L.SETTLE_CAP_MS));
    }
});

test('formatActive labels every duration as active time and rounds down', () => {
    assert.equal(L.formatActive(0), 'less than a minute of active time');
    assert.equal(L.formatActive(59999), 'less than a minute of active time');
    assert.equal(L.formatActive(60000), 'about 1 min of active time');
    assert.equal(L.formatActive(7800000), 'about 2 h 10 min of active time');
    assert.equal(L.formatActive(7200000), 'about 2 h of active time');
    assert.equal(L.formatActive(7859999), 'about 2 h 10 min of active time');
    assert.equal(L.formatActive(6600000, true), 'about 1 hour 50 minutes of active time');
    assert.equal(L.formatActive(-1), '');
    assert.equal(L.formatActive(NaN), '');
    assert.equal(L.formatSpan(21600000), '6 h');
    assert.equal(L.formatSpan(1080000), '18 min');
});

test('spec §6 copy templates', () => {
    assert.equal(L.warningText('Ginger', 'thirst', 9600000),
        'Ginger is getting thirsty. Water will help. About 2 h 40 min of active time before it becomes serious.');
    assert.equal(L.criticalText('Ginger', 'thirst', 18600000, 'permadeath'),
        'Ginger urgently needs water. Care now keeps them safe. Without water, Ginger may pass away after about 5 h 10 min of active time.');
    assert.doesNotMatch(L.criticalText('Ginger', 'thirst', 18600000, 'gentle'), DEATHLY, 'gentle never mentions passing');
    assert.equal(L.faintedText('Ginger', 'thirst'), 'Ginger fainted from thirst. Water will revive them; work waits until then.');
    const summary = { name: 'Ginger', species: 'cat', appearance: 'cat', final_level: 3, final_stage: 2, tasks_accepted: 14, end_cause: 'neglect:thirst' };
    const registry = { species: { cat: { stages: [{ stage: 2, name: 'Desk Cat' }] } } };
    assert.equal(L.endText(summary, registry),
        "Ginger has passed away. They reached level 3 as a Desk Cat and helped with 14 accepted tasks. Your work, tasks and history are unchanged. You can adopt a new companion whenever you're ready.");
    assert.match(L.endText({ ...summary, end_cause: 'retired', tasks_accepted: 1 }), /^Ginger has retired\. .*1 accepted task\./);
    assert.equal(L.frozenText('paused'), "Paused: time isn't counting (on a break).");
    assert.equal(L.frozenText('task'), "Paused: time isn't counting (task running).");
    assert.equal(L.frozenText('unarmed'), "Paused: time isn't counting (not set up).");
    assert.equal(L.frozenText('none'), '');
    assert.match(L.frozenText('something_new'), /^Paused: time isn't counting/);
});

test('warning copy names the need, the action and an active-time duration', () => {
    for (const need of ['hunger', 'thirst']) {
        const t = L.warningText('Pip', need, 5000000);
        assert.match(t, new RegExp(L.NEED_FEELING[need]));
        assert.match(t, /(Food|Water) will help/);
        assert.match(t, /of active time/);
    }
    assert.match(L.warningText('Pip', 'mental_health', null), /^Pip is feeling a little low\. Play will help\.$/);
    assert.match(L.warningText('Pip', 'hygiene', null), /^Pip is getting scruffy\. A wash will help\.$/);
});

function snapWith(meters, extra = {}) {
    const r = L.RULES_V1.steady;
    const v = L.vitalsFromMeters(meters, r);
    const w = L.windows(v, r, 'permadeath');
    return {
        life: { life_id: LIFE, name: 'Ginger', species: 'cat', appearance: 'cat', armed: true, mode: 'permadeath', pace: 'steady',
                status: L.status(v, r), fainted: false, death_pending: false, level: 1, stage: 1, xp: 0, ...extra.life },
        lifecycle: {
            frozen_reason: 'none', rules: r, critical: null,
            needs: w.map((n, i) => ({ ...n, action: L.NEED_ACTION[n.need], meter: v.meters[i], hold_ms: v.hold_ms[i], neglect_ms: 0 })),
            ...extra.lifecycle,
        },
    };
}

test('predictions never yield a passing or death string, whatever the attended time', () => {
    for (const mode of ['permadeath', 'gentle']) {
        for (const pace of ['steady', 'playtest']) {
            for (const meters of [[10000, 10000, 10000, 10000], [2800, 2800, 9000, 9000], [0, 0, 0, 0], [100, 50, 0, 0]]) {
                for (const ms of [1, 60000, 3600000, 7200000, 28800000, 86400000, 1e9]) {
                    const s = snapWith(meters, { life: { mode, pace }, lifecycle: { rules: L.RULES_V1[pace] } });
                    const p = L.predict(s, ms);
                    assert.doesNotMatch(p.text, DEATHLY, `${mode}/${pace}/${meters}/${ms}: ${p.text}`);
                    assert.ok(['none', 'warning', 'critical'].includes(p.severity));
                    if (p.text) assert.match(p.text, /^≈ /, 'predictions are labelled approximate');
                }
            }
        }
    }
});

test('a predicted passing is shown only as urgent', () => {
    const s = snapWith([0, 0, 10000, 10000], { life: { status: 'critical' }, lifecycle: {
        critical: { need: 'thirst', action: 'water', grace_ms: 21600000, elapsed_ms: 21000000, remaining_ms: 600000, since_ms: 0 } } });
    s.lifecycle.needs[1].neglect_ms = 7200000;
    s.lifecycle.needs[1].state = 'critical';
    const p = L.predict(s, 3600000);
    assert.equal(p.severity, 'critical');
    assert.equal(p.text, '≈ Ginger may need you urgently. Open CodePet to check.');
});

test('predictions escalate from committed state and respect frozen clocks', () => {
    const full = snapWith([10000, 10000, 10000, 10000]);
    assert.equal(L.predict(full, 60000).severity, 'none');
    assert.equal(L.predict(full, 8 * 3600000).severity, 'warning', '6.6 h → thirst warning');
    assert.equal(L.predict(snapWith([3100, 10000, 10000, 10000]), 3600000).severity, 'warning');
    for (const reason of ['paused', 'task', 'unarmed', 'sleeping', 'pending', 'no_life'])
        assert.equal(L.predict(snapWith([0, 0, 0, 0], { lifecycle: { frozen_reason: reason } }), 1e9).severity, 'none', reason);
    assert.equal(L.predict(snapWith([0, 0, 0, 0], { life: { armed: false } }), 1e9).severity, 'none', 'unarmed lives never predict');
    assert.equal(L.predict(snapWith([0, 0, 0, 0], { life: { death_pending: true } }), 1e9).severity, 'none');
    assert.equal(L.predict(null, 1e9).severity, 'none');
    assert.equal(L.predict(full, -5).severity, 'none');
});

test('statusLine picks the right calm template', () => {
    const healthy = snapWith([10000, 10000, 10000, 10000]);
    assert.equal(L.statusLine(healthy), 'Ginger is doing well.');
    const warn = snapWith([10000, 2800, 10000, 10000]);
    assert.match(L.statusLine(warn), /^Ginger is getting thirsty\. Water will help\. About .* of active time before it becomes serious\.$/);
    const unarmed = snapWith([7000, 7000, 7000, 7000], { life: { armed: false } });
    assert.match(L.statusLine(unarmed), /Set up life rules/);
    const pending = snapWith([0, 0, 0, 0], { life: { death_pending: true } });
    assert.match(L.statusLine(pending), /kept safe/);
    const crit = snapWith([0, 0, 10000, 10000], { life: { status: 'critical' }, lifecycle: { critical: { need: 'hunger', grace_ms: 21600000, elapsed_ms: 3600000, remaining_ms: 18000000 } } });
    assert.match(L.statusLine(crit), /^Ginger urgently needs food\. Care now keeps them safe\. Without food, Ginger may pass away after about 5 h of active time\.$/);
    const fainted = snapWith([0, 0, 10000, 10000], { life: { status: 'critical', fainted: true, mode: 'gentle' }, lifecycle: { critical: { need: 'thirst', grace_ms: 21600000, elapsed_ms: 21600000, remaining_ms: 0 } } });
    assert.equal(L.statusLine(fainted), 'Ginger fainted from thirst. Water will revive them; work waits until then.');
    assert.match(L.statusLine({ life: null, memorial: { count: 0, latest: null } }), /^No companion yet\./);
    assert.match(L.statusLine({ life: null, memorial: { count: 1, latest: { name: 'Biscuit', species: 'dog', appearance: 'dog', final_level: 2, final_stage: 2, tasks_accepted: 5, end_cause: 'neglect:hunger' } } }),
        /^Biscuit has passed away\. They reached level 2 as a Rambler/);
});

test('needLine and lifecycle helpers', () => {
    const s = snapWith([10000, 2800, 10000, 10000]);
    const row = L.needRow(s, 'thirst');
    assert.match(L.needLine(row), /^Hydration 28% · low · water · about .* of active time before it becomes serious$/);
    assert.match(L.needLine(L.needRow(s, 'hunger')), /^Nourishment 100% · fine · food · low in about 8 h of active time$/);
    assert.equal(L.needLine(null), '');
    const v = L.vitalsFromSnapshot(s);
    assert.deepEqual(plain(v.meters), [10000, 2800, 10000, 10000]);
    assert.equal(L.vitalsFromSnapshot({ lifecycle: { needs: [] } }), null);
    assert.equal(L.hasNoLife({ life: null }), true);
    assert.equal(L.hasNoLife({ meters: {} }), false, 'a v0.1 snapshot is not "no life"');
});

test('rules card and adoption copy carries the exact numbers and stays calm', () => {
    const steady = L.rulesLines(L.RULES_V1.steady, 'permadeath', 'Ginger').join('\n');
    assert.match(steady, /30% or less/);
    assert.match(steady, /empty for 2 h of active time/);
    assert.match(steady, /after 6 h more active time/);
    assert.match(steady, /about 17 h of active time with every warning ignored/);
    assert.match(steady, /Pause\/Travel/);
    const gentle = L.rulesLines(L.RULES_V1.steady, 'gentle', 'Ginger').join('\n');
    assert.match(gentle, /never pass away/);
    assert.doesNotMatch(gentle, /passes away/);
    const play = L.rulesLines(L.RULES_V1.playtest, 'permadeath', 'Pip').join('\n');
    assert.match(play, /empty for 6 min/);
    assert.match(play, /after 18 min more active time/);
    assert.match(L.paceExplain('playtest'), /^Playtest: .*about 51 min of active time/);
    assert.match(L.paceExplain('steady'), /about 17 h of active time/);
    assert.match(L.modeExplain('permadeath'), /^Permadeath \(preselected\)/);
    assert.match(L.modeExplain('gentle'), /never pass away/);
    for (const text of [steady, gentle, play, L.paceExplain('playtest'), L.paceExplain('steady'), L.modeExplain('gentle'), L.modeExplain('permadeath')])
        assert.equal(L.bannedIn(text), '', text);
});

test('leniency choices only ever offer longer windows or slower decay', () => {
    assert.deepEqual(plain(L.graceHourChoices(21600000)), Array.from({ length: 42 }, (_, i) => i + 7));
    assert.deepEqual(plain(L.graceHourChoices(1080000)), Array.from({ length: 48 }, (_, i) => i + 1));
    assert.deepEqual(plain(L.graceHourChoices(48 * 3600000)), []);
    assert.deepEqual(plain(L.criticalAfterHourChoices(7200000)), Array.from({ length: 22 }, (_, i) => i + 3));
    for (const p of L.decayPercentChoices()) assert.ok(p >= 25 && p < 100);
});

test('banned list matches the spec and the detector respects word starts', () => {
    assert.deepEqual(plain(L.BANNED), ['kill', 'killed', 'starv', 'your fault', 'abandon', 'neglected you', 'guilt', 'punish', 'streak', 'hurry', '!!!']);
    assert.equal(L.bannedIn('Skills · simulation progress'), '', '"Skills" is not "kill"');
    assert.equal(L.bannedIn('Keep your streak alive'), 'streak');
    assert.equal(L.bannedIn('Starving!'), 'starv');
    assert.equal(L.bannedIn('Hurry up'), 'hurry');
    assert.equal(L.bannedIn('Wow!!!'), '!!!');
    assert.equal(L.bannedIn('abandonment'), 'abandon');
    assert.equal(L.bannedIn(''), '');
    assert.equal(L.bannedIn(null), '');
});

test('roster mirrors the core species order and default names', () => {
    const core = readFileSync(new URL('../../crates/codepet-core/src/species.rs', import.meta.url), 'utf8');
    const ids = [...core.match(/SPECIES_IDS: \[&str; 15\] = \[([\s\S]*?)\];/)[1].matchAll(/"([a-z_]+)"/g)].map((m) => m[1]);
    const names = [...core.match(/DEFAULT_NAMES: \[&str; 15\] = \[([\s\S]*?)\];/)[1].matchAll(/"([A-Za-z]+)"/g)].map((m) => m[1]);
    assert.deepEqual(plain(L.speciesIds()), ids);
    assert.deepEqual(plain(L.ROSTER.map((s) => s.defaultName)), names);
    for (const s of L.ROSTER) {
        assert.equal(s.stages.length, 3);
        assert.match(L.lookLabel(s.id), /^[A-Z][a-z ]+: the [a-z -]+$/);
    }
    assert.equal(L.lookLabel('cat'), 'Cat: the curious desk inspector');
    assert.equal(L.displayName('nope'), 'Companion');
    assert.equal(L.stageName('nope', 2), 'stage 2');
    const registry = { species: { cat: { displayName: 'House Cat', personality: 'the registry text', stages: [{ stage: 1, name: 'Kitten' }], careLabels: { water: 'Fresh water' } } } };
    assert.equal(L.displayName('cat', registry), 'House Cat');
    assert.equal(L.stageName('cat', 1, registry), 'Kitten');
    assert.equal(L.stageName('cat', 3, registry), 'Maned Inspector', 'roster fallback per stage');
    assert.equal(L.careLabel('cat', 'water', registry), 'Fresh water');
    assert.equal(L.careLabel('cat', 'food', registry), 'Food');
});

test('fallback roster matches the generated registry (bar and overlay name stages alike)', () => {
    const reg = JSON.parse(readFileSync(new URL('../assets/pets/registry.json', import.meta.url), 'utf8'));
    assert.deepEqual(plain(L.speciesIds()), reg.order);
    for (const s of L.ROSTER) {
        const r = reg.species[s.id];
        for (const k of ['displayName', 'defaultName', 'personality', 'habitat'])
            assert.equal(s[k], r[k], `${s.id}.${k}`);
        assert.deepEqual(plain(s.stages), r.stages.map((x) => x.name), `${s.id} stage names`);
        for (const st of [1, 2, 3])
            assert.equal(L.stageName(s.id, st), L.stageName(s.id, st, reg), `${s.id} stage ${st} with and without the registry`);
    }
});

test('headline and next-stage text', () => {
    const life = { name: 'Ginger', appearance: 'cat', species: 'cat', level: 2, stage: 2, xp: 120, next_stage: { stage: 3, min_level: 4, xp: 400 } };
    assert.equal(L.lifeHeadline(life), 'Ginger · Level 2 · Desk Prowler');
    assert.equal(L.nextStageText(life), 'Grows into a Maned Inspector at level 4 (280 XP to go)');
    assert.match(L.nextStageText({ ...life, next_stage: null }), /^Fully grown/);
});

test('normalizeName mirrors the core rule', () => {
    assert.equal(L.normalizeName('  Ginger  '), 'Ginger');
    assert.equal(L.normalizeName(''), '');
    assert.equal(L.normalizeName('   '), '');
    assert.equal(L.normalizeName('a'.repeat(24)), 'a'.repeat(24));
    assert.equal(L.normalizeName('a'.repeat(25)), '');
    assert.equal(L.normalizeName('Gin‮ger'), 'Ginger', 'bidi controls are stripped');
    assert.equal(L.normalizeName('\u{1F431}'.repeat(24)), '\u{1F431}'.repeat(24), '24 emoji = 96 bytes is allowed');
    assert.equal(L.normalizeName(42), '');
});
