// Node tests for omarchy-plugin/CodePetBar.js (the bar plugin's pure model).
// Run: node --test qml/tests/test_bar_model.mjs
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

const M = loadLibrary('../../omarchy-plugin/CodePetBar.js');
const L = loadLibrary('../shared/LifeText.js');
const plain = (x) => JSON.parse(JSON.stringify(x));
const LIFE = 'life-0123456789abcdef01234567';
const BANNED = /kill|starv|your fault|abandon|neglected you|guilt|punish|streak|hurry|!!!|\bdie\b|\bdied\b|\bdeath\b|\bdead\b/i;

const REGISTRY = {
    schemaVersion: 1,
    species: {
        cat: {
            displayName: 'Cat', defaultName: 'Ginger',
            careLabels: { food: 'Kibble', water: 'Fresh water', play: 'Yarn tuft', wash: 'Brush', bathroom: 'Litter', sleep: 'Basket' },
            stages: [{ stage: 1, key: 'young', name: 'Kitten', minLevel: 1 },
                     { stage: 2, key: 'grown', name: 'Desk Cat', minLevel: 2 },
                     { stage: 3, key: 'mature', name: 'Grand Inspector', minLevel: 4 }],
        },
    },
};

function need(n, state, extra = {}) {
    const action = { hunger: 'food', thirst: 'water', mental_health: 'play', hygiene: 'wash' }[n];
    return { need: n, action, state, until_warning_ms: null, until_critical_ms: null, until_end_ms: null, ...extra };
}

// A real `codepet rpc` export (armed steady permadeath life, full meters). The storage test
// `the_ui_fixture_has_exactly_the_exported_shape` keeps it identical in shape to Rust's output.
const RUST_EXPORT = JSON.parse(readFileSync(new URL('./fixtures/bar-state.v1.json', import.meta.url), 'utf8'));

function barState(over = {}) {
    const base = {
        schema: 'codepet.bar.v1', state_version: 812, db_id: 'db-0123456789abcdef01234567', generated_at_ms: Date.UTC(2026, 9, 15, 14, 5), app_version: '0.2.0',
        ruleset: 'lifecycle-v1',
        life: { life_id: LIFE, ordinal: 1, name: 'Ginger', appearance: 'cat', stage: 2, level: 2, xp: 120,
                next_stage_xp: 400, status: 'healthy', mode: 'permadeath', armed: true, pace: 'steady',
                paused: false, fainted: false, death_pending: false },
        meters: { hunger: 6100, thirst: 8000, mental_health: 8000, hygiene: 9000 },
        needs: [need('hunger', 'ok', { until_critical_ms: 46_800_000, until_warning_ms: 25_000_000 }),
                need('thirst', 'ok', { until_critical_ms: 30_000_000, until_warning_ms: 12_000_000 }),
                need('mental_health', 'ok'), need('hygiene', 'ok')],
        critical: null,
        care: { sleeping: false, sleep_remaining_ms: 120000, sleep_started_at_ms: null, sleep_due: false,
                bathroom_due: false, ready_to_work: true, readiness_reasons: [] },
        task: null,
        presence: { wanted: true, life_id: LIFE, tick_ms: 60000, idle_timeout_s: 180 },
        unseen_events: [],
        memorial: { count: 0, latest_name: null },
        // v0.3 blocks (docs/v0.3/contracts/bar.v2.schema.json), WP0 defaults.
        enabled: true,
        agent: { state: 'idle', apps: [], stale_at_ms: null, finished_seq: 0, finished_at_ms: null,
                 finished_app: null, finished_treat: false, finished_jar_full: false, hello_seq: 0,
                 hello_at_ms: null, hello_app: null, test: null, error_at_ms: null, error_app: null,
                 helpers_busy: false },
        treats: { jar: 0, full: false },
        clients: {},
    };
    const out = { ...base, ...over };
    if (over.life) out.life = { ...base.life, ...over.life };
    if (over.care) out.care = { ...base.care, ...over.care };
    return out;
}

const cfg = (label = 'name') => M.config({ label });

// ------------------------------------------------------------------ parse

