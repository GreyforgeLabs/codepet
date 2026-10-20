// Differential oracle for the native pet's Rust port of the QML face rules.
// Prints, for a corpus built from the v0.3 bar fixtures, the JavaScript reference results
// (qml/shared/petstate.js and OverlayFace.js). The Rust test replays the same corpus.
import { readFileSync, readdirSync } from 'node:fs';
import vm from 'node:vm';

const root = new URL('../../../', import.meta.url);
function load(rel) {
    const ctx = vm.createContext({});
    vm.runInContext(readFileSync(new URL(rel, root), 'utf8').replace(/^\.pragma library\n/, ''), ctx);
    return ctx;
}
const P = load('qml/shared/petstate.js');
const O = load('qml/shared/OverlayFace.js');
const dir = new URL('docs/v0.3/contracts/fixtures/bar-v2/', root);
const base = 1792123900000;
const nows = [base - 60000, base, base + 1000, base + 2500, base + 31000, base + 130000];
const works = [null, { task: { status: 'running', kind: 'bug' } }, { task: { status: 'review', kind: 'research' } },
    { task: { status: 'scouted', has_finding: true, source_kind: 'scout' } }, { task: { status: 'scouted', has_finding: false } },
    { task: { status: 'preparing', kind: 'build', source_kind: 'scout_fix' } }, { task: null }];
const out = [];
for (const name of readdirSync(dir).filter(n => n.endsWith('.json') && !n.startsWith('clients-')).sort()) {
    const fixture = JSON.parse(readFileSync(new URL(name, dir), 'utf8'));
    const docs = [fixture];
    for (const hunger of [500, 2500, 4500, 7000]) {
        const d = JSON.parse(JSON.stringify(fixture));
        if (d.meters) d.meters.hunger = hunger;
        docs.push(d);
    }
    docs.forEach((doc, variant) => {
        const start = O.seenAtStart(doc);
        for (const now of nows) for (const seen of [{ finished_seq: 0, hello_seq: 0, test_at_ms: 0 }, start.seen]) {
            const shared = P.sharedPriority(doc, now, seen);
            const current = P.currentPriority(doc, now, seen);
            const faces = works.map(w => O.withWork(current, w));
            out.push({
                name, variant, now, seen, shared, current,
                without_growth_len: (O.withoutGrowth(doc, start.grewSeq).unseen_events || []).length,
                start, faces,
                icons: faces.map(f => ['floor', 'aquatic', 'robot'].map(s => O.bubbleIcons(f.bubble, s, f.state === 'waiting'))),
                labels: faces.map(f => O.observerLabel(f, P.CLIENT_NAMES)),
                key: O.celebrationKey(current.celebrate),
                marked: O.markSeen(seen, current.celebrate),
            });
        }
    });
}
process.stdout.write(JSON.stringify({ works, cases: out }));
