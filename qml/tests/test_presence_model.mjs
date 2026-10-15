// Node tests for omarchy-plugin/PresenceModel.js (the bar leader's attended-presence
// recorder). Run: node --test qml/tests/test_presence_model.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

function loadLibrary(rel) {
    const src = readFileSync(new URL(rel, import.meta.url), 'utf8');
    assert.match(src, /^\.pragma library\n/, `${rel} must stay a .pragma library`);
    assert.doesNotMatch(src, /^\s*(import|\.import)\b/m, `${rel} must not import anything`);
    const ctx = {};
    vm.createContext(ctx);
    vm.runInContext(src.replace(/^\.pragma library$/m, ''), ctx, { filename: rel });
    return ctx;
}

const P = loadLibrary('../../omarchy-plugin/PresenceModel.js');
const plain = (x) => JSON.parse(JSON.stringify(x));
const LIFE = 'life-0123456789abcdef01234567';
const LIFE2 = 'life-fedcba9876543210fedcba98';
const T0 = 1_760_000_000_000;

function fresh(now = T0) {
    const m = P.create(P.newSession(now, () => 0.5), LIFE, now);
    assert.ok(m, 'model created');
    P.start(m, now);
    return m;
}

// Drives n ticks of `gap` ms each; returns the final wall time.
function run(m, from, gap, n, eligible = true, idle = false) {
    let now = from;
    for (let i = 0; i < n; i++) {
        now += gap;
        P.onTick(m, now, eligible, idle);
    }
    return now;
}

test('newSession matches the Rust session pattern and is per call', () => {
    const a = P.newSession(T0, () => 0.1);
    const b = P.newSession(T0, () => 0.9);
    assert.match(a, /^bar-[a-z0-9-]{6,58}$/);
    assert.match(b, /^bar-[a-z0-9-]{6,58}$/);
    assert.notEqual(a, b);
    assert.match(P.newSession(T0, Math.random), /^bar-[a-z0-9-]{6,58}$/);
    // A broken random source still yields a valid id.
    assert.match(P.newSession(T0, () => NaN), /^bar-[a-z0-9-]{6,58}$/);
});

test('create refuses invalid session or life ids', () => {
    assert.equal(P.create('nope', LIFE, T0), null);
    assert.equal(P.create(P.newSession(T0, () => 0.2), 'life-XYZ', T0), null);
    assert.equal(P.create(P.newSession(T0, () => 0.2), LIFE, NaN), null);
});

test('timer start records the wall time and never credits', () => {
    const m = fresh();
    assert.equal(m.attended_ms, 0);
    assert.equal(m.ticks, 0);
    assert.equal(m.pending.length, 0);
    assert.equal(m.lastWall, T0);
});

test('gap 49,999 ms is never credited', () => {
    const m = fresh();
    run(m, T0, 49_999, 10);
    assert.equal(m.attended_ms, 0);
    assert.equal(m.ticks, 0);
});

test('gap 90,001 ms is never credited', () => {
    const m = fresh();
    run(m, T0, 90_001, 10);
    assert.equal(m.attended_ms, 0);
    assert.equal(m.ticks, 0);
});

test('gaps of exactly 50,000 and 90,000 ms credit after the 180 s look-back', () => {
    for (const gap of [50_000, 60_000, 90_000]) {
        const m = fresh();
        // First tick: pending, nothing committed until it is 180 s old.
        let now = run(m, T0, gap, 1);
        assert.equal(m.ticks, 0, `gap ${gap}: no commit before look-back`);
        assert.equal(m.pending.length, 1);
        // Keep ticking until the first pending tick is >= 180 s old.
        let n = 1;
        while (m.ticks === 0) { now = run(m, now, gap, 1); n++; }
        assert.ok(now - (T0 + gap) >= 180_000, `gap ${gap}: committed only after look-back`);
        assert.equal(m.ticks, 1);
        assert.equal(m.attended_ms, Math.min(gap, 60_000), `gap ${gap}: credit is min(gap, 60 s)`);
        assert.ok(P.valid(m, now), `gap ${gap}: payload passes the Rust bounds`);
    }
});

test('steady 60 s ticks commit one minute per tick after the first three', () => {
    const m = fresh();
    const now = run(m, T0, 60_000, 10);
    // ticks stamped at +60..+600; those <= now-180 s (+420) are committed: 7.
    assert.equal(m.ticks, 7);
    assert.equal(m.attended_ms, 7 * 60_000);
    assert.equal(m.pending.length, 3);
    assert.equal(m.updated_wall_ms, now);
    assert.ok(P.valid(m, now));
});

test('a negative gap (clock jump back) clears pending ticks', () => {
    const m = fresh();
    let now = run(m, T0, 60_000, 2);
    assert.equal(m.pending.length, 2);
    now -= 30_000;
    P.onTick(m, now, true, false);
    assert.equal(m.pending.length, 0);
    assert.equal(m.ticks, 0);
});

test('a long gap (suspend) clears pending ticks', () => {
    const m = fresh();
    let now = run(m, T0, 60_000, 2);
    now += 3_600_000;
    P.onTick(m, now, true, false);
    assert.equal(m.pending.length, 0);
    assert.equal(m.attended_ms, 0);
});

