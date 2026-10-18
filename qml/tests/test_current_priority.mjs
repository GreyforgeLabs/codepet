import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

function load(path) {
    const context = vm.createContext({});
    vm.runInContext(readFileSync(new URL(path, import.meta.url), 'utf8').replace(/^\.pragma library\n/, ''), context);
    return context;
}
const P = load('../shared/petstate.js');
const O = load('../shared/OverlayFace.js');
const now = 1700000000000;
function doc(food = 10000) {
    return { enabled: true, life: { name: 'Pip', armed: true },
        meters: { hunger: food, thirst: 10000, mental_health: 10000, hygiene: 10000 },
        needs: [{ action: 'food', state: food === 0 ? 'empty' : 'ok' }],
        agent: { apps: [{ id: 'codex', state: 'working', since_ms: now - 1000, stale_at_ms: now + 60000 }] },
        treats: { jar: 0 }, unseen_events: [] };
}

test('current face keeps early and urgent care visible through observed and owned work', () => {
    for (const food of [0, 3000, 6000]) for (const external of ['working', 'waiting']) {
        const state = doc(food); state.agent.apps[0].state = external;
        const face = P.currentPriority(state, now, {});
        assert.equal(face.state, food <= 3000 ? 'need_now' : 'need_soon');
        assert.equal(face.bubble.need, 'food');
        assert.equal(face.bar, 'need_food');
        for (const status of ['preparing', 'running', 'review', 'applying', 'scouted']) {
            const owned = O.withWork(face, { task: { status, source_kind: 'scout', has_finding: true } });
            assert.equal(owned.state, face.state);
            assert.equal(owned.bubble.need, 'food');
        }
    }
    assert.equal(P.currentPriority(doc(6001), now, {}).state, 'working');
});

test('growth is shown through activity; urgent care and rest retain precedence', () => {
    const state = doc(); state.unseen_events = [{ seq: 5, kind: 'evolved' }];
    let face = P.currentPriority(state, now, {});
    assert.equal(face.line, 'grew');
    assert.equal(O.withWork(face, { task: { status: 'running', kind: 'bug' } }).line, 'grew');
    state.needs = [{ action: 'water', state: 'critical' }];
    assert.equal(P.currentPriority(state, now, {}).state, 'need_now');
    state.care = { sleeping: true };
    assert.equal(P.currentPriority(state, now, {}).state, 'asleep');
    state.enabled = false;
    assert.equal(P.currentPriority(state, now, {}).state, 'off');
});