test('parse accepts codepet.bar.v1 and refuses everything else', () => {
    const text = JSON.stringify(barState());
    assert.equal(M.parse(text).state_version, 812);
    assert.equal(M.parse(''), null);
    assert.equal(M.parse(null), null);
    assert.equal(M.parse('{'), null);
    assert.equal(M.parse('[]'), null);
    assert.equal(M.parse(JSON.stringify({ ...barState(), schema: 'codepet.bar.v2' })), null);
    assert.equal(M.parse(JSON.stringify({ ...barState(), state_version: -1 })), null);
    assert.equal(M.parse(JSON.stringify({ ...barState(), state_version: 1.5 })), null);
    assert.equal(M.parse(JSON.stringify({ ...barState(), life: 'Ginger' })), null);
    assert.ok(M.parse(JSON.stringify({ ...barState(), life: null })));
});

test('parse bounds the file to 16 KiB (bytes, not characters)', () => {
    const doc = barState();
    const pad = 16384 - Buffer.byteLength(JSON.stringify({ ...doc, pad: '' }));
    assert.ok(M.parse(JSON.stringify({ ...doc, pad: 'x'.repeat(pad) })), 'exactly 16384 bytes is accepted');
    assert.equal(M.parse(JSON.stringify({ ...doc, pad: 'x'.repeat(pad + 1) })), null, '16385 bytes is refused');
    // Multi-byte characters count by UTF-8 bytes.
    assert.equal(M.parse(JSON.stringify({ ...doc, pad: '★'.repeat(Math.ceil(pad / 3) + 1) })), null);
});

test('parseRegistry tolerates missing or malformed registries', () => {
    assert.equal(M.parseRegistry(''), null);
    assert.equal(M.parseRegistry('{"species": []}'), null);
    assert.ok(M.parseRegistry(JSON.stringify(REGISTRY)));
    assert.equal(M.speciesName(null, 'cat'), 'Companion');
    assert.equal(M.stageName(null, 'cat', 2), 'stage 2');
    assert.equal(M.careLabel(null, 'cat', 'food'), 'Food');
    assert.equal(M.speciesName(REGISTRY, 'cat'), 'Cat');
    assert.equal(M.stageName(REGISTRY, 'cat', 3), 'Grand Inspector');
    assert.equal(M.careLabel(REGISTRY, 'cat', 'water'), 'Fresh water');
    assert.equal(M.careLabel(REGISTRY, 'cat', 'routine'), 'Routine');
    assert.equal(M.stageMinLevel(null, 'cat', 3), 4);
});

// ------------------------------------------------------------------ settings + paths

test('ownSettings reads this widget entry from bar.layout and falls back to injected', () => {
    const shell = JSON.stringify({ bar: { layout: { left: [{ id: 'other', label: 'x' }], center: [],
        right: [{ id: 'omarchy.tray' }, { id: 'greyforge.codepet', label: 'status', stateHome: '/tmp/qa' }] } } });
    assert.deepEqual(plain(M.ownSettings(shell, 'greyforge.codepet', { label: 'none' })), { label: 'status', stateHome: '/tmp/qa' });
    // Legacy layout keys directly under bar.
    const legacy = JSON.stringify({ bar: { center: [{ id: 'greyforge.codepet', label: 'none' }] } });
    assert.deepEqual(plain(M.ownSettings(legacy, 'greyforge.codepet', {})), { label: 'none' });
    // Missing entry, broken file, oversize file: the injected settings win.
    assert.deepEqual(plain(M.ownSettings('{"bar":{"layout":{}}}', 'greyforge.codepet', { label: 'none', id: 'x' })), { label: 'none' });
    assert.deepEqual(plain(M.ownSettings('not json', 'greyforge.codepet', { presence: false })), { presence: false });
    assert.deepEqual(plain(M.ownSettings('', 'greyforge.codepet', null)), {});
});

