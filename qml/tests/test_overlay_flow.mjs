// Node tests for the desktop pet's pure rules (SPEC-v0.3 §3.8, §7.3, §8):
// qml/omarchy/OverlayFlow.js (when the process exits, "Pet on my desktop", the Home call)
// and qml/shared/OverlayFace.js (animations, picture bubbles, celebrate-once).
// Run: node --test qml/tests/test_overlay_flow.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import vm from 'node:vm';

const read = (rel) => readFileSync(new URL(rel, import.meta.url), 'utf8');
const plain = (x) => JSON.parse(JSON.stringify(x));

function load(rel) {
    const src = read(rel);
    assert.match(src, /^\.pragma library\n/, `${rel} must stay a .pragma library`);
    assert.doesNotMatch(src, /^\s*(import|\.import)\b/m, `${rel} must import nothing`);
    const ctx = {};
    vm.createContext(ctx);
    vm.runInContext(src.replace(/^\.pragma library$/m, ''), ctx, { filename: rel });
    return ctx;
}

const F = load('../omarchy/OverlayFlow.js');
const O = load('../shared/OverlayFace.js');
const P = load('../shared/petstate.js');
const V = JSON.parse(read('../../docs/v0.3/contracts/agent_vectors.json'));
const ICONS = JSON.parse(read('../../docs/v0.3/contracts/icons.json'));
const fixture = (n) => JSON.parse(read(`../../docs/v0.3/contracts/fixtures/bar-v2/${n}.json`));

const shellJson = (entry, where = 'layout') => {
    const right = entry === undefined ? [{ id: 'omarchy.clock' }] : [{ id: 'omarchy.clock' }, entry];
    const bar = where === 'layout' ? { layout: { left: [], center: [], right } } : { right };
    return JSON.stringify({ version: 1, bar });
};

test('settings path is the file the Omarchy shell reads: $HOME/.config, whatever XDG_CONFIG_HOME says', () => {
    assert.equal(F.settingsPath('/home/u'), '/home/u/.config/omarchy/shell.json');
    assert.equal(F.settingsPath('/home/u/'), '/home/u/.config/omarchy/shell.json');
    assert.equal(F.settingsPath('relative'), '');
    assert.equal(F.settingsPath('/a/../b'), '');
    assert.equal(F.settingsPath(''), '');
    assert.equal(F.settingsPath(undefined), '');
    assert.equal(F.settingsPath.length, 1, 'no XDG_CONFIG_HOME parameter');
    const shell = read('../shell.qml');
    assert.match(shell, /OverlayFlow\.settingsPath\(Quickshell\.env\("HOME"\)\)/);
    assert.doesNotMatch(shell, /XDG_CONFIG_HOME/, 'the overlay never follows XDG_CONFIG_HOME for shell.json');
    // Omarchy's own path, when the packaged shell is there to compare with.
    const omarchy = '/usr/share/omarchy/shell/shell.qml';
    if (existsSync(omarchy))
        assert.match(readFileSync(omarchy, 'utf8'), /userConfigPath: home \+ "\/\.config\/omarchy\/shell\.json"/);
});

test('"Pet on my desktop" is the bar entry\'s overlayOnLoad, read like the bar plugin', () => {
    const id = 'greyforge.codepet';
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: true })), true);
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: 'true' })), true);
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: false })), false);
    assert.equal(F.desktopPetSetting(shellJson({ id })), false, 'missing key = manifest default (off)');
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: 1 })), false);
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: true }, 'flat')), true, 'older flat bar sections');
    // The CodePet cell was removed from the bar (no uninstall): off, the pet exits.
    assert.equal(F.desktopPetSetting(shellJson(undefined)), false);
    assert.equal(F.desktopPetSetting(JSON.stringify({ version: 1, bar: { layout: { right: [] } } })), false);
    assert.equal(F.desktopPetSetting(JSON.stringify({ version: 1 })), false, 'Omarchy\'s built-in bar has no CodePet');
    assert.equal(F.desktopPetSetting(JSON.stringify({ version: 1, bar: 3 })), false);
    // Unknown: no usable file (none, junk, not version 1, which Omarchy ignores too).
    // A standalone overlay keeps running.
    assert.equal(F.desktopPetSetting(''), null);
    assert.equal(F.desktopPetSetting('{nope'), null);
    assert.equal(F.desktopPetSetting('[]'), null);
    assert.equal(F.desktopPetSetting(JSON.stringify({ bar: { layout: { right: [] } } })), null);
    assert.equal(F.desktopPetSetting(JSON.stringify({ version: 2, bar: { layout: { right: [{ id, overlayOnLoad: true }] } } })), null);
    assert.equal(F.desktopPetSetting(undefined), null);
    const big = shellJson({ id, overlayOnLoad: true }).replace('"version":1', `"pad":"${'x'.repeat(1048576)}","version":1`);
    assert.equal(F.desktopPetSetting(big), null, 'over 1 MiB');
});

