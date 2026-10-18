// Node tests for the bar plugin's pure model (v0.3): omarchy-plugin/CodePetBar.js,
// omarchy-plugin/petstate.js and the generated omarchy-plugin/Strings.js.
// The contract vectors (docs/v0.3/contracts/agent_vectors.json: priority, popup_routing,
// connect_card, helpers_rows) are replayed here against the JavaScript the plugin runs.
// Run: node --test qml/tests/test_bar_model.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
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
const json = (rel) => JSON.parse(readFileSync(new URL(rel, import.meta.url), 'utf8'));
const plain = (x) => JSON.parse(JSON.stringify(x));

const M = loadLibrary('../../omarchy-plugin/CodePetBar.js');
const P = loadLibrary('../../omarchy-plugin/petstate.js');
const S = loadLibrary('../../omarchy-plugin/Strings.js').STRINGS;
const CONTRACTS = '../../docs/v0.3/contracts/';
const V = json(CONTRACTS + 'agent_vectors.json');
const STRINGS = json(CONTRACTS + 'strings.json');
const REG = json('../assets/pets/registry.json');
const FIXTURES = Object.fromEntries(readdirSync(new URL(CONTRACTS + 'fixtures/bar-v2/', import.meta.url))
    .filter((f) => f.endsWith('.json'))
    .map((f) => [f.slice(0, -5), json(CONTRACTS + 'fixtures/bar-v2/' + f)]));
const STATUS = (name) => json(CONTRACTS + `fixtures/clients-status/${name}.json`);
// A real v0.2 `codepet rpc` export (schema codepet.bar.v1), kept by a storage test.
const V1_EXPORT = json('./fixtures/bar-state.v1.json');

test('Strings.js is the plugin strings.json: every contract string plus the WP5 additions', () => {
    const plugin = json('../../omarchy-plugin/strings.json');
    const extra = json('../../tools/ui/strings-wp5.json');
    assert.deepEqual(plain(S), plugin);
    for (const [k, v] of Object.entries(STRINGS)) assert.equal(plugin[k], v, k);
    for (const [k, v] of Object.entries(extra)) {
        assert.ok(!(k in STRINGS), `${k} is an addition, not an override`);
        assert.equal(plugin[k], v, k);
    }
    assert.equal(Object.keys(plugin).length, Object.keys(STRINGS).length + Object.keys(extra).length);
});

test('parse accepts codepet.bar.v1 and v2 and refuses everything else', () => {
    for (const doc of [FIXTURES['idle-happy'], { ...FIXTURES['idle-happy'], schema: 'codepet.bar.v1' }]) {
        const got = M.parse(JSON.stringify(doc));
        assert.ok(got, doc.schema);
        assert.equal(got.state_version, doc.state_version);
    }
    for (const bad of ['', 'nope', '[]', JSON.stringify({ ...FIXTURES['idle-happy'], schema: 'codepet.bar.v3' }),
        JSON.stringify({ ...FIXTURES['idle-happy'], state_version: -1 }),
        JSON.stringify({ ...FIXTURES['idle-happy'], state_version: 1.5 }),
        JSON.stringify({ ...FIXTURES['idle-happy'], life: 'x' })]) {
        assert.equal(M.parse(bad), null, bad.slice(0, 40));
    }
});

test('a v0.2 export reads as v0.3 with the On, agent, treat and helper defaults', () => {
    const got = M.parse(JSON.stringify(V1_EXPORT));
    assert.ok(got);
    assert.equal(got.enabled, true);
    assert.deepEqual(plain(got.treats), { jar: 0, full: false });
    assert.deepEqual(plain(got.clients), {});
    assert.equal(got.agent.state, 'idle');
    assert.deepEqual(plain(got.agent.apps), []);
    const face = P.sharedPriority(got, Date.now(), {});
    assert.ok(['idle', 'need_soon', 'need_now', 'welcome', 'no_pet'].includes(face.state), face.state);
});