test('config normalizes settings to the manifest defaults', () => {
    assert.deepEqual(plain(M.config({})), { label: 'name', presence: true, overlayOnLoad: false, binary: '', stateHome: '' });
    assert.deepEqual(plain(M.config(null)), plain(M.config({})));
    const c = M.config({ label: 'bogus', presence: 'false', overlayOnLoad: true, binary: 'relative/codepet', stateHome: '../x' });
    assert.equal(c.label, 'name');
    assert.equal(c.presence, false);
    assert.equal(c.overlayOnLoad, true);
    assert.equal(c.binary, '', 'relative binary ignored');
    assert.equal(c.stateHome, '', 'relative stateHome ignored');
    assert.equal(M.config({ stateHome: '/srv/../etc' }).stateHome, '', 'dot-dot stateHome ignored');
    assert.equal(M.config({ stateHome: '/home/u/qa-state/' }).stateHome, '/home/u/qa-state');
});

test('paths: defaults, XDG overrides, and a test stateHome for both files and children', () => {
    const env = { HOME: '/home/u' };
    const p = M.paths(M.config({}), env);
    assert.equal(p.stateDir, '/home/u/.local/state/codepet');
    assert.equal(p.barState, '/home/u/.local/state/codepet/bar-state.json');
    assert.equal(p.presence, '/home/u/.local/state/codepet/presence.json');
    assert.equal(p.runtime, '/home/u/.local/share/codepet/current/');
    assert.equal(p.registry, '/home/u/.local/share/codepet/current/qml/assets/pets/registry.json');
    assert.equal(p.binary, '/home/u/.local/bin/codepet');
    assert.equal(p.launcher, '/home/u/.local/bin/codepet-overlay');
    assert.equal(p.shellConfig, '/home/u/.config/omarchy/shell.json');
    assert.equal(p.testState, false);
    assert.deepEqual(plain(p.childEnv), {});
    assert.deepEqual(plain(p.launcherEnv), []);

    const x = M.paths(M.config({}), { HOME: '/home/u', XDG_STATE_HOME: '/s', XDG_DATA_HOME: '/d', XDG_CONFIG_HOME: '/c' });
    assert.equal(x.barState, '/s/codepet/bar-state.json');
    assert.equal(x.runtime, '/d/codepet/current/');
    assert.equal(x.shellConfig, '/c/omarchy/shell.json');
    assert.deepEqual(plain(x.childEnv), {}, 'an inherited XDG_STATE_HOME is not re-exported');

    const t = M.paths(M.config({ stateHome: '/home/u/qa', binary: '/opt/codepet' }), { HOME: '/home/u', XDG_STATE_HOME: '/s' });
    assert.equal(t.barState, '/home/u/qa/codepet/bar-state.json');
    assert.equal(t.presence, '/home/u/qa/codepet/presence.json');
    assert.equal(t.binary, '/opt/codepet');
    assert.equal(t.testState, true);
    assert.deepEqual(plain(t.childEnv), { XDG_STATE_HOME: '/home/u/qa', CODEPET_TEST_STATE: '1' },
        'a test state home also tells the CLI (TEST_STATE: no Connect, Stop watching or ready check)');
    assert.deepEqual(plain(t.launcherEnv), ['CODEPET_BIN=/opt/codepet', 'XDG_STATE_HOME=/home/u/qa', 'CODEPET_TEST_STATE=1'],
        'the configured binary and a test state home reach the desktop-pet launcher');
    const b = M.paths(M.config({ binary: '/opt/codepet' }), { HOME: '/home/u' });
    assert.deepEqual(plain(b.launcherEnv), ['CODEPET_BIN=/opt/codepet']);
    assert.deepEqual(plain(b.childEnv), {});
});

test('no path ever points inside the plugin folder', () => {
    const p = M.paths(M.config({ stateHome: '/q' }), { HOME: '/home/u' });
    for (const [k, v] of Object.entries(p)) {
        if (typeof v !== 'string') continue;
        assert.doesNotMatch(v, /omarchy\/plugins/, `${k} must stay outside the plugin dir`);
    }
});

// ------------------------------------------------------------------ thumbnails + methods

test('thumbPath accepts species ids and stages 1..3 only', () => {
    assert.equal(M.thumbPath('cat', 1), 'qml/assets/pets/cat/thumb-s1.webp');
    assert.equal(M.thumbPath('red_panda', 3), 'qml/assets/pets/red_panda/thumb-s3.webp');
    for (const bad of ['../x', '../../etc', 'Cat', 'c', 'a'.repeat(25), 'cat/..', 'cat.webp', '', null, 7]) {
        assert.equal(M.thumbPath(bad, 1), '', `rejects ${bad}`);
    }
    for (const stage of [0, 4, '1', 1.5, null]) assert.equal(M.thumbPath('cat', stage), '');
});