test('Calm mode: the bar entry\'s calm, live; a setReducedMotion call until it changes; else the launch env', () => {
    const id = 'greyforge.codepet';
    assert.equal(F.calmSetting(shellJson({ id, calm: true })), true);
    assert.equal(F.calmSetting(shellJson({ id, calm: 'true' })), true);
    assert.equal(F.calmSetting(shellJson({ id, calm: false })), false);
    assert.equal(F.calmSetting(shellJson({ id })), false, 'missing key = the plugin default (off; Omarchy has no system signal)');
    assert.equal(F.calmSetting(shellJson(undefined)), null);
    assert.equal(F.calmSetting('{nope'), null);
    assert.equal(F.reducedMotion(null, true, ''), true);
    assert.equal(F.reducedMotion(null, false, '1'), false, 'the live setting beats the launch env');
    assert.equal(F.reducedMotion(null, null, '1'), true, 'no setting: CODEPET_REDUCED_MOTION=1');
    assert.equal(F.reducedMotion(null, null, ''), false);
    assert.equal(F.reducedMotion(null, null, 'yes'), false);
    assert.equal(F.reducedMotion(true, false, ''), true, 'an explicit call wins');
    assert.equal(F.reducedMotion(false, true, '1'), false);
});

test('the process exits on Off or when the desktop pet is switched off, and only then', () => {
    assert.equal(F.exitReason(fixture('off'), null), 'off');
    assert.equal(F.exitReason(fixture('off-no-pet'), true), 'off');
    assert.equal(F.exitReason(fixture('idle-happy'), false), 'desktop_pet_off');
    assert.equal(F.exitReason(null, false), 'desktop_pet_off');
    assert.equal(F.exitReason(fixture('idle-happy'), true), '');
    assert.equal(F.exitReason(fixture('idle-happy'), null), '');
    assert.equal(F.exitReason(null, null), '', 'no file yet: wait for one');
    const v1 = fixture('idle-happy');
    delete v1.enabled;
    assert.equal(F.exitReason(v1, null), '', 'a v1 export (no enabled) is on');
    // Off ends it at once; a switch read as off ends it at once only after it was on here.
    assert.equal(F.exitsAtOnce('off', false), true);
    assert.equal(F.exitsAtOnce('desktop_pet_off', true), true);
    assert.equal(F.exitsAtOnce('desktop_pet_off', false), false, 'a launch can race the write that turns it on');
    assert.equal(F.exitsAtOnce('', true), false);
    assert.ok(F.SETTLE_MS >= 1000 && F.SETTLE_MS <= 3000, 'well inside the launcher\'s 3 s wait');
});

