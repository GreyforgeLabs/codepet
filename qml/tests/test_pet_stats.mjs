import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const M = vm.createContext({});
vm.runInContext(readFileSync(new URL('../../omarchy-plugin/CodePetBar.js', import.meta.url), 'utf8')
    .replace(/^\.pragma library$/m, ''), M);
const A = vm.createContext({});
vm.runInContext(readFileSync(new URL('../shared/PetAge.js', import.meta.url), 'utf8').replace(/^\.pragma library$/m, ''), A);
assert.equal(readFileSync(new URL('../shared/PetAge.js', import.meta.url), 'utf8'), readFileSync(new URL('../../omarchy-plugin/PetAge.js', import.meta.url), 'utf8'));
const plain = value => JSON.parse(JSON.stringify(value));

test('care meters show stored percentages, including full, low, and unknown', () => {
    const doc = { meters: { hunger: 7250, thirst: 9999, mental_health: 0, hygiene: 10000 } };
    assert.deepEqual(plain(M.careMeter(doc, 'food')), { known: true, percent: 72, fraction: 0.725 });
    assert.equal(M.careMeter(doc, 'water').percent, 99, 'almost full must not claim full');
    assert.equal(M.careMeter(doc, 'play').percent, 0);
    assert.equal(M.careMeter(doc, 'wash').percent, 100);
    for (const value of [null, '6000', NaN, Infinity, -1, 10001])
        assert.equal(M.careMeter({ meters: { hunger: value } }, 'food').known, false);
    assert.equal(M.careMeter(null, 'water').known, false);
    assert.equal(M.careMeter(doc, 'unknown').known, false);
});

test('XP is exact and evolution progress uses the current stage floor', () => {
    assert.deepEqual(plain(M.petProgress({ level: 1, stage: 1, xp: 30, next_stage_xp: 80 })),
        { known: true, level: 1, xp: 30, evolved: false, remaining: 50, fraction: 0.375 });
    const middle = M.petProgress({ level: 3, stage: 2, xp: 240, next_stage_xp: 400 });
    assert.equal(middle.xp, 240);
    assert.equal(middle.remaining, 160);
    assert.equal(middle.fraction, 0.5);
    const final = M.petProgress({ level: 5, stage: 3, xp: 1234, next_stage_xp: null });
    assert.equal(final.xp, 1234, 'total XP remains visible after final evolution');
    assert.equal(final.evolved, true);
    assert.equal(final.fraction, 1);
    for (const life of [null, {}, { level: 1, stage: 1, xp: '30' }, { level: 1, stage: 1, xp: -1 }])
        assert.equal(M.petProgress(life).known, false);
});

test('stat labels allow numbers, percentages, XP and level without relaxing other copy', () => {
    for (const label of ['Level 3', '240 XP', '72%', '160 XP to evolve']) {
        assert.deepEqual(plain(M.kidTextProblems(label, true)), []);
        assert.notEqual(M.kidTextProblems(label).length, 0);
    }
    assert.notEqual(M.kidTextProblems('provider token 42', true).length, 0);
});

test('work export has its own schema and preserves numeric readiness', () => {
    const doc = {readiness:{ready:true,energy:67},provider:{available:true},task:{status:'running',kind:'bug'}};
    assert.equal(M.parse(JSON.stringify(doc)),null,'work is not a bar-state document');
    assert.deepEqual(plain(M.parseWork(JSON.stringify(doc))),doc);
    for (const readiness of [null,{}, {ready:true,energy:'67'},{ready:true,energy:-1},{ready:true,energy:101}])
        assert.equal(M.parseWork(JSON.stringify({...doc,readiness})),null);
});

test('age uses the current life birth time with minute/hour/day boundaries and safe clock skew', () => {
    const born = 1700000000000;
    for (const [elapsed, value, unit] of [[0,0,'minutes'],[59999,0,'minutes'],[60000,1,'minute'],[120000,2,'minutes'],[3600000,1,'hour'],[7200000,2,'hours'],[86400000,1,'day'],[172800000,2,'days'],[-3600000,0,'minutes']])
        assert.deepEqual(plain(A.age({adopted_at_ms:born},born+elapsed)),{known:true,value,unit,born});
    for (const invalid of [undefined,null,0,-1,'1700000000000',NaN,Infinity,1e30,123.5])
        assert.equal(A.age({adopted_at_ms:invalid},born).known,false);
    assert.equal(A.age(null,born).known,false);
    assert.equal(A.age({adopted_at_ms:born},NaN).known,false);
    assert.equal(A.age({adopted_at_ms:born+86400000},born+172800000).value,1,'replacement age belongs to its own life');
});