test('allowedMethod permits only read and care methods, never task.*', () => {
    for (const m of ['state.get', 'pet.care', 'pet.sleep', 'pet.wake', 'pet.pause', 'pet.events_seen']) {
        assert.equal(M.allowedMethod(m), true, m);
    }
    for (const m of ['task.start', 'task.cancel', 'task.accept', 'task.preflight', 'task.discard', 'task.recover',
                     'pet.adopt', 'pet.retire', 'pet.rules', 'pet.select', 'pet.configure_life', 'settings.set',
                     'STATE.GET', '', undefined]) {
        assert.equal(M.allowedMethod(m), false, String(m));
    }
    assert.equal(M.allowedCareAction('routine'), true);
    assert.equal(M.allowedCareAction('sleep'), false);
});

test('retryKey reuses one key per press until confirmed, and changes with the action', () => {
    const a = M.retryKey(null, 'pet.care{"action":"water"}', 1000, 1);
    assert.match(a.key, /^[A-Za-z0-9._:-]{1,128}$/, 'valid protocol id');
    const again = M.retryKey(a, 'pet.care{"action":"water"}', 5000, 2);
    assert.equal(again.key, a.key, 'unconfirmed retry of the same press reuses the key');
    const other = M.retryKey(a, 'pet.care{"action":"food"}', 5000, 3);
    assert.notEqual(other.key, a.key, 'a different action gets a fresh key');
    const afterSuccess = M.retryKey(null, 'pet.care{"action":"water"}', 9000, 4);
    assert.notEqual(afterSuccess.key, a.key, 'a confirmed press is never replayed');
    assert.match(M.newRequestId(Date.now(), 9), /^[A-Za-z0-9._:-]{1,128}$/);
});

test('a retry key never outlives its press: confirmed or expired keys are never replayed', () => {
    const sig = 'pet.care{"action":"water"}';
    const t0 = 1_000_000;
    const a = M.retryKey(null, sig, t0, 1);
    // Confirmed: dropped.
    assert.equal(M.retryAfterResponse(a, { ok: true, result: {} }, t0 + 100), null);
    // Unconfirmed (lost response, timeout, transport or backend failure): kept briefly.
    for (const code of ['CLIENT_TIMEOUT', 'TRANSPORT', 'PROTOCOL_MISMATCH', 'INVALID_STATE']) {
        const kept = M.retryAfterResponse(a, { ok: false, error: { code } }, t0 + 15_000);
        assert.equal(kept.key, a.key, code);
        assert.equal(kept.expiresAt, t0 + 15_000 + M.RETRY_WINDOW_MS);
        // An immediate retry of the same press reuses it...
        assert.equal(M.retryKey(kept, sig, t0 + 20_000, 2).key, a.key);
        // ...a press of the same button much later is a new press with a new key, so the
        // backend applies new care instead of replaying the old receipt.
        assert.notEqual(M.retryKey(kept, sig, t0 + 15_000 + M.RETRY_WINDOW_MS + 1, 3).key, a.key);
        assert.notEqual(M.retryKey(kept, sig, t0 + 3 * 3_600_000, 4).key, a.key, 'hours later');
    }
    assert.ok(M.RETRY_WINDOW_MS > 15_000 && M.RETRY_WINDOW_MS <= 300_000);
    // A key without an expiry (older state) is never reused.
    assert.notEqual(M.retryKey({ key: 'bar-x-1', signature: sig }, sig, t0, 5).key, 'bar-x-1');
});

// ------------------------------------------------------------------ summarize