test('HIDE toast is a plain notify-send argv naming the show/hide key', () => {
    const argv = plain(F.hiddenToastArgv());
    assert.equal(argv[0], 'notify-send');
    assert.ok(argv.includes('--app-name=CodePet'));
    assert.equal(argv[argv.length - 2], 'Your pet is hiding');
    assert.equal(argv[argv.length - 1], 'Press ' + F.TOGGLE_KEY_HINT + ' to bring it back.');
    assert.equal(F.TOGGLE_KEY_HINT, 'Super + Alt + P');
    for (const word of argv) assert.ok(!/[;&|`$]/.test(word), 'no shell syntax: ' + word);
});

test('Home opens through the Omarchy shell, silently', () => {
    assert.deepEqual(plain(F.openHomeArgv('/usr/share/omarchy')),
                     ['/usr/share/omarchy/bin/omarchy-shell', '-q', 'shell', 'summon', 'greyforge.codepet']);
    // omarchy-shell refuses to run without OMARCHY_PATH: pass the packaged one.
    const fallback = ['/usr/bin/env', 'OMARCHY_PATH=/usr/share/omarchy', 'omarchy-shell', '-q', 'shell', 'summon', 'greyforge.codepet'];
    assert.deepEqual(plain(F.openHomeArgv('')), fallback);
    assert.deepEqual(plain(F.openHomeArgv(undefined)), fallback);
    assert.deepEqual(plain(F.openHomeArgv('relative/x')), fallback);
    // A burst of clicks sends one call a second.
    assert.equal(F.homeDue(0, 5000), true, 'never sent');
    assert.equal(F.homeDue(5000, 5000), false);
    assert.equal(F.homeDue(5000, 5999), false);
    assert.equal(F.homeDue(5000, 6000), true);
    assert.equal(F.homeDue(5000, 4000), true, 'a clock that went back allows one');
    let sent = 0, last = 0;
    for (let t = 10000; t < 10900; t += 30) if (F.homeDue(last, t)) { sent += 1; last = t; }
    assert.equal(sent, 1, 'thirty clicks in under a second: one Omarchy call');
    assert.equal(F.PLUGIN_ID, JSON.parse(read('../../omarchy-plugin/manifest.json')).id);
    assert.equal(F.opensHome, undefined, 'the v0.2 window verbs and the stub panel verb are gone');
});

test('Updates opens the existing bar page through literal Omarchy IPC argv', () => {
    assert.deepEqual(plain(F.openUpdatesArgv('/usr/share/omarchy')),
                     ['/usr/share/omarchy/bin/omarchy-shell', '-q', 'greyforge.codepet', 'openUpdates']);
    const fallback = ['/usr/bin/env', 'OMARCHY_PATH=/usr/share/omarchy', 'omarchy-shell', '-q', 'greyforge.codepet', 'openUpdates'];
    for (const path of ['', undefined, 'relative/x']) assert.deepEqual(plain(F.openUpdatesArgv(path)), fallback);
    assert.deepEqual(plain(F.openUpdatesArgv('/fixture/a path;$literal')),
                     ['/fixture/a path;$literal/bin/omarchy-shell', '-q', 'greyforge.codepet', 'openUpdates']);
});

test('every overlay value of the shared order has an animation', () => {
    const values = new Set(V.priority.vectors.map((v) => v.expect.overlay));
    for (const ov of values) assert.ok(Object.prototype.hasOwnProperty.call(O.ANIMATIONS, ov), ov);
    assert.equal(O.animation('none').state, '');
    assert.equal(O.animation('sleep').state, 'sleep');
    assert.equal(O.animation('waiting').bob, true);
    assert.equal(O.animation('working').roam, true);
    assert.equal(O.animation('needs_care').roam, false);
    assert.equal(O.animation('needs_care_frame').roam, true);
    assert.equal(O.animation('wander').roam, true);
    assert.equal(O.animation('celebrate').state, 'idle', 'the celebration itself is a one-shot transient');
    assert.equal(O.animation('nonsense').state, 'idle');
    assert.equal(O.animation('constructor').state, 'idle', 'no prototype keys');
    const states = new Set(V.priority.vectors.map((v) => O.animation(v.expect.overlay).state));
    const petStates = ['', 'idle', 'sleep', 'waiting', 'needs_care', 'working', 'error'];
    for (const s of states) assert.ok(petStates.includes(s), s);
});

test('care icon sets follow careIconSet, then habitat', () => {
    assert.equal(O.careIconSet({ careIconSet: 'robot', habitat: 'floor' }), 'robot');
    assert.equal(O.careIconSet({ habitat: 'aquatic' }), 'aquatic');
    assert.equal(O.careIconSet({ habitat: 'floor' }), 'floor');
    assert.equal(O.careIconSet({ careIconSet: 'lava', habitat: 'aquatic' }), 'aquatic');
    assert.equal(O.careIconSet(null), 'floor');
    const reg = JSON.parse(read('../assets/pets/registry.json'));
    for (const [id, want] of Object.entries(ICONS.care_icon_set.resolved))
        assert.equal(O.careIconSet(reg.species[id]), want, id);
});

test('bubbles name shipped icons only, in the face\'s order', () => {
    const icon = (rel) => new URL(`../assets/icons/${rel}`, import.meta.url);
    for (const vec of V.priority.vectors) {
        const doc = fixture(vec.fixture);
        const reg = JSON.parse(read('../assets/pets/registry.json'));
        const set = O.careIconSet(doc.life ? reg.species[doc.life.appearance] : null);
        const list = O.bubbleIcons(vec.expect.bubble, set, vec.expect.state === 'waiting');
        for (const rel of list) assert.ok(existsSync(icon(rel)), `${vec.name}: ${rel}`);
        assert.ok(list.length <= 4);
        if (vec.expect.bubble === null) assert.equal(list.length, 0, vec.name);
        else assert.ok(list.length > 0, vec.name);
    }
    assert.deepEqual(plain(O.bubbleIcons({ apps: ['codex'] }, 'floor', true)),
                     ['status/bubble_wait.webp', 'apps/app_codex.webp']);
    assert.deepEqual(plain(O.bubbleIcons({ apps: ['claude-code', 'nope', 'grok-cli', 'hermes', 'opencode'] }, 'floor', false)),
                     ['apps/app_claude_code.webp', 'apps/app_grok.webp']);
    assert.deepEqual(plain(O.bubbleIcons({ need: 'water' }, 'aquatic', false)), ['care-aquatic/water.webp']);
    assert.deepEqual(plain(O.bubbleIcons({ need: 'bathroom' }, 'floor', false)), []);
    assert.deepEqual(plain(O.bubbleIcons({ need: 'food' }, 'lava', false)), ['care-floor/food.webp']);
    assert.deepEqual(plain(O.bubbleIcons({ star: true, confetti: true }, 'floor', false)),
                     ['care-floor/star.webp', 'status/confetti.webp']);
    assert.deepEqual(plain(O.bubbleIcons({ bandage: true }, 'floor', false)), ['status/bandage.webp']);
    assert.deepEqual(plain(O.bubbleIcons({ helpers: true }, 'floor', false)), ['status/robot_helpers.webp']);
    assert.deepEqual(plain(O.bubbleIcons(null, 'floor', false)), []);
    for (const id of V.clients.map((c) => c.id)) assert.ok(existsSync(icon(O.appTile(id))), id);
    assert.ok(existsSync(icon(O.ICONS.heart)));
});

test('a celebration plays once: its key, seen and growth', () => {
    const now = 1792123900000;
    const s0 = { finished_seq: 0, hello_seq: 0, test_at_ms: 0 };
    const fin = P.sharedPriority(fixture('finished-treat'), now, s0);
    assert.notEqual(O.celebrationKey(fin.celebrate), '');
    assert.equal(O.celebrationKey(null), '');
    const s1 = O.markSeen(s0, fin.celebrate);
    assert.equal(s1.finished_seq, fin.celebrate.seq);
    assert.equal(P.sharedPriority(fixture('finished-treat'), now, s1).state, 'idle');
    const hello = P.sharedPriority(fixture('hello'), now, s0);
    assert.equal(O.markSeen(s1, hello.celebrate).hello_seq, hello.celebrate.seq);
    assert.equal(O.markSeen(s1, hello.celebrate).finished_seq, s1.finished_seq, 'other seqs kept');
    const ready = P.sharedPriority(fixture('ready-check-done'), now, s0);
    assert.equal(O.markSeen(s0, ready.celebrate).test_at_ms, ready.celebrate.at_ms);
    assert.deepEqual(plain(O.markSeen({ finished_seq: 9 }, { kind: 'finished', seq: 3 })),
                     { finished_seq: 9, hello_seq: 0, test_at_ms: 0 }, 'never moves backwards');
    assert.deepEqual(plain(O.markSeen('junk', null)), s0);
    // Growth: Home acknowledges it in Rust; the overlay drops it from its own face once played.
    const grewDoc = fixture('grew-up');
    const grew = P.sharedPriority(grewDoc, now, s0);
    assert.equal(grew.celebrate.kind, 'grew');
    const after = O.withoutGrowth(grewDoc, grew.celebrate.seq);
    assert.notEqual(after, grewDoc);
    assert.equal(grewDoc.unseen_events.length > 0, true, 'the input is never mutated');
    assert.notEqual(P.sharedPriority(after, now, s0).state, 'celebrate');
    assert.equal(O.withoutGrowth(grewDoc, grew.celebrate.seq - 1), grewDoc, 'newer growth still plays');
    assert.equal(O.withoutGrowth(grewDoc, -1), grewDoc);
    assert.equal(O.withoutGrowth(null, 5), null);
});

test('a new overlay process counts what its first export holds as celebrated', () => {
    const now = 1792123900000;
    for (const name of ['finished-treat', 'hello', 'ready-check-done', 'grew-up']) {
        const doc = fixture(name);
        const start = O.seenAtStart(doc);
        const face = P.sharedPriority(O.withoutGrowth(doc, start.grewSeq), now, start.seen);
        assert.notEqual(face.state, 'celebrate', `${name}: no replay after a restart`);
        assert.equal(P.sharedPriority(doc, now, { finished_seq: 0, hello_seq: 0, test_at_ms: 0 }).state, 'celebrate',
                     `${name}: the fixture does celebrate for a process that saw it arrive`);
    }
    assert.deepEqual(plain(O.seenAtStart(null)), { seen: { finished_seq: 0, hello_seq: 0, test_at_ms: 0 }, grewSeq: -1 });
    const running = fixture('idle-happy');
    running.agent = { ...running.agent, test: { state: 'running', at_ms: 5 } };
    assert.equal(O.seenAtStart(running).seen.test_at_ms, 0, 'a running ready check is not seen yet');
});


test('desktop placement validates saved coordinates and writes one structured host setting', () => {
    const p = {x: 0.2, y: 0.7, pinned: true, screen: 'DP-1'};
    assert.deepEqual(plain(F.placementSetting(shellJson({id: F.PLUGIN_ID, desktopPlacement: p}))), p);
    for (const bad of [null, [], {}, {x: -1, y: 0}, {x: 0, y: 2}, {x: '0', y: 0}]) {
        const got = F.placementSetting(shellJson({id: F.PLUGIN_ID, desktopPlacement: bad}));
        assert.equal(got.x, null);
        assert.equal(got.y, null);
        assert.equal(got.pinned, false);
    }
    assert.deepEqual(plain(F.placementArgv('/usr/share/omarchy', p)),
        ['qs', 'ipc', '-p', '/usr/share/omarchy/shell', 'call', '--', 'shell', 'setBarWidget',
         F.PLUGIN_ID, 'desktopPlacement', JSON.stringify(p), '{}']);
});


test('observer bubble labels describe external activity only and follow expiry', () => {
    const names = P.CLIENT_NAMES;
    assert.equal(O.observerLabel({state: 'working', bubble: {apps: ['codex']}}, names), 'Codex working');
    assert.equal(O.observerLabel({state: 'waiting', bubble: {apps: ['codex']}}, names), 'Codex needs you');
    assert.equal(O.observerLabel({state: 'working', bubble: {apps: ['codex', 'hermes']}}, names), 'Codex +1 working');
    for (const state of ['need_now', 'need_soon', 'celebrate', 'idle', 'off'])
        assert.equal(O.observerLabel({state, bubble: {apps: ['codex']}}, names), '');
    assert.equal(O.observerLabel({state: 'working', bubble: {need: 'food'}}, names), '', 'owned job is not an observed client');
    assert.equal(O.observerLabel({state: 'working', bubble: {apps: ['unknown']}}, names), '');
    const now = 1700000000000, doc = fixture('idle-happy');
    doc.agent = {apps: [{id: 'codex', state: 'working', since_ms: now - 10, stale_at_ms: now + 500}]};
    assert.equal(O.observerLabel(P.currentPriority(doc, now, {}), names), 'Codex working');
    assert.equal(O.observerLabel(P.currentPriority(doc, now + 500, {}), names), '', 'stale activity clears at its existing deadline');
    doc.agent.apps = [];
    assert.equal(O.observerLabel(P.currentPriority(doc, now, {}), names), '', 'completed activity clears');
});
