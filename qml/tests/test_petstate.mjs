// Node tests for the generated shared-priority module petstate.js (SPEC-v0.3 §6.4).
// Run: node --test qml/tests/test_petstate.mjs
// Replays every `priority` vector of docs/v0.3/contracts/agent_vectors.json against the
// frozen bar-v2 fixtures, through both shipped copies, and checks the copies are the
// generator's current output.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const url = (rel) => new URL(rel, import.meta.url);
const read = (rel) => readFileSync(url(rel), 'utf8');
const plain = (x) => JSON.parse(JSON.stringify(x));
const COPIES = ['../shared/petstate.js', '../../omarchy-plugin/petstate.js'];
const VECTORS_REL = '../../docs/v0.3/contracts/agent_vectors.json';
const V = JSON.parse(read(VECTORS_REL));
const FIXTURE_DIR = url('../../docs/v0.3/contracts/fixtures/bar-v2/');
const FIXTURES = Object.fromEntries(readdirSync(FIXTURE_DIR).filter((f) => f.endsWith('.json'))
    .map((f) => [f.slice(0, -5), JSON.parse(readFileSync(new URL(f, FIXTURE_DIR), 'utf8'))]));
const STRINGS = JSON.parse(read('../../docs/v0.3/contracts/strings.json'));

function load(rel) {
    const src = read(rel);
    assert.match(src, /^\.pragma library\n/, `${rel} must stay a .pragma library`);
    assert.doesNotMatch(src, /^\s*(import|\.import)\b/m, `${rel} must import nothing`);
    const ctx = {};
    vm.createContext(ctx);
    vm.runInContext(src.replace(/^\.pragma library$/m, ''), ctx, { filename: rel });
    return ctx;
}

const MODULES = COPIES.map((rel) => [rel, load(rel)]);
const P = MODULES[0][1];
const SEEN0 = { finished_seq: 0, hello_seq: 0, test_at_ms: 0 };

test('both copies are byte-identical and current with the generator', () => {
    assert.equal(read(COPIES[0]), read(COPIES[1]));
    const gen = fileURLToPath(url('../../tools/petstate/gen_petstate.py'));
    const r = spawnSync('python3', ['-I', gen, '--check'], { encoding: 'utf8' });
    assert.equal(r.status, 0, r.stderr);
    const digest = createHash('sha256').update(readFileSync(url(VECTORS_REL))).digest('hex');
    assert.equal(P.SOURCE_SHA256, digest, 'regenerate after changing agent_vectors.json');
});

test('tables come from the contract', () => {
    assert.deepEqual(plain(P.CLIENTS), V.clients.map((c) => c.id));
    for (const c of V.clients) assert.equal(P.appName(c.id), c.name);
    assert.equal(P.appName('nope'), '');
    assert.equal(P.appName('toString'), '', 'no prototype names');
    assert.deepEqual(plain(P.NEED_LEVELS), V.priority.need_levels);
    assert.deepEqual(plain(P.NEED_PRIORITY), V.priority.need_priority);
    for (const k of Object.keys(P.C)) assert.equal(P.C[k], V.constants[k], k);
    assert.deepEqual(plain(P.ROWS.map((r) => r.state)), V.priority.order.map((r) => r.state));
});

for (const [rel, M] of MODULES) {
    test(`every priority vector (${rel})`, () => {
        assert.ok(V.priority.vectors.length >= 30);
        for (const vec of V.priority.vectors) {
            const doc = FIXTURES[vec.fixture];
            assert.ok(doc, `${vec.name}: fixture ${vec.fixture}`);
            const got = plain(M.sharedPriority(doc, vec.now, vec.seen));
            for (const [k, want] of Object.entries(vec.expect))
                assert.deepEqual(got[k], want, `${vec.name}: ${k}`);
        }
    });
}

test('every line the face can name exists in strings.json with exactly its vars', () => {
    const ph = (t) => new Set([...t.matchAll(/\{(\w+)\}/g)].map((m) => m[1]));
    for (const vec of V.priority.vectors) {
        const f = P.sharedPriority(FIXTURES[vec.fixture], vec.now, vec.seen);
        if (f.line === null) continue;
        assert.ok(f.line in STRINGS, `${vec.name}: ${f.line}`);
        assert.deepEqual([...ph(STRINGS[f.line])].sort(), Object.keys(f.vars).sort(), `${vec.name}: ${f.line}`);
    }
});

test('celebrate names its candidate so a UI can mark it seen once', () => {
    const fin = P.sharedPriority(FIXTURES['finished-treat'], 1792123900000, SEEN0);
    assert.equal(fin.state, 'celebrate');
    assert.equal(fin.celebrate.kind, 'finished');
    const seen = { ...SEEN0, finished_seq: fin.celebrate.seq };
    assert.equal(P.sharedPriority(FIXTURES['finished-treat'], 1792123900000, seen).state, 'idle');
    const hello = P.sharedPriority(FIXTURES.hello, 1792123900000, SEEN0);
    assert.equal(hello.celebrate.kind, 'hello');
    assert.equal(P.sharedPriority(FIXTURES.hello, 1792123900000, { ...SEEN0, hello_seq: hello.celebrate.seq }).state,
                 'idle');
    const ready = P.sharedPriority(FIXTURES['ready-check-done'], 1792123900000, SEEN0);
    assert.equal(ready.celebrate.kind, 'test');
    assert.notEqual(P.sharedPriority(FIXTURES['ready-check-done'], 1792123900000,
                                     { ...SEEN0, test_at_ms: ready.celebrate.at_ms }).state, 'celebrate');
    const grew = P.sharedPriority(FIXTURES['grew-up'], 1792123900000, SEEN0);
    assert.equal(grew.celebrate.kind, 'grew');
    assert.ok(grew.celebrate.seq > 0);
    assert.equal(P.sharedPriority(FIXTURES['idle-happy'], 1792123900000, SEEN0).celebrate, null);
});

test('robust to v1 exports, missing blocks and junk', () => {
    const v1 = JSON.parse(JSON.stringify(FIXTURES['idle-happy']));
    v1.schema = 'codepet.bar.v1';
    delete v1.enabled; delete v1.agent; delete v1.treats; delete v1.clients;
    const f = P.sharedPriority(v1, 1792123900000, undefined);
    assert.equal(f.state, 'idle');
    assert.equal(f.treats_visible, false);
    assert.equal(f.wake_at_ms, null);
    for (const junk of [null, undefined, 7, 'x', [], {}]) {
        const g = P.sharedPriority(junk, 0, null);
        assert.ok(g && typeof g === 'object');
    }
    assert.equal(P.sharedPriority(null, 0, null).state, null);
    assert.equal(P.sharedPriority({}, 0, null).state, 'no_pet');
    const odd = JSON.parse(JSON.stringify(FIXTURES['working-one']));
    odd.agent.apps.push(null, { id: 'claude-code', state: 'working' }, 'x');
    odd.needs.push(null, { action: '__proto__', state: 'empty' });
    assert.equal(P.sharedPriority(odd, 1792123900000, SEEN0).state, 'working');
});

test('Off and the screens without a pet arm no timer (K9)', () => {
    for (const name of ['off', 'off-treats', 'off-no-pet', 'fresh-no-pet', 'memorial', 'owner-unarmed-welcome']) {
        const f = P.sharedPriority(FIXTURES[name], 1792123900000, SEEN0);
        assert.equal(f.wake_at_ms, null, name);
        assert.equal(f.pulse, false, name);
        assert.equal(f.overlay === 'none' || f.overlay === 'wander', true, name);
    }
});