const STATUS_CASES = {
    missing: null,
    no_life: barState({ life: null }),
    death_pending: barState({ life: { death_pending: true, status: 'critical' }, task: { state: 'review_ready', category: 'thirst' } }),
    fainted: barState({ life: { status: 'critical', fainted: true, mode: 'gentle' } }),
    critical: barState({ life: { status: 'critical' }, needs: [need('thirst', 'critical', { until_critical_ms: 0 })],
                         critical: { need: 'thirst', action: 'water', grace_ms: 21_600_000, elapsed_ms: 3_600_000, remaining_ms: 18_000_000 } }),
    unarmed: barState({ life: { armed: false } }),
    warning: barState({ life: { status: 'warning' }, needs: [need('thirst', 'low', { until_critical_ms: 9_600_000 })] }),
    review: barState({ task: { state: 'review_ready', category: 'hunger' } }),
    sleeping: barState({ care: { sleeping: true } }),
    paused: barState({ life: { paused: true } }),
    working: barState({ task: { state: 'running', category: 'hunger' } }),
    evolved: barState({ unseen_events: [{ seq: 88, stage: 2 }] }),
    healthy: barState(),
};

test('every status has a non-colour glyph (healthy and missing have none by design)', () => {
    const expected = { missing: '', no_life: '+', death_pending: '✿', fainted: '!!', critical: '!!', unarmed: '◦',
        warning: '!', review: '?', sleeping: 'z', paused: '‖', working: '•', evolved: '★', healthy: '' };
    for (const [status, st] of Object.entries(STATUS_CASES)) {
        const s = M.summarize(st, cfg(), { registry: REGISTRY });
        assert.equal(s.status, status, `status for ${status}`);
        assert.equal(s.glyph, expected[status], `glyph for ${status}`);
        assert.ok(s.accessibleName.startsWith('CodePet'), `accessible name for ${status}`);
        assert.doesNotMatch(s.accessibleName, BANNED, `calm copy for ${status}`);
    }
    // Every status in the precedence list is covered by a glyph entry.
    for (const status of M.STATUS_ORDER) assert.ok(status in M.STATUS_GLYPH, status);
    const glyphs = Object.values(expected).filter(Boolean);
    assert.equal(new Set(glyphs.filter(g => g !== '!!')).size, glyphs.filter(g => g !== '!!').length, 'glyphs are distinct');
});

test('severity is derived from status', () => {
    assert.equal(M.summarize(STATUS_CASES.critical, cfg()).severity, 'critical');
    assert.equal(M.summarize(STATUS_CASES.fainted, cfg()).severity, 'critical');
    assert.equal(M.summarize(STATUS_CASES.warning, cfg()).severity, 'warning');
    assert.equal(M.summarize(STATUS_CASES.healthy, cfg()).severity, 'none');
});

test('labels: name | status | none', () => {
    assert.equal(M.summarize(null, cfg()).label, 'CodePet');
    assert.equal(M.summarize(STATUS_CASES.no_life, cfg()).label, 'adopt');
    assert.equal(M.summarize(STATUS_CASES.healthy, cfg('name')).label, 'Ginger');
    assert.equal(M.summarize(STATUS_CASES.warning, cfg('name')).label, 'Ginger');
    assert.equal(M.summarize(STATUS_CASES.warning, cfg('status')).label, 'Ginger · water!');
    assert.equal(M.summarize(STATUS_CASES.critical, cfg('status')).label, 'Ginger · water!!');
    assert.equal(M.summarize(STATUS_CASES.unarmed, cfg('status')).label, 'Ginger · set up');
    assert.equal(M.summarize(STATUS_CASES.healthy, cfg('status')).label, 'Ginger');
    assert.equal(M.summarize(STATUS_CASES.warning, cfg('none')).label, '');
    assert.equal(M.summarize(null, cfg('none')).label, '');
    const long = barState({ life: { name: 'A'.repeat(80) } });
    assert.ok(M.summarize(long, cfg()).label.length <= 24, 'names are bounded');
});

test('accessible name reads name, stage, level, the need and the task', () => {
    const st = barState({ life: { status: 'warning' }, needs: [need('thirst', 'low', { until_critical_ms: 6_600_000 })],
                          task: { state: 'running', category: 'thirst' } });
    const s = M.summarize(st, cfg(), { registry: REGISTRY });
    assert.equal(s.accessibleName,
        'CodePet: Ginger, Desk Cat, level 2. Warning: hydration low; water within about 1 hour 50 minutes of active time. Task: running (simulation).');
});

