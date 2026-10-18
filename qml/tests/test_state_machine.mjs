// Node tests for qml/shared/PetStateMachine.js and qml/shared/PetCatalog.js (T5, SPEC §17.6).
// Run: node --test qml/tests/test_state_machine.mjs
// PetCatalog is exercised against the generated fixture pack in fixtures/pets/.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import vm from 'node:vm';

const read = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8');
const plain = (x) => JSON.parse(JSON.stringify(x));

function loadLibrary(rel, globals = {}) {
    const src = read(rel);
    assert.match(src, /^\.pragma library\n/, `${rel} must stay a .pragma library`);
    const ctx = { ...globals };
    vm.createContext(ctx);
    vm.runInContext(src.replace(/^\.pragma library$/m, '').replace(/^\.import .*$/gm, ''), ctx, { filename: rel });
    return ctx;
}

const SM = loadLibrary('../shared/PetStateMachine.js');
const FIXTURE = JSON.parse(read('./fixtures/pets/registry.json'));
const C = loadLibrary('../shared/PetCatalog.js', { PetRegistry: { REGISTRY: FIXTURE } });
const BANNED = /kill|starv|your fault|abandon|neglected you|guilt|punish|streak|hurry|!!!|\bdie\b|\bdied\b|\bdeath\b|\bdead\b/i;
const STATES = ['idle', 'locomotion', 'food', 'water', 'play', 'wash', 'bathroom', 'sleep_enter',
                'sleep', 'sleep_exit', 'celebrate', 'working', 'waiting', 'needs_care', 'error'];

test('PetStateMachine imports nothing (pure)', () => {
    assert.doesNotMatch(read('../shared/PetStateMachine.js'), /^\s*(import|\.import)\b/m);
});

test('PetCatalog imports only the generated registry', () => {
    const imports = read('../shared/PetCatalog.js').match(/^\.import .*$/gm);
    assert.deepEqual(imports, ['.import "PetRegistry.js" as PetRegistry']);
});

test('PetSprite stays passive: no Timer, no requests, no animations', () => {
    const src = read('../shared/PetSprite.qml');
    assert.ok(!src.includes('Timer {'), 'PetSprite.qml must not contain a Timer');
    assert.ok(!src.includes('.request('), 'PetSprite.qml must not send requests');
    assert.doesNotMatch(src, /\b(NumberAnimation|SequentialAnimation|PropertyAnimation|Behavior on)\b/);
    assert.match(src, /asynchronous: false/);
    // Bounded residency: no shared pixmap cache, so only the shown frame stays decoded.
    assert.match(src, /cache: false/);
    assert.doesNotMatch(src, /cache: true/);
});

test('shared QML never imports Quickshell', () => {
    for (const f of ['PetSprite.qml', 'DesktopPet.qml', 'PetCare.qml', 'PetCatalog.js', 'PetStateMachine.js',
                     'OverlayFace.js', 'petstate.js'])
        assert.doesNotMatch(read(`../shared/${f}`), /^\s*import\s+Quickshell/m, f);
});

