// Node tests for omarchy-plugin/DesktopPetFlow.js, the bar side of "Pet on my desktop"
// (SPEC-v0.3 §3.1, §7.2 decision 5), and its agreement with the overlay's reader
// (qml/omarchy/OverlayFlow.js). Run: node --test qml/tests/test_desktop_pet_flow.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
const D = load('../../omarchy-plugin/DesktopPetFlow.js');
const F = load('../omarchy/OverlayFlow.js');
const fixture = (n) => JSON.parse(read(`../../docs/v0.3/contracts/fixtures/bar-v2/${n}.json`));
const MANIFEST = JSON.parse(read('../../omarchy-plugin/manifest.json'));
const asShellJson = (settings) => JSON.stringify({ bar: { layout: { right: [{ id: MANIFEST.id, ...settings }] } } });

test('the switch is the manifest setting the overlay reads', () => {
    assert.ok(MANIFEST.barWidget.schema.some((f) => f.key === D.SETTING && f.type === 'boolean'));
    assert.equal(MANIFEST.barWidget.defaults[D.SETTING], false, 'off until the first Let\'s go!');
    assert.equal(D.SETTING, F.SETTING);
});

test("Let's go! turns the desktop pet on and shows it (decision 5)", () => {
    const r = D.afterStart({ id: 'greyforge.codepet', label: 'name', presence: true, overlayOnLoad: false });
    assert.deepEqual(plain(r), { settings: { label: 'name', presence: true, overlayOnLoad: true }, changed: true, launch: 'show' });
    assert.equal(F.desktopPetSetting(asShellJson(r.settings)), true, 'the overlay reads it as on');
    assert.equal(D.afterStart({ overlayOnLoad: true }).changed, false, 'already on: nothing to write');
    assert.equal(D.afterStart({ overlayOnLoad: 'true' }).changed, true, 'a string is normalised');
    assert.deepEqual(plain(D.afterStart(null).settings), { overlayOnLoad: true });
});

test('the right-click switch: on shows, off needs no verb (the pet exits by itself)', () => {
    const on = D.setDesktopPet({ overlayOnLoad: false, label: 'status' }, true);
    assert.deepEqual(plain(on), { settings: { overlayOnLoad: true, label: 'status' }, changed: true, launch: 'show' });
    const off = D.setDesktopPet({ overlayOnLoad: true, label: 'status' }, false);
    assert.deepEqual(plain(off), { settings: { overlayOnLoad: false, label: 'status' }, changed: true, launch: null });
    assert.equal(F.desktopPetSetting(asShellJson(off.settings)), false);
    assert.equal(F.exitReason(fixture('idle-happy'), F.desktopPetSetting(asShellJson(off.settings))), 'desktop_pet_off');
    assert.equal(D.setDesktopPet({}, 'yes').settings.overlayOnLoad, false, 'only a real true turns it on');
});

test('bar load shows the pet only when on, CodePet on and a pet started', () => {
    const on = { overlayOnLoad: true };
    assert.equal(D.launchOnLoad(on, fixture('idle-happy')), 'show');
    assert.equal(D.launchOnLoad(on, fixture('working-one')), 'show');
    assert.equal(D.launchOnLoad({ overlayOnLoad: false }, fixture('idle-happy')), null);
    assert.equal(D.launchOnLoad({}, fixture('idle-happy')), null, 'the v0.2 default is off');
    assert.equal(D.launchOnLoad(on, fixture('off')), null, 'Off: no desktop pet');
    assert.equal(D.launchOnLoad(on, fixture('fresh-no-pet')), null, 'no pet yet');
    assert.equal(D.launchOnLoad(on, fixture('owner-unarmed-welcome')), null, 'before Let\'s go!');
    assert.equal(D.launchOnLoad(on, null), null, 'no file yet');
});

test('turning CodePet back on brings the pet back, only then', () => {
    const on = { overlayOnLoad: true };
    assert.equal(D.launchOnEnabled(on, false, true), 'show');
    assert.equal(D.launchOnEnabled(on, true, true), null);
    assert.equal(D.launchOnEnabled(on, true, false), null, 'Off: the pet exits by itself');
    assert.equal(D.launchOnEnabled({ overlayOnLoad: false }, false, true), null);
});