test('the predicted badge escalates from uncredited attended time, never past !!, never to an ending', () => {
    const st = STATUS_CASES.healthy; // thirst: warning in 12,000,000 ms, critical in 30,000,000 ms
    assert.equal(M.summarize(st, cfg(), { attendedSinceMs: 0 }).glyph, '');
    assert.equal(M.summarize(st, cfg(), { attendedSinceMs: 11_999_999 }).glyph, '');
    let s = M.summarize(st, cfg('status'), { attendedSinceMs: 12_000_000 });
    assert.equal(s.glyph, '!');
    assert.equal(s.predicted, true);
    assert.equal(s.label, 'Ginger · ≈ water!', 'predicted text is labelled ≈');
    s = M.summarize(st, cfg(), { attendedSinceMs: 30_000_000 });
    assert.equal(s.glyph, '!!');
    assert.equal(s.severity, 'critical');
    for (const t of [30_000_001, 60_000_000, 999_999_999]) {
        s = M.summarize(st, cfg(), { attendedSinceMs: t });
        assert.equal(s.glyph, '!!', 'capped at !!');
        assert.notEqual(s.status, 'death_pending');
        assert.notEqual(s.glyph, '✿');
        assert.notEqual(s.glyph, '+');
        assert.doesNotMatch(s.accessibleName, /pass away|passed away|memorial/);
        assert.doesNotMatch(M.lifecycleText(st, { predicted: s.status }), /pass away|passed away|memorial/);
    }
    // Committed severity never goes down because of a prediction.
    assert.equal(M.summarize(STATUS_CASES.critical, cfg(), { attendedSinceMs: 0 }).glyph, '!!');
    // A predicted warning overrides lower-precedence statuses but not a committed one.
    assert.equal(M.summarize(STATUS_CASES.working, cfg(), { attendedSinceMs: 12_000_000 }).glyph, '!');
    assert.equal(M.summarize(STATUS_CASES.warning, cfg(), { attendedSinceMs: 1_000_000 }).predicted, false);
    // ...while a predicted critical does escalate a committed warning.
    const esc = M.summarize(STATUS_CASES.warning, cfg(), { attendedSinceMs: 9_600_000 });
    assert.equal(esc.glyph, '!!');
    assert.equal(esc.predicted, true);
    assert.equal(esc.committedStatus, 'warning');
});

test('the predicted ! fires from the real Rust bar-state shape (no hand-added fields)', () => {
    const st = M.parse(JSON.stringify(RUST_EXPORT));
    assert.ok(st, 'the real export parses');
    const keys = (o) => Object.keys(o).sort();
    for (const n of st.needs)
        assert.deepEqual(keys(n), ['action', 'need', 'state', 'until_critical_ms', 'until_end_ms', 'until_warning_ms']);
    // The hand-written fixtures in this file use exactly the exported need fields.
    for (const n of barState().needs) assert.deepEqual(keys(n), keys(st.needs[0]));
    assert.deepEqual(keys(barState()), keys(st));
    const thirst = st.needs.find((n) => n.need === 'thirst');
    assert.ok(thirst.until_warning_ms > 0 && thirst.until_warning_ms < thirst.until_critical_ms);
    const before = M.summarize(st, cfg('status'), { attendedSinceMs: thirst.until_warning_ms - 60000 });
    assert.equal(before.glyph, '');
    assert.equal(before.predicted, false);
    const warn = M.summarize(st, cfg('status'), { attendedSinceMs: thirst.until_warning_ms });
    assert.equal(warn.glyph, '!', 'a predicted warning badge from the real file');
    assert.equal(warn.predicted, true);
    assert.equal(warn.label, 'Ginger · ≈ water!');
    const crit = M.summarize(st, cfg('status'), { attendedSinceMs: thirst.until_critical_ms });
    assert.equal(crit.glyph, '!!');
    assert.equal(crit.predicted, true);
    // Non-lethal needs (mood, hygiene) are predicted too, never past a warning.
    const nonLethal = { ...st, needs: st.needs.filter((n) => n.need === 'mental_health' || n.need === 'hygiene') };
    const mood = nonLethal.needs.find((n) => n.need === 'mental_health');
    assert.equal(M.predict(nonLethal, mood.until_warning_ms - 1).severity, 'none');
    assert.deepEqual(plain(M.predict(nonLethal, mood.until_warning_ms)), { severity: 'warning', need: 'mental_health', action: 'play' });
    assert.equal(M.predict(nonLethal, 1e12).severity, 'warning');
});