test('DesktopPet: one cosmetic scheduler, single-shot wake and explicit care feedback, no transport', () => {
    const src = read('../shared/DesktopPet.qml').replace(/\/\/[^\n]*/g, '');
    assert.equal(src.match(/Timer \{/g).length, 3);
    assert.match(src, /objectName: "desktopPet\.cosmeticTimer"/);
    const wake = src.slice(src.indexOf('objectName: "desktopPet.wakeTimer"'));
    assert.match(wake.slice(0, 200), /repeat: false/, 'the wake timer is single-shot');
    const feedback = src.slice(src.indexOf('id: careFeedbackTimer'), src.indexOf('id: cosmeticTimer'));
    assert.match(feedback, /interval: 6000/);
    assert.match(feedback, /repeat: false/);
    assert.doesNotMatch(feedback, /running:/, 'feedback never polls while idle');
    assert.match(feedback, /onTriggered: root.careMessage = ""/);
    assert.equal(src.match(/careFeedbackTimer\.restart\(\)/g).length, 1);
    assert.match(src, /onCareMessageChanged: if \(careMessage !== ""\) careFeedbackTimer\.restart\(\)/);
    for (const banned of ['.request(', 'host', 'Process', 'execDetached', 'state.get', 'pet.care', 'pet.events_seen'])
        assert.ok(!src.includes(banned), `DesktopPet must not contain ${banned}`);
    // Operator placement correction permits the small Pinned label (§14).
    assert.doesNotMatch(src, /\bLabel \{|\bButton \{/);
    assert.ok(!src.includes('Item {\n        id: prop'), 'the separate prop item is gone');
});

// ---- priority table ----

const F = (over = {}) => ({ hasLife: true, sleeping: false, taskState: '', critical: false, warning: false,
                            bathroomDue: false, sleepDue: false, walking: false, error: false, ...over });

test('baseState priority table', () => {
    assert.equal(SM.baseState(F({ hasLife: false, error: true, sleeping: true })), 'memorial');
    assert.equal(SM.baseState(F({ error: true, sleeping: true, critical: true })), 'error');
    assert.equal(SM.baseState(F({ sleeping: true, critical: true, taskState: 'running' })), 'sleep');
    assert.equal(SM.baseState(F({ critical: true, taskState: 'running' })), 'needs_care');
    for (const s of ['preparing', 'running', 'verifying', 'cancelling'])
        assert.equal(SM.baseState(F({ taskState: s, warning: true, walking: true })), 'working', s);
    for (const s of ['waiting_for_user', 'review_ready'])
        assert.equal(SM.baseState(F({ taskState: s, warning: true })), 'waiting', s);
    assert.equal(SM.baseState(F({ taskState: 'recovery_required', warning: true })), 'error');
    assert.equal(SM.baseState(F({ warning: true })), 'needs_care');
    assert.equal(SM.baseState(F({ bathroomDue: true })), 'needs_care');
    assert.equal(SM.baseState(F({ sleepDue: true })), 'needs_care');
    assert.equal(SM.baseState(F({ warning: true, walking: true })), 'locomotion', 'warning yields to walking');
    assert.equal(SM.baseState(F({ walking: true })), 'locomotion');
    assert.equal(SM.baseState(F()), 'idle');
    assert.equal(SM.baseState(F({ taskState: 'accepted' })), 'idle', 'terminal task states are not shown');
});

test('task-state mapping', () => {
    assert.equal(SM.taskVisual('running'), 'working');
    assert.equal(SM.taskVisual('preparing'), 'working');
    assert.equal(SM.taskVisual('verifying'), 'working');
    assert.equal(SM.taskVisual('cancelling'), 'working');
    assert.equal(SM.taskVisual('waiting_for_user'), 'waiting');
    assert.equal(SM.taskVisual('review_ready'), 'waiting');
    assert.equal(SM.taskVisual('recovery_required'), 'error');
    for (const s of ['', 'draft', 'accepted', 'discarded', 'failed', 'cancelled', null, undefined])
        assert.equal(SM.taskVisual(s), '', String(s));
});

test('facts from snapshots: no life → memorial, legacy snapshot keeps the v0.1 pet', () => {
    assert.equal(SM.baseState(SM.facts({ life: null, care: { sleeping: true } }, false)), 'memorial');
    assert.equal(SM.baseState(SM.facts({ care: { sleeping: false } }, false)), 'idle', 'no life key = legacy pet');
    assert.equal(SM.facts(null, false).loaded, false);
    const crit = SM.facts({ life: { status: 'warning', fainted: true }, care: {} }, false);
    assert.equal(crit.critical, true, 'fainted counts as critical');
    assert.equal(SM.facts({ life: { status: 'healthy', death_pending: true }, care: {} }, false).critical, true);
    const f = SM.facts({ life: { status: 'warning' }, care: { bathroom_due: true, sleep_due: true, sleeping: false },
                         active_task: { state: 'running' } }, true);
    assert.deepEqual(plain(f), { loaded: true, hasLife: true, sleeping: false, taskState: 'running', critical: false,
                                 warning: true, bathroomDue: true, sleepDue: true, walking: true, error: false });
});

test('roaming only for a calm awake pet', () => {
    assert.equal(SM.canRoam(F()), true);
    assert.equal(SM.canRoam(F({ warning: true })), true);
    assert.equal(SM.canRoam(F({ sleeping: true })), false);
    assert.equal(SM.canRoam(F({ critical: true })), false);
    assert.equal(SM.canRoam(F({ taskState: 'running' })), false);
    assert.equal(SM.canRoam(F({ taskState: 'review_ready' })), false);
    assert.equal(SM.canRoam(F({ hasLife: false })), false);
});

// ---- interrupt rules ----

const meta = (name) => C.stateMeta('cat', name);

test('interrupt rules: care is not interrupted by locomotion; sleep_enter completes', () => {
    assert.equal(SM.display(meta('locomotion'), meta('food')), 'transient');
    assert.equal(SM.display(meta('idle'), meta('water')), 'transient');
    assert.equal(SM.display(meta('needs_care'), meta('play')), 'transient');
    assert.equal(SM.display(meta('error'), meta('wash')), 'base', 'error (100) outranks care (70)');
    for (const base of ['error', 'sleep', 'needs_care', 'idle'])
        assert.equal(SM.display(meta(base), meta('sleep_enter')), 'transient', `sleep_enter completes over ${base}`);
    assert.equal(SM.display(meta('error'), meta('sleep_exit')), 'transient');
    assert.equal(SM.display(meta('sleep'), meta('celebrate')), 'base', 'sleep (85) outranks celebrate (60)');
    assert.equal(SM.display(meta('needs_care'), meta('celebrate')), 'transient');
    assert.equal(SM.display(meta('idle'), null), 'base');
    // A new transient: a higher one replaces, a lower one queues, nothing replaces sleep_enter.
    assert.equal(SM.admit(meta('celebrate'), meta('food')), 'replace');
    assert.equal(SM.admit(meta('food'), meta('celebrate')), 'queue');
    assert.equal(SM.admit(meta('sleep_enter'), meta('error')), 'queue');
    assert.equal(SM.admit(null, meta('food')), 'replace');
    assert.equal(SM.canInterrupt(meta('idle'), meta('idle')), true, 'any');
    assert.equal(SM.canInterrupt(meta('food'), meta('water')), false, 'higher needs strictly higher');
    assert.equal(SM.canInterrupt({ name: 'mystery' }, { name: 'idle' }), true, 'unknown defaults to any');
});

// ---- durations and frames ----

test('durationMs follows Amendment A4', () => {
    const expect = { food: 2400, water: 2400, play: 2400, wash: 2400, bathroom: 2400, sleep_enter: 1200,
                     sleep_exit: 1200, celebrate: 1600 };
    for (const [name, ms] of Object.entries(expect)) assert.equal(SM.durationMs(meta(name)), ms, name);
    for (const name of ['idle', 'locomotion', 'sleep', 'working', 'waiting', 'needs_care', 'error'])
        assert.equal(SM.durationMs(meta(name)), Infinity, name);
    assert.equal(SM.durationMs({ frames: ['a', 'b'], frameMs: 400, loop: false, repeats: 2 }), 1600);
    assert.equal(SM.cycleMs(meta('idle')), 2400);
    for (const name of STATES) {
        const m = meta(name);
        assert.equal(SM.frameMsOf(m) % 200, 0, `${name} frameMs on the 200 ms grid`);
        assert.ok(SM.frameMsOf(m) >= 200 && SM.frameMsOf(m) <= 1000, name);
        if (!m.loop) assert.ok(SM.durationMs(m) <= 4000, `${name} finite total ≤ 4000 ms`);
    }
    assert.equal(SM.frameMsOf({ frameMs: 50 }), 200, 'never faster than 5 updates/s');
    assert.equal(SM.frameMsOf({ frameMs: 'x' }), 200);
});

test('frameAt walks ticks; loops wrap, once holds, reduced motion is static', () => {
    const idle = meta('idle'); // 4 frames × 600 ms = 3 ticks per frame
    assert.deepEqual([0, 2, 3, 5, 6, 9, 12, 13].map((t) => SM.frameAt(idle, t, false)), [0, 0, 1, 1, 2, 3, 0, 0]);
    const food = meta('food'); // 6 frames × 400 ms = 2 ticks per frame, once
    assert.deepEqual([0, 1, 2, 4, 10, 11, 12, 40].map((t) => SM.frameAt(food, t, false)), [0, 0, 1, 2, 5, 5, 5, 5]);
    const loco = meta('locomotion');
    assert.deepEqual([0, 1, 2, 3, 4, 5].map((t) => SM.frameAt(loco, t, false)), [0, 1, 2, 3, 0, 1]);
    // Reduced motion: always the state's reducedFrame (Amendment A4).
    const reduced = { idle: 0, locomotion: 0, food: 1, water: 1, play: 1, wash: 1, bathroom: 1, sleep_enter: 1,
                      sleep: 0, sleep_exit: 1, celebrate: 0, working: 0, waiting: 0, needs_care: 0, error: 0 };
    for (const [name, r] of Object.entries(reduced))
        for (const t of [0, 1, 7, 100]) assert.equal(SM.frameAt(meta(name), t, true), r, `${name}@${t}`);
    assert.equal(SM.frameAt({ frames: ['a'], reducedFrame: 5 }, 0, true), 0, 'out-of-range reducedFrame clamps to 0');
    assert.equal(SM.frameAt(idle, -4, false), 0);
    assert.equal(SM.frameAt({ frames: ['a', 'b'], frameMs: 200, loop: false, repeats: 2 }, 3, false), 1);
    assert.equal(SM.frameAt({ frames: ['a', 'b'], frameMs: 200, loop: false, repeats: 2 }, 9, false), 1);
});

test('snapInterval keeps every wake on the 200 ms grid', () => {
    assert.equal(SM.snapInterval(0), 200);
    assert.equal(SM.snapInterval(1), 200);
    assert.equal(SM.snapInterval(201), 400);
    assert.equal(SM.snapInterval(8000), 8000);
    assert.equal(SM.snapInterval(NaN), 200);
    assert.equal(SM.snapInterval(Infinity), 200);
});

// ---- catalog ----

test('catalog lookups over the fixture registry', () => {
    assert.deepEqual(plain(C.ids()), ['cat', 'goldfish', 'ghost']);
    assert.equal(C.isSpecies('cat'), true);
    assert.equal(C.isSpecies('dragon'), false);
    assert.equal(C.isSpecies('../cat'), false);
    assert.equal(C.isSpecies('constructor'), false);
    assert.equal(C.isSpecies('__proto__'), false);
    assert.equal(C.species('dragon'), null);
    assert.equal(C.normalizeState('walk'), 'locomotion');
    assert.equal(C.normalizeState('dance'), 'idle');
    assert.equal(C.normalizeState(undefined), 'idle');
    assert.equal(C.stateMeta('cat', 'walk').name, 'locomotion');
    assert.equal(C.stateMeta('cat', 'dance').name, 'idle');
    assert.equal(C.careLabel('cat', 'food'), 'Kibble');
    assert.equal(C.careLabel('goldfish', 'water'), 'Water refresh');
    assert.equal(C.careLabel('dragon', 'food'), 'Offer food', 'generic fallback');
    assert.equal(C.careLabel('cat', 'juggle'), 'Care');
    assert.match(C.careHint('goldfish', 'water'), /refreshes the tank water/);
    assert.equal(C.careHint('cat', 'water'), '');
    assert.equal(C.stageName('cat', 1), 'Pounce Kit');
    assert.equal(C.stageName('cat', 3), 'Maned Inspector');
    assert.equal(C.stageName('cat', 9), 'Maned Inspector', 'stage clamps');
    assert.equal(C.stageName('dragon', 2), 'Grown');
    assert.equal(C.displayName('goldfish'), 'Goldfish');
});

test('frame URLs use explicit per-state lists, WebP, and only the selected pack and stage', () => {
    assert.equal(C.frameRelUrl('cat', 2, 'idle', 2), '../assets/pets/cat/s2/b01.webp');
    assert.equal(C.frameRelUrl('cat', 1, 'food', 5), '../assets/pets/cat/s1/b04.webp', 'confirmed-result frame is last');
    assert.equal(C.frameRelUrl('cat', 1, 'food', 0), '../assets/pets/cat/s1/b03.webp', 'pending hold is the approach');
    assert.equal(C.frameRelUrl('cat', 3, 'walk', 6), '../assets/pets/cat/s3/b10.webp', 'walk alias, frame wraps');
    assert.equal(C.frameRelUrl('dragon', 1, 'idle', 0), '../assets/pets/_placeholder.webp');
    assert.equal(C.frameRelUrl('cat', 1, 'memorial', 0), '../assets/pets/_memorial.webp');
    assert.equal(C.thumbRelUrl('goldfish', 2), '../assets/pets/goldfish/thumb-s2.webp');
    assert.equal(C.thumbRelUrl('nope', 2), '../assets/pets/_placeholder.webp');
    for (const id of ['cat', 'goldfish'])
        for (const stage of [1, 2, 3])
            for (const s of STATES)
                for (let f = 0; f < C.stateMeta(id, s).frames.length; f++) {
                    const url = C.frameRelUrl(id, stage, s, f);
                    assert.ok(url.startsWith(`../assets/pets/${id}/s${stage}/`), url);
                    const file = new URL(url.replace('../assets/pets/', './fixtures/pets/'), import.meta.url);
                    assert.ok(existsSync(file), `fixture frame exists: ${url}`);
                }
});

test('frame entry forms: stem, file name, and stage-token paths', () => {
    const reg = plain(FIXTURE);
    reg.species.cat.states.idle.frames = ['b00', 'b01.webp', 'alt/{stage}/x.webp'];
    C.install(reg, 'file:///packs');
    try {
        assert.equal(C.frameRelUrl('cat', 2, 'idle', 0), 'file:///packs/cat/s2/b00.webp');
        assert.equal(C.frameRelUrl('cat', 2, 'idle', 1), 'file:///packs/cat/s2/b01.webp');
        assert.equal(C.frameRelUrl('cat', 2, 'idle', 2), 'file:///packs/cat/alt/2/x.webp');
        assert.equal(C.placeholderUrl(), 'file:///packs/_placeholder.webp');
        // A state missing from a manifest borrows idle's pictures but keeps its own rules.
        delete reg.species.cat.states.working;
        const w = C.stateMeta('cat', 'working');
        assert.deepEqual(plain(w.frames), ['b00', 'b01.webp', 'alt/{stage}/x.webp']);
        assert.equal(w.priority, 40);
        assert.equal(w.frameMs, 800);
        // A legacy numeric frame count is ignored in favour of the Amendment A4 list.
        reg.species.cat.states.sleep = { frames: 2, frameMs: 1000, loop: true };
        assert.deepEqual(plain(C.stateMeta('cat', 'sleep').frames), ['b07']);
    } finally {
        C.install();
    }
    assert.equal(C.placeholderUrl(), '../assets/pets/_placeholder.webp', 'install() restores the default');
    assert.deepEqual(plain(C.ids()), ['cat', 'goldfish', 'ghost']);
    C.install({ nonsense: true });
    try {
        assert.deepEqual(plain(C.ids()), ['cat', 'goldfish', 'ghost'], 'an invalid registry falls back to the generated one');
    } finally {
        C.install();
    }
});

test('resolve: placeholder, memorial, reduced motion and mirror rules', () => {
    assert.deepEqual(plain(C.resolve('dragon', 1, 'idle', 0, false)),
        { url: '../assets/pets/_placeholder.webp', mirror: false, placeholder: true, state: 'idle', frame: 0 });
    assert.deepEqual(plain(C.resolve('cat', 2, 'memorial', 3, false)),
        { url: '../assets/pets/_memorial.webp', mirror: false, placeholder: false, state: 'memorial', frame: 0 });
    const r = C.resolve('cat', 2, 'water', 4, true);
    assert.equal(r.url, '../assets/pets/cat/s2/c02.webp', 'reduced motion shows reducedFrame 1');
    assert.equal(r.frame, 1);
    assert.equal(C.resolve('cat', 1, 'locomotion', 1, false).mirror, true, 'floor species mirror');
    assert.equal(C.resolve('goldfish', 1, 'locomotion', 1, false).mirror, false, 'aquatic never mirrors');
    for (const s of STATES) assert.equal(C.stateMeta('goldfish', s).mirror, false, s);
    assert.equal(C.resolve('cat', 1, 'idle', -1, false).frame, 3, 'negative frame wraps');
    assert.equal(C.resolve('cat', 0, 'idle', 0, false).url, '../assets/pets/cat/s1/b00.webp', 'stage clamps to 1');
});

test('geometry, roaming, gait and hit rectangles', () => {
    assert.equal(C.roams('cat'), true);
    assert.equal(C.roams('goldfish'), false);
    assert.equal(C.roams('dragon'), false);
    assert.equal(C.isAquatic('goldfish'), true);
    assert.deepEqual(plain(C.locomotion('goldfish')), { style: 'swim_in_habitat', pxPerTick: 0, pauseTicks: [30, 75], airborneFrames: [] });
    assert.deepEqual(plain(C.locomotion('cat')), { style: 'walk', pxPerTick: 4, pauseTicks: [40, 110], airborneFrames: [] });
    assert.deepEqual(plain(C.locomotion('dragon')).pauseTicks, [40, 110]);
    // cat hitBox [30, 60, 112, 124] on a 192 canvas, drawn at 96 px → ×0.5.
    assert.deepEqual(plain(C.hitRect('cat', 96, false)), { x: 15, y: 30, width: 56, height: 62 });
    // Mirrored about the canvas centre: x' = 192 − (30 + 112) = 50 → 25.
    assert.deepEqual(plain(C.hitRect('cat', 96, true)), { x: 25, y: 30, width: 56, height: 62 });
    // A symmetric box stays put; a box is clipped to the drawn square.
    const reg = plain(FIXTURE);
    reg.species.cat.geometry.hitBox = [40, 60, 112, 124];
    C.install(reg);
    try {
        assert.deepEqual(plain(C.hitRect('cat', 192, true)), plain(C.hitRect('cat', 192, false)));
        reg.species.cat.geometry.hitBox = [100, 100, 150, 150];
        assert.deepEqual(plain(C.hitRect('cat', 192, false)), { x: 100, y: 100, width: 92, height: 92 });
        reg.species.cat.geometry.hitBox = [0, 0, -1, 5];
        assert.deepEqual(plain(C.hitRect('cat', 192, false)), { x: 0, y: 0, width: 192, height: 192 }, 'invalid box → canvas');
    } finally {
        C.install();
    }
    // Aquatic: the habitat (water.outer) is the clickable footprint and never mirrors.
    const g = plain(FIXTURE.species.goldfish.geometry.water.outer);
    assert.deepEqual(plain(C.hitRect('goldfish', 192, false)), { x: g[0], y: g[1], width: g[2], height: g[3] });
    assert.deepEqual(plain(C.hitRect('goldfish', 192, true)), plain(C.hitRect('goldfish', 192, false)));
    assert.deepEqual(plain(C.hitRect('dragon', 112, false)), { x: 0, y: 0, width: 112, height: 112 });
    assert.deepEqual(plain(C.hitRect('', 0, false)), { x: 0, y: 0, width: 0, height: 0 });
});

// ---- menu status copy ----

const life = (over = {}) => ({ life_id: 'life-1', name: 'Ginger', appearance: 'cat', stage: 2, status: 'healthy',
                               mode: 'permadeath', armed: true, paused: false, fainted: false, death_pending: false, ...over });
const need = (n, state, extra = {}) => ({ need: n, lethal: n === 'hunger' || n === 'thirst', state, ...extra });

test('statusLine copy follows the calm templates and never predicts a death', () => {
    assert.equal(SM.statusLine(null), '');
    assert.equal(SM.statusLine({ life: null }), "No companion right now. Adopt one whenever you're ready.");
    assert.equal(SM.statusLine({ life: life() }), '', 'healthy defers to readiness text');
    assert.equal(SM.statusLine({ care: {} }), '', 'legacy snapshot defers to readiness text');
    assert.equal(SM.statusLine({ life: life({ status: 'warning' }),
                                 lifecycle: { needs: [need('hunger', 'ok'), need('thirst', 'low', { until_critical_ms: 9600000 })] } }),
        'Ginger is getting thirsty. Water will help. About 2 h 40 min of active time before it becomes serious.');
    assert.equal(SM.statusLine({ life: life({ status: 'warning' }), lifecycle: { needs: [need('hygiene', 'low')] } }),
        'Ginger is getting a bit grubby. A wash will help.');
    assert.equal(SM.statusLine({ life: life({ status: 'critical' }),
                                 lifecycle: { critical: { need: 'thirst', remaining_ms: 18600000 } } }),
        'Ginger urgently needs water. Care now keeps them safe. Without water, Ginger may pass away after about 5 h 10 min of active time.');
    assert.equal(SM.statusLine({ life: life({ status: 'critical', mode: 'gentle' }),
                                 lifecycle: { critical: { need: 'hunger', remaining_ms: 600000 } } }),
        'Ginger urgently needs food. Care now keeps them safe.');
    assert.equal(SM.statusLine({ life: life({ status: 'critical', fainted: true, mode: 'gentle' }),
                                 lifecycle: { critical: { need: 'thirst' } } }),
        'Ginger fainted from thirst. Water will revive them; work waits until then.');
    assert.equal(SM.statusLine({ life: life({ status: 'critical', death_pending: true }) }),
        "Ginger's grace time has run out. They will pass away when the current task is finished; your work is kept.",
        'a pending end never promises that care can still help');
    assert.equal(SM.statusLine({ life: life({ paused: true }) }), "Paused: time isn't counting (on a break).");
    assert.equal(SM.statusLine({ life: life({ armed: false }) }), "Paused: time isn't counting (not set up).");
    assert.equal(SM.formatActive(60000), 'about 1 min of active time');
    assert.equal(SM.formatActive(3600000), 'about 1 h of active time');
    assert.equal(SM.formatActive(7800000), 'about 2 h 10 min of active time');
    // Rounded down, never up: an estimate never claims more time than the rules give.
    assert.equal(SM.formatActive(30000), 'less than a minute of active time');
    assert.equal(SM.formatActive(90000), 'about 1 min of active time');
    assert.equal(SM.formatActive(3570000), 'about 59 min of active time');
    assert.equal(SM.statusLine({ life: life({ status: 'warning' }),
                                 lifecycle: { needs: [need('thirst', 'low', { until_critical_ms: 30000 })] } }),
        'Ginger is getting thirsty. Water will help. Less than a minute of active time before it becomes serious.');
    const lines = [];
    for (const status of ['healthy', 'warning', 'critical'])
        for (const mode of ['permadeath', 'gentle'])
            for (const flags of [{}, { fainted: true }, { death_pending: true }, { paused: true }, { armed: false }])
                for (const n of ['hunger', 'thirst', 'mental_health', 'hygiene'])
                    lines.push(SM.statusLine({ life: life({ status, mode, ...flags }),
                        lifecycle: { needs: [need(n, 'low', { until_critical_ms: 600000 })], critical: { need: n, remaining_ms: 1 } } }));
    for (const l of lines) assert.doesNotMatch(l, BANNED, l);
});

test('care retry keys are reused only briefly after an unconfirmed press', () => {
    const sig = 'pet.care{"action":"water"}';
    const t0 = 5_000_000;
    const until = t0 + SM.CARE_RETRY_WINDOW_MS;
    assert.equal(SM.reuseRetryKey('care-a-1', sig, until, sig, t0 + 1000), true, 'immediate retry of the same press');
    assert.equal(SM.reuseRetryKey('care-a-1', sig, until, sig, until), true);
    assert.equal(SM.reuseRetryKey('care-a-1', sig, until, sig, until + 1), false, 'after the window it is a new press');
    assert.equal(SM.reuseRetryKey('care-a-1', sig, until, sig, t0 + 6 * 3_600_000), false, 'hours later');
    assert.equal(SM.reuseRetryKey('care-a-1', sig, until, 'pet.care{"action":"food"}', t0), false, 'another action');
    assert.equal(SM.reuseRetryKey('', sig, until, sig, t0), false, 'no previous key');
    assert.equal(SM.reuseRetryKey('care-a-1', sig, 0, sig, t0), false, 'confirmed (expiry cleared)');
    // The overlay and the bar plugin use the same window.
    const barSrc = readFileSync(new URL('../../omarchy-plugin/CodePetBar.js', import.meta.url), 'utf8');
    assert.match(barSrc, new RegExp('var RETRY_WINDOW_MS = ' + SM.CARE_RETRY_WINDOW_MS + '\\b'));
});

test('PetCare timers never launch a request (spec §1), directly or through a function', () => {
    const src = read('../shared/PetCare.qml').replace(/\/\/[^\n]*/g, '');
    const blockAfter = (text, start) => {
        const i = text.indexOf('{', start);
        let depth = 0;
        for (let j = i; j < text.length; j++) {
            if (text[j] === '{') depth++;
            else if (text[j] === '}' && --depth === 0) return text.slice(i, j + 1);
        }
        throw new Error('unbalanced');
    };
    const fns = {};
    for (const m of src.matchAll(/\bfunction\s+(\w+)\s*\(/g)) fns[m[1]] = blockAfter(src, m.index + m[0].length);
    const timers = [...src.matchAll(/\bTimer\s*\{/g)].map((m) => blockAfter(src, m.index));
    assert.ok(timers.length >= 4);
    const bad = /request\(|perform\(|startRoutine\(|careClicked\(/;
    for (const t of timers) {
        assert.doesNotMatch(t, bad, t);
        for (const [, name] of t.matchAll(/root\.(\w+)\(/g)) assert.doesNotMatch(fns[name] || '', bad, `${name} from a timer`);
    }
    // The routine is one request: storage's atomic pet.care {action: "routine"}.
    assert.match(fns.startRoutine, /perform\("pet\.care", \{action: "routine"\}, "routine"/);
    assert.equal((fns.startRoutine.match(/perform\(/g) || []).length, 1);
    assert.match(src, /onTriggered: root\.playRoutineStep\(\)/);
});

test('hoppers move only on the airborne frames the shipped registry lists', () => {
    const REAL = JSON.parse(read('../assets/pets/registry.json'));
    const R = loadLibrary('../shared/PetCatalog.js', { PetRegistry: { REGISTRY: REAL } });
    for (const id of ['frog', 'rabbit', 'owl']) {
        const listed = REAL.species[id].states.locomotion.airborneFrames;
        assert.ok(listed.length > 0 && listed.length < 4, `${id} is a hopper`);
        assert.deepEqual([...R.locomotion(id).airborneFrames], listed, `${id} airborne frames come from states.locomotion`);
    }
    assert.deepEqual([...R.locomotion('cat').airborneFrames], [], 'walkers move on every frame');
    assert.deepEqual([...R.locomotion('goldfish').airborneFrames], []);
    // Fallbacks: a top-level airborneFrames, then the roster's pxFrames.
    const reg = (loco, stateAirborne) => ({ schemaVersion: 1, order: ['x'], species: { x: {
        habitat: 'floor', locomotion: loco, states: { locomotion: { airborneFrames: stateAirborne } } } } });
    const F1 = loadLibrary('../shared/PetCatalog.js', { PetRegistry: { REGISTRY: reg({ airborneFrames: [3], pxFrames: [1] }, []) } });
    assert.deepEqual([...F1.locomotion('x').airborneFrames], [3]);
    const F2 = loadLibrary('../shared/PetCatalog.js', { PetRegistry: { REGISTRY: reg({ pxFrames: [1] }, undefined) } });
    assert.deepEqual([...F2.locomotion('x').airborneFrames], [1]);
});

test('hitRect follows the stage: young pets do not capture clicks around them', () => {
    const REAL = JSON.parse(read('../assets/pets/registry.json'));
    const R = loadLibrary('../shared/PetCatalog.js', { PetRegistry: { REGISTRY: REAL } });
    const scaled = (b, size) => ({ x: b[0] * size / 192, y: b[1] * size / 192, width: b[2] * size / 192, height: b[3] * size / 192 });
    const near = (a, b) => ['x', 'y', 'width', 'height'].every((k) => Math.abs(a[k] - b[k]) < 1e-9);
    for (const id of REAL.order) {
        const g = REAL.species[id].geometry;
        for (const stage of [1, 2, 3]) {
            const box = g.stageHitBoxes[String(stage)];
            assert.ok(near(plain(R.hitRect(id, 192, false, stage)), scaled(box, 192)), `${id} s${stage}`);
            assert.ok(near(plain(R.hitRect(id, 112, false, stage)), scaled(box, 112)), `${id} s${stage} at 112`);
        }
        // The young box is inside the all-stage union and smaller than it.
        const s1 = g.stageHitBoxes['1'];
        assert.ok(s1[2] * s1[3] < g.hitBox[2] * g.hitBox[3], `${id}: young box is smaller`);
    }
    // A stage-1 cat: the mask is the young box, not the union [9,12,177,172].
    assert.ok(near(plain(R.hitRect('cat', 192, false, 1)), scaled([26, 45, 146, 139], 192)));
    // Mirroring still mirrors the stage box about the canvas centre.
    const m = plain(R.hitRect('cat', 192, true, 1));
    assert.equal(m.x, 192 - (26 + 146));
    // Without a stage (older callers) the union box is used; out-of-range stages clamp.
    assert.ok(near(plain(R.hitRect('cat', 192, false)), scaled(REAL.species.cat.geometry.hitBox, 192)));
    assert.ok(near(plain(R.hitRect('cat', 192, false, 9)), scaled(REAL.species.cat.geometry.stageHitBoxes['3'], 192)));
});

// Review finding: the desktop pet and the life panel must show the same countdown for the
// same remaining_ms (both rounded down).
test('statusLine and LifeText.criticalText render the same permadeath countdown', () => {
    const LT = loadLibrary('../shared/LifeText.js');
    for (const remaining of [0, 30000, 59999, 60000, 90000, 3570000, 3600000, 18630000]) {
        const pet = SM.statusLine({ life: { name: 'Ginger', status: 'critical', mode: 'permadeath' },
                                    lifecycle: { critical: { need: 'thirst', remaining_ms: remaining } } });
        const panel = LT.criticalText('Ginger', 'thirst', remaining, 'permadeath');
        const span = (text) => text.match(/after (.*) of active time\.$/)[1];
        assert.equal(span(pet), span(panel), `remaining_ms ${remaining}`);
        assert.equal(SM.formatActive(remaining), LT.formatActive(remaining), `formatActive ${remaining}`);
    }
});