test('parse bounds the file to 16 KiB (bytes, not characters)', () => {
    const doc = { ...FIXTURES['idle-happy'], pad: 'é'.repeat(9000) };
    assert.equal(M.parse(JSON.stringify(doc)), null);
});

test('parseRegistry, ownSettings and config', () => {
    assert.equal(M.parseRegistry('{}'), null);
    assert.equal(M.parseRegistry('nope'), null);
    assert.ok(M.parseRegistry(JSON.stringify(REG)));
    const shell = { bar: { layout: { right: [{ id: 'greyforge.codepet', label: 'none', calm: true }] } } };
    assert.deepEqual(plain(M.ownSettings(JSON.stringify(shell), 'greyforge.codepet', { label: 'name', id: 'x' })),
        { label: 'none', calm: true });
    assert.deepEqual(plain(M.ownSettings('garbage', 'greyforge.codepet', { label: 'name', id: 'x' })), { label: 'name' });
    assert.deepEqual(plain(M.config({})), { label: 'name', presence: true, overlayOnLoad: false, calm: false, desktopPlacement: { pinned: false }, binary: '', stateHome: '' });
    const c = M.config({ label: 'status', presence: 'false', overlayOnLoad: true, calm: 'true', binary: 'rel/x', stateHome: '/tmp/qa/' });
    assert.deepEqual(plain(c), { label: 'name', presence: false, overlayOnLoad: true, calm: true, desktopPlacement: { pinned: false }, binary: '', stateHome: '/tmp/qa' });
    assert.equal(M.config({ label: 'none' }).label, 'none');
});

