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

test('settings path follows XDG rules', () => {
    assert.equal(F.settingsPath('/x/config', '/home/u'), '/x/config/omarchy/shell.json');
    assert.equal(F.settingsPath('/x/config/', '/home/u'), '/x/config/omarchy/shell.json');
    assert.equal(F.settingsPath('', '/home/u'), '/home/u/.config/omarchy/shell.json');
    assert.equal(F.settingsPath('relative', '/home/u'), '/home/u/.config/omarchy/shell.json');
    assert.equal(F.settingsPath('/a/../b', '/home/u'), '/home/u/.config/omarchy/shell.json');
    assert.equal(F.settingsPath(undefined, 'relative'), '');
    assert.equal(F.settingsPath(null, ''), '');
});

test('"Pet on my desktop" is the bar entry\'s overlayOnLoad, read like the bar plugin', () => {
    const id = 'greyforge.codepet';
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: true })), true);
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: 'true' })), true);
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: false })), false);
    assert.equal(F.desktopPetSetting(shellJson({ id })), false, 'missing key = manifest default (off)');
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: 1 })), false);
    assert.equal(F.desktopPetSetting(shellJson({ id, overlayOnLoad: true }, 'flat')), true, 'older flat bar sections');
    // Unknown: no file, junk, no CodePet cell. A standalone overlay keeps running.
    assert.equal(F.desktopPetSetting(shellJson(undefined)), null);
    assert.equal(F.desktopPetSetting(''), null);
    assert.equal(F.desktopPetSetting('{nope'), null);
    assert.equal(F.desktopPetSetting('[]'), null);
    assert.equal(F.desktopPetSetting(JSON.stringify({ bar: 3 })), null);
    assert.equal(F.desktopPetSetting(undefined), null);
    const big = shellJson({ id, overlayOnLoad: true }).replace('"version":1', `"pad":"${'x'.repeat(1048576)}","version":1`);
    assert.equal(F.desktopPetSetting(big), null, 'over 1 MiB');
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
});

test('Home opens through the Omarchy shell, silently', () => {
    assert.deepEqual(plain(F.openHomeArgv('/usr/share/omarchy')),
                     ['/usr/share/omarchy/bin/omarchy-shell', '-q', 'shell', 'summon', 'greyforge.codepet']);
    assert.deepEqual(plain(F.openHomeArgv('')), ['omarchy-shell', '-q', 'shell', 'summon', 'greyforge.codepet']);
    assert.deepEqual(plain(F.openHomeArgv('relative/x')), ['omarchy-shell', '-q', 'shell', 'summon', 'greyforge.codepet']);
    assert.equal(F.PLUGIN_ID, JSON.parse(read('../../omarchy-plugin/manifest.json')).id);
    for (const v of ['panel', 'tasks', 'care', 'companion', 'life']) assert.ok(F.opensHome(v), v);
    for (const v of ['', 'show', 'hide', 'refresh', 'toString']) assert.ok(!F.opensHome(v), v);
});

test('every overlay value of the shared order has an animation', () => {
    const values = new Set(V.priority.vectors.map((v) => v.expect.overlay));
    for (const ov of values) assert.ok(Object.prototype.hasOwnProperty.call(O.ANIMATIONS, ov), ov);
    assert.equal(O.animation('none').state, '');
    assert.equal(O.animation('sleep').state, 'sleep');
    assert.equal(O.animation('waiting').bob, true);
    assert.equal(O.animation('working').roam, false);
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