test('idle (and lock, seen as idle) drops the last <= 3 pending ticks', () => {
    const m = fresh();
    let now = run(m, T0, 60_000, 6); // 3 committed, 3 pending
    assert.equal(m.ticks, 3);
    assert.equal(m.pending.length, 3);
    P.onTick(m, now + 60_000, true, true); // idle notification arrives
    assert.equal(m.pending.length, 0, 'pending ticks dropped');
    assert.equal(m.ticks, 3, 'committed minutes are kept');
    assert.equal(m.lastWall, null, 'the idle gap is never credited later');
    // Coming back: the timer restarts and the first tick after it does not credit.
    now += 600_000;
    P.start(m, now);
    run(m, now, 60_000, 3);
    assert.equal(m.ticks, 3, 'look-back applies again after idle');
});

test('ineligible (fullscreen, paused, setting off) clears pending and stops writes', () => {
    const m = fresh();
    let now = run(m, T0, 60_000, 3);
    assert.equal(m.pending.length, 3);
    const committed = P.onTick(m, now + 60_000, false, false);
    assert.equal(committed, 0, 'no write is due');
    assert.equal(m.pending.length, 0);
    const before = P.payload(m);
    // Further ineligible ticks never change the payload.
    for (let i = 2; i < 10; i++) assert.equal(P.onTick(m, now + i * 60_000, false, false), 0);
    assert.equal(P.payload(m), before);
});

test('stop and clearPending report how many ticks were dropped', () => {
    const m = fresh();
    run(m, T0, 60_000, 2);
    assert.equal(P.clearPending(m), 2);
    assert.equal(P.clearPending(m), 0);
    assert.equal(P.stop(null), 0);
});

test('payload has exactly the seven fields Rust accepts', () => {
    const m = fresh();
    const now = run(m, T0, 60_000, 5);
    const doc = JSON.parse(P.payload(m));
    assert.deepEqual(Object.keys(doc).sort(),
        ['attended_ms', 'life_id', 'session', 'started_wall_ms', 'ticks', 'updated_wall_ms', 'v']);
    assert.equal(doc.v, 1);
    assert.equal(doc.life_id, LIFE);
    assert.equal(doc.ticks, 2);
    assert.equal(doc.attended_ms, 120_000);
    assert.equal(doc.updated_wall_ms, now);
    assert.ok(P.payload(m).endsWith('\n'));
    assert.ok(Buffer.byteLength(P.payload(m)) <= 2048, 'fits the 2 KiB file bound');
});

test('Rust bounds hold over a long session with slightly fast or jittery timers', () => {
    for (const gaps of [[50_000], [59_900], [60_000], [61_000, 59_000, 89_000, 50_500]]) {
        const m = fresh();
        let now = T0;
        for (let i = 0; i < 3_000; i++) {
            now += gaps[i % gaps.length];
            P.onTick(m, now, true, false);
            assert.ok(P.valid(m, now), `bounds hold at tick ${i} with gaps ${gaps}`);
        }
        assert.ok(m.ticks > 2_900);
        assert.ok(m.attended_ms <= m.ticks * 60_000);
        assert.ok(m.attended_ms <= m.updated_wall_ms - m.started_wall_ms + 60_000);
    }
});

test('a life change starts a new session: no session spans two lives', () => {
    const a = fresh();
    run(a, T0, 60_000, 6);
    const b = P.create(P.newSession(T0 + 1, () => 0.7), LIFE2, T0 + 400_000);
    assert.notEqual(a.session, b.session);
    assert.equal(b.life_id, LIFE2);
    assert.equal(b.attended_ms, 0, 'the new life starts from zero');
    assert.equal(b.ticks, 0);
});

test('sleeping does not stop recording while presence is wanted', () => {
    // The recorder never looks at sleep: eligibility is setting && wanted && !idle && !fullscreen.
    // A nap therefore keeps ticking and Rust discards the nap portion itself.
    const src = readFileSync(new URL('../../omarchy-plugin/PresenceRecorder.qml', import.meta.url), 'utf8');
    const eligible = src.match(/readonly property bool eligible:([^\n]*)/)[1];
    assert.doesNotMatch(eligible, /sleep/i);
    assert.match(eligible, /wanted/);
    assert.match(eligible, /!idle/);
    assert.match(eligible, /!fullscreen/);
    const m = fresh();
    run(m, T0, 60_000, 10, true, false);
    assert.equal(m.ticks, 7);
});

test('attendedSince counts only commits after the given wall time', () => {
    const m = fresh();
    const now = run(m, T0, 60_000, 10); // commits at +240 .. +600
    assert.equal(P.attendedSince(m, T0), 7 * 60_000);
    assert.equal(P.attendedSince(m, T0 + 420_000), 3 * 60_000);
    assert.equal(P.attendedSince(m, now), 0);
    assert.equal(P.attendedSince(null, T0), 0);
    assert.equal(P.attendedSince(m, NaN), 0);
});

test('history stays bounded', () => {
    const m = fresh();
    run(m, T0, 60_000, 1_000);
    assert.ok(m.history.length <= P.MAX_HISTORY);
    assert.equal(m.history.length, m.historyCredit.length);
    assert.equal(P.attendedSince(m, T0), P.MAX_HISTORY * 60_000, 'prediction input caps at 8 h');
});

test('plain JSON round trip of the model is stable', () => {
    const m = fresh();
    run(m, T0, 60_000, 4);
    assert.deepEqual(plain(JSON.parse(P.payload(m))).ticks, 1);
});