test('no prediction when time is frozen', () => {
    const t = 99_000_000;
    for (const st of [STATUS_CASES.unarmed, STATUS_CASES.paused, STATUS_CASES.death_pending, STATUS_CASES.fainted,
                      barState({ presence: { wanted: false, life_id: LIFE } }),
                      barState({ presence: { wanted: true, life_id: 'life-ffffffffffffffffffffffff' } }),
                      barState({ presence: null }), STATUS_CASES.no_life, null]) {
        assert.equal(M.predict(st, t).severity, 'none');
    }
    // Sleep: the remaining nap is discarded first (Rust does the same).
    const napping = barState({ care: { sleeping: true, sleep_remaining_ms: 120000 } });
    assert.equal(M.predict(napping, 12_000_000).severity, 'none');
    assert.equal(M.predict(napping, 12_120_000).severity, 'warning');
});

// ------------------------------------------------------------------ text

test('formatActive and formatAsOf', () => {
    assert.equal(M.formatActive(0), 'less than a minute of active time');
    assert.equal(M.formatActive(59_999), 'less than a minute of active time');
    assert.equal(M.formatActive(60_000), 'about 1 min of active time');
    assert.equal(M.formatActive(7_800_000), 'about 2 h 10 min of active time');
    assert.equal(M.formatActive(7_200_000), 'about 2 h of active time');
    assert.equal(M.formatActive(6_600_000, true), 'about 1 hour 50 minutes of active time');
    assert.equal(M.formatActive(3_660_000, true), 'about 1 hour 1 minute of active time');
    assert.equal(M.formatActive(-1), '');
    assert.equal(M.formatActive(NaN), '');
    assert.equal(M.formatAsOf(Date.UTC(2026, 9, 15, 14, 5)), 'as of 14:05');
    assert.equal(M.formatAsOf(0), '');
});