test('paths: defaults, XDG overrides, test stateHome and Calm mode reach the launcher', () => {
    const env = { HOME: '/home/u', XDG_STATE_HOME: '', XDG_DATA_HOME: '/d', XDG_CONFIG_HOME: '' };
    const p = M.paths(M.config({}), env);
    assert.equal(p.barState, '/home/u/.local/state/codepet/bar-state.json');
    assert.equal(p.registry, '/d/codepet/current/qml/assets/pets/registry.json');
    assert.equal(p.binary, '/home/u/.local/bin/codepet');
    assert.deepEqual(plain(p.childEnv), {});
    assert.deepEqual(plain(p.launcherEnv), []);
    const q = M.paths(M.config({ stateHome: '/qa', binary: '/opt/codepet', calm: true }), env);
    assert.equal(q.stateDir, '/qa/codepet');
    assert.ok(q.testState);
    assert.deepEqual(plain(q.childEnv), { XDG_STATE_HOME: '/qa', CODEPET_TEST_STATE: '1' });
    assert.deepEqual(plain(q.launcherEnv), ['CODEPET_BIN=/opt/codepet', 'XDG_STATE_HOME=/qa', 'CODEPET_TEST_STATE=1', 'CODEPET_REDUCED_MOTION=1']);
    for (const v of Object.values(p)) if (typeof v === 'string') assert.ok(!v.includes('omarchy-plugin') && !v.includes('/plugins/'), v);

    const x = M.paths(M.config({}), { HOME: '/home/u', XDG_STATE_HOME: '/s', XDG_DATA_HOME: '/d', XDG_CONFIG_HOME: '/c' });
    assert.equal(x.barState, '/s/codepet/bar-state.json');
    assert.equal(x.runtime, '/d/codepet/current/');
    assert.equal(x.shellConfig, '/home/u/.config/omarchy/shell.json',
        'the file the Omarchy shell reads, whatever XDG_CONFIG_HOME says (notes.md §9 item 2)');
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

test('the bar sends only supported popup methods: never task.*, pet.adopt or configure_life', () => {
    for (const m of ['state.get', 'pet.care', 'pet.start', 'pet.rename', 'treats.give', 'app.set_enabled', 'app.set_helpers',
        'agent.forget_history', 'clients.status', 'clients.connect', 'clients.disconnect', 'clients.test', 'pet.retire'])
        assert.ok(M.allowedMethod(m), m);
    for (const m of ['task.start', 'task.create', 'pet.adopt', 'pet.configure_life', 'providers.inspect', 'hello', ''])
        assert.ok(!M.allowedMethod(m), m);
    const proto = json(CONTRACTS + 'protocol.json');
    for (const m of M.ALLOWED_METHODS)
        assert.ok(m === 'pet.care_tempo' || proto.kept.includes(m) || proto.added.some((a) => a.method === m), `${m} is a supported popup method`);
});

// ------------------------------------------------------------------ contract vectors

test('sharedPriority replays every agent_vectors.json priority vector', () => {
    assert.ok(V.priority.vectors.length >= 30);
    for (const vec of V.priority.vectors) {
        const doc = M.normalize(FIXTURES[vec.fixture]);
        assert.ok(doc, vec.fixture);
        const got = plain(P.sharedPriority(doc, vec.now, vec.seen));
        for (const [k, want] of Object.entries(vec.expect))
            assert.deepEqual(got[k], want, `${vec.name}: ${k}`);
    }
});

test('petstate constants equal the contract constants', () => {
    for (const k of Object.keys(P.C)) assert.equal(P.C[k], V.constants[k], k);
    assert.deepEqual(plain(P.NEED_PRIORITY), V.priority.need_priority);
    assert.deepEqual(plain(P.NEED_LEVELS), V.priority.need_levels);
});

test('popup routing vectors', () => {
    for (const vec of V.popup_routing.vectors)
        assert.equal(M.route(M.normalize(FIXTURES[vec.fixture])), vec.expect, vec.fixture);
    assert.equal(M.route(null), 'loading');
});

test('Helpers rows: fixture and bucket vectors, and Connect all', () => {
    const H = V.helpers_rows;
    assert.equal(M.SEEN_JUST_NOW_MS, H.seen_buckets.seen_just_now_ms);
    for (const vec of H.fixture_vectors) {
        const clients = FIXTURES[vec.fixture].clients;
        for (const [cid, want] of Object.entries(vec.expect.rows))
            assert.deepEqual(plain(M.helperRow(cid, clients[cid] ?? null, vec.now, vec.tz_offset_min)), want, `${vec.fixture} ${cid}`);
        assert.deepEqual(plain(M.connectAll(clients)), vec.expect.connect_all, vec.fixture);
        const rows = M.helperRows(clients, vec.now, vec.tz_offset_min).map((r) => r.id);
        assert.deepEqual(plain(rows), plain(M.CLIENTS).filter((c) => vec.expect.rows[c]?.visible), vec.fixture);
    }
    for (const vec of H.bucket_vectors) {
        const rec = { installed: true, connected: true, needs_step: null, failed_reason: null, job: null,
            last_seen_ms: vec.last_seen_ms, first_seen_ms: vec.first_seen_ms };
        assert.equal(M.helperRow('grok-cli', rec, vec.now, vec.tz_offset_min).row, vec.expect, vec.name);
    }
});

test('the Connect card offers the connect_card vectors (never Codex)', () => {
    for (const vec of V.connect_card.vectors)
        assert.deepEqual(plain(M.connectOffered(STATUS(vec.status_fixture))), vec.expect, vec.status_fixture);
    for (const name of ['after-connect', 'all-connected', 'nothing-installed', 'owner-first-run'])
        assert.ok(!M.connectOffered(STATUS(name)).includes('codex'), name);
});

// ------------------------------------------------------------------ copy

test('copy resolution: habitat overrides, careWords and plain verbs', () => {
    assert.equal(M.careSet(REG, 'cat'), 'floor');
    assert.equal(M.careSet(REG, 'goldfish'), 'aquatic');
    assert.equal(M.careSet(REG, 'axolotl'), 'aquatic');
    assert.equal(M.careSet(REG, 'robot'), 'robot');
    assert.equal(M.careSet(null, 'goldfish'), 'aquatic', 'without a registry the ids still decide');
    assert.equal(M.t(S, 'tile.water', 'aquatic'), 'Fresh water');
    assert.equal(M.t(S, 'tile.wash', 'aquatic'), 'Clean tank');
    assert.equal(M.t(S, 'tile.food', 'aquatic'), 'Feed', 'no override -> base');
    assert.equal(M.t(S, 'tile.food', 'robot'), 'Charge');
    assert.equal(M.t(S, 'rules.1', 'robot'), "Charge me when I'm low!");
    assert.equal(M.t(S, 'act.working.two', 'floor', { name: 'Ginger', app: 'claude-code', app2: 'grok-cli' }),
        'Helping Claude Code and Grok.');
    const word = M.careWord(REG, 'cat', 'food');
    assert.equal(M.reactText(S, REG, 'cat', 'food', false, 'Ginger'), `Yum! ${word}!`);
    assert.equal(M.reactText(S, REG, 'cat', 'wash', false, 'Ginger'), 'Squeaky clean!');
    assert.equal(M.reactText(S, REG, 'cat', 'food', true, 'Ginger'), 'Ginger is full!');
    assert.equal(M.reactText(S, REG, 'goldfish', 'water', false, 'Ginger'), 'Ahh! Fresh water!');
    assert.equal(M.reactText(S, REG, 'goldfish', 'wash', true, 'Ginger'), 'The tank is clean!');
    assert.equal(M.reactText(S, REG, 'robot', 'food', false, 'Bolt'), 'Zap! All charged!');
    assert.equal(M.reactText(S, REG, 'robot', 'play', true, 'Bolt'), 'Bolt played already!', 'base when the set has no override');
    assert.equal(M.reactText(S, null, 'cat', 'food', false, 'Ginger'), 'Yum!', 'no careWords -> the plain verb');
    assert.equal(M.reactText(S, null, 'cat', 'play', false, 'Ginger'), 'Wheee!');
});

test('every face line renders filled, short and jargon-free (K1, K2, K8)', () => {
    for (const vec of V.priority.vectors) {
        const doc = M.normalize(FIXTURES[vec.fixture]);
        const face = P.sharedPriority(doc, vec.now, vec.seen);
        const line = M.faceLine(S, REG, doc, face);
        if (!face.line) { assert.equal(line, ''); continue; }
        assert.ok(line.length > 0, vec.name);
        assert.doesNotMatch(line, /[{}]/, `${vec.name}: ${line}`);
        assert.deepEqual(plain(M.kidTextProblems(line)), [], `${vec.name}: ${line}`);
    }
});

test('every species reacts in kid words for every need (K2, K8)', () => {
    for (const id of M.speciesIds(REG)) {
        for (const need of M.CARE_NEEDS) for (const full of [false, true]) {
            const s = M.reactText(S, REG, id, need, full, M.defaultName(REG, id));
            assert.ok(s && !/[{}]/.test(s), `${id} ${need}`);
            assert.deepEqual(plain(M.kidTextProblems(s)), [], `${id} ${need}: ${s}`);
        }
        assert.deepEqual(plain(M.kidTextProblems(M.defaultName(REG, id))), [], id);
    }
});

test('kidTextProblems: digits, jargon, the verb "turn on" and /reload-plugins', () => {
    assert.deepEqual(plain(M.kidTextProblems('Ginger is happy!')), []);
    assert.deepEqual(plain(M.kidTextProblems('Turn on')), []);
    assert.deepEqual(plain(M.kidTextProblems('! Turned off in Codex')), []);
    assert.deepEqual(plain(M.kidTextProblems('Turn Ginger into…')), []);
    assert.deepEqual(plain(M.kidTextProblems('Or type /reload-plugins.')), []);
    assert.deepEqual(plain(M.kidTextProblems('Level 2')), ['K1 digit or %', 'K2 level']);
    assert.deepEqual(plain(M.kidTextProblems('50%')), ['K1 digit or %']);
    assert.deepEqual(plain(M.kidTextProblems('your turn')), ['K2 turn']);
    assert.deepEqual(plain(M.kidTextProblems('Connect the plugin hook')), ['K2 hook', 'K2 plugin']);
    assert.deepEqual(plain(M.kidTextProblems('one two three four five six seven eight nine')), ['K8 more than 8 words']);
    assert.equal(M.wordCount('… Waiting for Claude Code'), 4);
});

// ------------------------------------------------------------------ Home, picker, bar cell

test('needLevels maps ok/low/empty/critical to happy/soon/now/critical', () => {
    assert.deepEqual(plain(M.needLevels(FIXTURES['need-food-critical'])).food, 'critical');
    assert.deepEqual(plain(M.needLevels(FIXTURES['need-water-now'])).water, 'now');
    assert.deepEqual(plain(M.needLevels(FIXTURES['need-food-soon'])).food, 'soon');
    assert.deepEqual(plain(M.needLevels(FIXTURES['idle-happy'])), { food: 'happy', water: 'happy', play: 'happy', wash: 'happy' });
});

test('Home shows only what can be done (K4, K6)', () => {
    const at = (name) => {
        const doc = M.normalize(FIXTURES[name]);
        return plain(M.homeControls(doc, P.sharedPriority(doc, 1792123900000, {})));
    };
    assert.deepEqual(at('off'), { tiles: [], wake: '', turnOn: true, round: false, treats: false });
    assert.deepEqual(at('asleep'), { tiles: [], wake: 'pet.wake', turnOn: false, round: false, treats: false });
    assert.deepEqual(at('paused'), { tiles: [], wake: 'pet.pause', turnOn: false, round: false, treats: false });
    const fainted = at('fainted');
    assert.deepEqual(fainted.tiles.map((t) => t.need), ['food', 'water']);
    assert.equal(fainted.round, false);
    assert.equal(fainted.treats, false);
    const happy = at('idle-happy');
    assert.deepEqual(happy.tiles.map((t) => t.need), ['food', 'water', 'play', 'wash']);
    assert.ok(happy.round && happy.treats, "empty jar stays visible with an explanation");
    assert.ok(at('idle-treats').treats);
    assert.ok(!at('off-treats').treats, 'no Treats while Off');
    const crit = at('need-food-critical');
    assert.equal(crit.tiles.find((t) => t.need === 'food').glow, 'now');
    assert.equal(at('need-food-soon').tiles.find((t) => t.need === 'food').glow, 'soon');
    for (const name of Object.keys(FIXTURES)) {
        const c = at(name);
        assert.ok(c.tiles.length <= 4, name);
        assert.ok(!(c.turnOn && c.tiles.length), name);
    }
});

test('the thought bubble shows at most three pictures', () => {
    const icons = (name) => {
        const doc = M.normalize(FIXTURES[name]);
        return plain(M.bubbleIcons(REG, doc, P.sharedPriority(doc, 1792123900000, {})));
    };
    assert.deepEqual(icons('working-one'), ['apps/app_claude_code']);
    assert.equal(icons('working-many').length, 3);
    assert.deepEqual(icons('waiting').length, 1);
    assert.deepEqual(icons('fainted'), ['status/bandage']);
    assert.deepEqual(icons('need-water-now'), ['care-floor/water']);
    assert.deepEqual(icons('idle-happy'), []);
    assert.deepEqual(icons('helpers-busy'), ['status/robot_helpers']);
    for (const p of [...icons('working-many'), ...icons('fainted')]) assert.match(M.iconPath(p), /^assets\/icons\/[a-z-]+\/[a-z0-9_]+\.webp$/);
    const icons2 = JSON.parse(readFileSync(new URL('../../omarchy-plugin/assets/icons/index.json', import.meta.url)));
    for (const p of [...Object.values(M.APP_ICONS), ...Object.values(M.BAR_BADGE).filter(Boolean), 'status/bandage',
        'status/robot_helpers', 'care-floor/star', 'status/confetti', 'ui/egg'])
        assert.ok(icons2.icons[p], `${p} is a shipped icon`);
});

test('stars: five steps from this stage floor to the next, all lit at the last stage', () => {
    assert.equal(M.stars({ stage: 1, xp: 0, next_stage_xp: 80 }), 0);
    assert.equal(M.stars({ stage: 1, xp: 16, next_stage_xp: 80 }), 1);
    assert.equal(M.stars({ stage: 1, xp: 79, next_stage_xp: 80 }), 4);
    assert.equal(M.stars({ stage: 2, xp: 120, next_stage_xp: 400 }), 0);
    assert.equal(M.stars({ stage: 2, xp: 336, next_stage_xp: 400 }), 4);
    assert.equal(M.stars({ stage: 3, xp: 400, next_stage_xp: null }), 5);
    assert.equal(M.stars(null), 0);
});

test('names: 24 characters and 96 bytes, trimmed, no control characters', () => {
    assert.equal(M.nameProblem('Ginger'), '');
    assert.equal(M.nameProblem('  Mr Whiskers '), '');
    assert.equal(M.nameProblem('x'.repeat(24)), '');
    assert.equal(M.nameProblem('x'.repeat(25)), 'err.name_too_long');
    assert.equal(M.nameProblem('😺'.repeat(24)), '', '24 emoji are 96 bytes');
    assert.equal(M.nameProblem('😺'.repeat(32)), 'err.name_too_long', '32 emoji, 128 bytes');
    assert.equal(M.nameProblem('   '), 'name.type', 'an empty name asks for one');
    assert.equal(M.nameProblem('a\u0007b'), 'err.name_too_long');
    assert.ok(STRINGS['err.name_too_long']);
    assert.equal(M.cleanName('  Pip '), 'Pip');
});

test('species: the cat first, then manifest order, each with a default name', () => {
    const ids = M.speciesIds(REG);
    assert.equal(ids[0], 'cat');
    assert.equal(ids.length, 15);
    assert.deepEqual(plain(ids), plain(M.SPECIES));
    assert.equal(M.defaultName(REG, 'cat'), 'Ginger');
});

test('frames: loops wrap, one-shots hold, Calm mode shows the reduced frame', () => {
    const id = 'cat';
    const idle = M.stateMeta(REG, id, 'idle');
    assert.equal(M.frameRel(REG, id, 1, 'idle', 0, false), `qml/assets/pets/cat/s1/${idle.frames[0].replace(/\.webp$/, '')}.webp`);
    const food = M.stateMeta(REG, id, 'food');
    assert.ok(!food.loop);
    assert.equal(M.frameRel(REG, id, 2, 'food', 1e9, false), M.frameRel(REG, id, 2, 'food', food.totalMs - 1, false), 'held');
    assert.equal(M.frameRel(REG, id, 2, 'food', 0, true), `qml/assets/pets/cat/s2/${food.frames[food.reducedFrame].replace(/\.webp$/, '')}.webp`);
    assert.equal(M.frameRel(REG, 'nope', 1, 'idle', 0, false), '');
    assert.equal(M.frameRel(REG, '../x', 1, 'idle', 0, false), '');
    assert.equal(M.petStateFor({ state: 'off' }, 0), 'sleep');
    assert.equal(M.petStateFor({ state: 'testing' }, 0), 'working');
    assert.equal(M.petStateFor({ state: 'oops', overlay: 'idle' }, 0), 'idle');
    assert.equal(M.petStateFor({ state: 'oops', overlay: 'error' }, 0), 'error');
});

test('bar cell: every priority bar value draws a shipped badge and a safe thumb', () => {
    const cfg = M.config({});
    const cell = (name, calm = false) => {
        const doc = M.normalize(FIXTURES[name]);
        return plain(M.barCell(doc, P.sharedPriority(doc, 1792123900000, {}), REG, cfg, calm));
    };
    const off = cell('off');
    assert.equal(off.opacity, 0.4);
    assert.equal(off.badge, 'bar/badge_moon');
    assert.equal(cell('off-no-pet').thumb, 'ui/egg');
    const egg = cell('fresh-no-pet');
    assert.ok(egg.thumbIsIcon && egg.hi);
    assert.equal(egg.badge, 'bar/badge_sparkle');
    const hi = cell('owner-unarmed-welcome');
    assert.ok(hi.hi && !hi.thumbIsIcon);
    assert.match(cell('asleep').thumb, /\/s\d\/b07\.webp$/);
    assert.equal(cell('fainted').badge, 'bar/badge_bandage');
    assert.match(cell('working-one').thumb, /\/s\d\/b04\.webp$/);
    assert.equal(cell('waiting').badge, 'bar/badge_hand');
    assert.ok(cell('waiting').pulse);
    assert.ok(!cell('waiting', true).pulse, 'no pulse in Calm mode');
    assert.equal(cell('need-water-now').badge, 'bar/badge_drop');
    assert.equal(cell('idle-treats').badge, 'bar/badge_star');
    assert.equal(cell('idle-happy').badge, '');
    assert.equal(cell('idle-happy').label, 'Ginger');
    assert.equal(plain(M.barCell(null, null, null, cfg, false)).thumb, 'ui/egg');
    assert.equal(M.barCell(M.normalize(FIXTURES['idle-happy']), { bar: 'none' }, REG, M.config({ label: 'none' }), false).label, '');
});

test('seenAfter advances only past the celebration the face shows', () => {
    const doc = M.normalize(FIXTURES['hello']);
    const vec = V.priority.vectors.find((v) => v.fixture === 'hello' && v.expect.state === 'celebrate');
    assert.ok(vec);
    const face = P.sharedPriority(doc, vec.now, vec.seen);
    const seen = M.seenAfter(vec.seen, doc, face);
    assert.equal(seen.hello_seq, doc.agent.hello_seq);
    assert.notEqual(P.sharedPriority(doc, vec.now, seen).state, 'celebrate');
    assert.deepEqual(plain(M.seenAfter({}, doc, { state: 'idle' })), { finished_seq: 0, hello_seq: 0, test_at_ms: 0 });
});

test('each care or treat press gets its own key; only an unconfirmed one is reused', () => {
    const food = 'pet.care{"action":"food"}';
    const water = 'pet.care{"action":"water"}';
    // Two presses before any answer: two keys (both applied), never one replayed receipt.
    let c1 = M.retryClaim({}, food, 1000, 1);
    let c2 = M.retryClaim(c1.book, food, 1001, 2);
    assert.match(c1.key, /^bar-[a-z0-9]+-1$/);
    assert.notEqual(c2.key, c1.key, 'a press while the same press is in flight gets a fresh key');
    let c3 = M.retryClaim(c2.book, food, 1002, 3);
    assert.equal(new Set([c1.key, c2.key, c3.key]).size, 3, 'three quick presses, three keys');
    // A confirmed answer forgets its key; a later press is new.
    let book = M.retryRelease(c3.book, c1.key, { ok: true }, 2000);
    assert.equal(book[c1.key], undefined);
    // Food times out, then Water is pressed and answers: Food's record keeps Food's key and
    // signature (Water's answer never overwrites it).
    let f = M.retryClaim({}, food, 1000, 10);
    let w = M.retryClaim(f.book, water, 1100, 11);
    book = M.retryRelease(w.book, f.key, { ok: false, error: { code: 'CLIENT_TIMEOUT' } }, 5000);
    book = M.retryRelease(book, w.key, { ok: true }, 5100);
    assert.deepEqual(plain(book[f.key]), { signature: food, expiresAt: 5000 + M.RETRY_WINDOW_MS, pending: false });
    assert.equal(book[w.key], undefined);
    // Food pressed again inside the window: Food's own key, applied at most once.
    const again = M.retryClaim(book, food, 6000, 12);
    assert.equal(again.key, f.key, 'the unconfirmed press is retried under its own key');
    assert.equal(again.book[f.key].pending, true);
    // ... and a press of the same tile while that retry is in flight is a new press.
    assert.notEqual(M.retryClaim(again.book, food, 6001, 13).key, f.key);
    // Water now is a new press; after the window Food is a new press too.
    assert.notEqual(M.retryClaim(book, water, 6000, 14).key, w.key);
    assert.notEqual(M.retryClaim(book, food, 5001 + M.RETRY_WINDOW_MS, 15).key, f.key, 'after the window it is a new press');
    assert.equal(M.retryClaim(book, food, 5000 + M.RETRY_WINDOW_MS, 16).key, f.key, 'inside the window (inclusive)');
    // A release for an unknown key changes nothing; the book stays bounded.
    assert.deepEqual(plain(M.retryRelease(book, 'bar-x-1', { ok: false }, 1)), plain(book));
    let big = {};
    for (let i = 0; i < 40; i++) {
        const c = M.retryClaim(big, 'treats.give{}', 100 + i, 100 + i);
        big = M.retryRelease(c.book, c.key, { ok: false }, 100 + i);
    }
    assert.ok(Object.keys(big).length <= M.RETRY_BOOK_MAX, 'bounded: ' + Object.keys(big).length);
    assert.deepEqual(plain(M.RETRY_METHODS), ['pet.care', 'treats.give']);
});


test('the Connect card row after Connect: refused, stale and new failures', () => {
    const failed = { connected: false, failed_reason: 'too_old', job: null };
    const running = { connected: false, failed_reason: 'too_old', job: { id: 'job-0123456789abcdef01234567', kind: 'connect', state: 'running' } };
    assert.equal(M.connectRowView(failed, { refused: true }, 0), 'failed', 'a refused Connect fails its rows at once');
    assert.equal(M.connectRowView({ connected: true, failed_reason: null, job: null }, { refused: true }, 0), 'failed');
    const mark = { refused: false, reason: 'too_old', sawJob: false, at: 1000 };
    assert.equal(M.connectRowView(failed, mark, 1500), 'pending', 'the failure from before Connect is not the result');
    assert.equal(M.connectRowView(running, mark, 1500), 'busy');
    assert.equal(M.connectRowView(failed, Object.assign({}, mark, { sawJob: true }), 1500), 'failed', 'after its job: the result');
    assert.equal(M.connectRowView({ connected: false, failed_reason: 'blocked', job: null }, mark, 1500), 'failed', 'a new reason');
    assert.equal(M.connectRowView(failed, mark, 1000 + M.CONNECT_GRACE_MS), 'failed', 'after the grace time');
    assert.equal(M.connectRowView(failed, Object.assign({}, mark, { reason: null }), 1500), 'failed', 'no failure before');
    assert.equal(M.connectRowView({ connected: true, failed_reason: null, job: null }, mark, 1500), 'done');
    assert.equal(M.connectRowView(failed, null, 0), 'failed', 'no mark: the plain state');
});

test('the name check asks for a name when there is none', () => {
    assert.equal(M.nameProblem(''), 'name.type');
    assert.equal(M.nameProblem('   '), 'name.type');
    assert.equal(M.nameProblem(null), 'name.type');
    assert.equal(M.nameProblem('Pip'), '');
    assert.equal(M.nameProblem('x'.repeat(25)), 'err.name_too_long');
    assert.equal(M.nameProblem('a\u0007b'), 'err.name_too_long');
    assert.equal(S['name.type'], 'Type a name');
});

test('maxUnseenSeq and whenKey', () => {
    assert.equal(M.maxUnseenSeq([{ seq: 3, kind: 'evolved' }, { seq: 9 }, { seq: 12, kind: 'other' }]), 9);
    assert.equal(M.maxUnseenSeq(null), -1);
    const now = Date.UTC(2026, 9, 16, 12);
    assert.equal(M.whenKey(now - 1000, now, 0), 'when.now');
    assert.equal(M.whenKey(now - 3600e3, now, 0), 'when.today');
    assert.equal(M.whenKey(now - 86400e3, now, 0), 'when.yesterday');
    assert.equal(M.whenKey(now - 5 * 86400e3, now, 0), 'when.while');
});