test('lifecycle text follows the copy rules', () => {
    assert.equal(M.lifecycleText(barState({ life: { status: 'warning' }, needs: [need('thirst', 'low', { until_critical_ms: 9_600_000 })] })),
        'Ginger is getting thirsty. Water will help. About 2 h 40 min of active time before it becomes serious.');
    assert.equal(M.lifecycleText(STATUS_CASES.critical),
        'Ginger urgently needs water. Care now keeps them safe. Without water, Ginger may pass away after about 5 h of active time.');
    assert.equal(M.lifecycleText(barState({ life: { status: 'critical', mode: 'gentle' }, critical: { need: 'thirst', action: 'water', remaining_ms: 100 } })),
        'Ginger urgently needs water. Care now keeps them safe.', 'gentle mode never mentions an ending');
    assert.equal(M.lifecycleText(barState({ life: { status: 'critical', fainted: true }, critical: { need: 'thirst', action: 'water' } })),
        'Ginger fainted from thirst. Water will revive them; work waits until then.');
    assert.match(M.lifecycleText(STATUS_CASES.unarmed), /^Paused: time isn't counting \(not set up\)/);
    assert.equal(M.lifecycleText(STATUS_CASES.paused), "Paused: time isn't counting (on a break).");
    assert.equal(M.lifecycleText(STATUS_CASES.working), "Paused: time isn't counting (task running).");
    assert.match(M.lifecycleText(STATUS_CASES.no_life), /adopt a new companion whenever you're ready/);
    assert.match(M.lifecycleText(barState({ life: null, memorial: { count: 1, latest_name: 'Biscuit' } })), /^Biscuit is remembered/);
    assert.equal(M.lifecycleText(STATUS_CASES.death_pending), L.pendingText('Ginger'),
        'death pending uses the same words as the life UI');
    assert.match(M.lifecycleText(STATUS_CASES.death_pending), /will pass away when the current task is finished/);
    assert.doesNotMatch(M.lifecycleText(STATUS_CASES.death_pending), /passed away/, 'never says it already passed');
    assert.match(M.summarize(STATUS_CASES.death_pending, cfg(), { registry: REGISTRY }).accessibleName,
        /grace time has run out\. They will pass away when the current task is finished; your work is kept\. /);
    assert.equal(M.lifecycleText(null), 'Open CodePet once to set up.');
    assert.equal(M.lifecycleText(STATUS_CASES.healthy, { predicted: 'critical' }), '≈ Ginger may need you urgently. Open CodePet to check.');
    for (const st of Object.values(STATUS_CASES)) {
        for (const predicted of ['', 'warning', 'critical']) {
            assert.doesNotMatch(M.lifecycleText(st, { predicted }), BANNED);
            assert.doesNotMatch(M.tooltip(st, cfg(), { registry: REGISTRY, attendedSinceMs: 99_000_000 }), BANNED);
        }
    }
});

test('header, badges, evolution and need rows', () => {
    const st = barState();
    assert.equal(M.headerText(st, REGISTRY), 'Ginger · Desk Cat · Level 2 · 120 XP · grows into a Grand Inspector at level 4 (280 XP to go)');
    assert.equal(M.nextEvolutionText(barState({ life: { stage: 3 } }), REGISTRY), 'fully grown');
    assert.equal(M.nextEvolutionText(barState({ life: { stage: 1, xp: 10, next_stage_xp: 80 } }), null), 'grows into stage 2 at level 2 (70 XP to go)');
    assert.equal(M.badgesText(barState({ life: { pace: 'playtest', paused: true } })), 'Permadeath · Playtest · Paused');
    assert.equal(M.badgesText(barState({ life: { mode: 'gentle', armed: false } })), 'Gentle · Not set up');
    const rows = M.needRows(barState({ needs: [need('thirst', 'low', { until_critical_ms: 9_600_000 })] }), REGISTRY);
    assert.equal(rows.length, 4);
    assert.deepEqual(plain(rows.map(r => r.need)), ['hunger', 'thirst', 'mental_health', 'hygiene']);
    assert.equal(rows[0].percent, 61);
    assert.equal(rows[1].stateText, 'low');
    assert.equal(rows[1].actionLabel, 'Fresh water');
    assert.equal(rows[1].timeText, 'serious after about 2 h 40 min of active time');
    assert.equal(rows[2].stateText, 'fine');
    assert.equal(M.needRows(null, null)[0].percent, -1);
});

test('readiness, sleep countdown and unseen events', () => {
    assert.equal(M.readinessText(barState()), 'Ready to work.');
    const notReady = barState({ care: { ready_to_work: false, readiness_reasons: ['Needs water', 'x'.repeat(400), 3] } });
    const text = M.readinessText(notReady);
    assert.match(text, /^Not ready: Needs water/);
    assert.ok(text.length < 200);
    const napping = barState({ care: { sleeping: true, sleep_remaining_ms: 120000 } });
    assert.equal(M.sleepRemainingMs(napping, napping.generated_at_ms + 30000), 90000);
    assert.equal(M.sleepRemainingMs(napping, napping.generated_at_ms + 999999), 0);
    assert.equal(M.sleepRemainingMs(barState(), Date.now()), -1);
    assert.equal(M.formatClock(90000), '1:30');
    assert.equal(M.maxUnseenSeq([{ seq: 3, stage: 2 }, { seq: 9, kind: 'evolved', stage: 3 }, { seq: 12, kind: 'other' }]), 9);
    assert.equal(M.maxUnseenSeq([]), -1);
    assert.equal(M.maxUnseenSeq(null), -1);
    assert.equal(M.grewText('Ginger', REGISTRY, 'cat', 2), '★ Ginger grew into a Desk Cat!');
    assert.equal(M.lifeButtonLabel(null), 'Adopt…');
    assert.equal(M.lifeButtonLabel(STATUS_CASES.unarmed), 'Set up…');
    assert.equal(M.lifeButtonLabel(STATUS_CASES.healthy), 'Life & memorial…');
});

test('tooltip is plain, bounded text with the as-of time', () => {
    const t = M.tooltip(STATUS_CASES.warning, cfg(), { registry: REGISTRY });
    assert.match(t, /^Ginger · Desk Cat · Level 2\n! Ginger is getting thirsty/);
    assert.match(t, /as of 14:05 · simulation only/);
    assert.ok(t.length < 1000);
    assert.equal(M.tooltip(null, cfg()), 'CodePet\nOpen CodePet once to set up.');
});
