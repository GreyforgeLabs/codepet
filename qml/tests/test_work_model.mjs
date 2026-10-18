import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const M = vm.createContext({});
vm.runInContext(readFileSync(new URL('../omarchy/WorkModel.js', import.meta.url), 'utf8'), M);
const ready = { readiness: { ready: true, energy: 80 }, provider: { available: true }, task: null };
test('readiness, provider, folder, task and request gate starts', () => {
  assert.equal(M.canStart(ready, false, '/project', 'Fix the bug'), true);
  for (const [state, busy, folder, prompt] of [[null, false, '/p', 'fix'], [ready, true, '/p', 'fix'], [ready, false, 'p', 'fix'], [ready, false, '/p', ' '], [{...ready, task: {status:'review'}}, false, '/p', 'fix'], [{...ready, readiness:{ready:false}}, false, '/p', 'fix'], [{...ready, provider:{available:false}}, false, '/p', 'fix']]) assert.equal(M.canStart(state,busy,folder,prompt),false);
});
test('only bound review acknowledgement allows accept, including interrupted apply recovery', () => {
  for (const status of ['review','applying']) assert.equal(M.canAccept({status,review_digest:'hash'},false,true),true);
  for (const task of [null,{status:'failed',review_digest:'hash'},{status:'review'},{status:'review',review_digest:''}]) assert.equal(M.canAccept(task,false,true),false);
  assert.equal(M.canAccept({status:'review',review_digest:'hash'},false,false),false);
  assert.equal(M.canAccept({status:'review',review_digest:'hash'},true,true),false);
});
test('persisted application recovery remains active but is not executing', () => {
  for (const status of ['preparing','running']) assert.equal(M.executing({status}),true);
  assert.equal(M.active({status:'applying'}),true);
  for (const task of [null,{status:'applying'},{status:'review'},{status:'accepted'},{status:'failed'}])
    assert.equal(M.executing(task),false);
});
test('folder dialog accepts only local file URLs and preserves literal special characters', () => {
  assert.equal(M.filePath('file:///a%20b/%24%28literal%29'),' /a b/$(literal)'.trim());
  assert.equal(M.filePath('file://remote/path'),'');
  assert.equal(M.filePath('file:///bad%ZZ'),'');
  assert.equal(M.kind('unknown'),'build');
});

test('scout one-shot eligibility respects readiness, provider, setting and active slot', () => {
  const state = {...ready, scout:{enabled:true,next_due_ms:0}};
  assert.equal(M.scoutEligible(state),true);
  for (const invalid of [null, {...state,scout:null}, {...state,scout:{enabled:false,next_due_ms:0}}, {...state,readiness:{ready:false}}, {...state,provider:{available:false}}, {...state,task:{status:'running'}}, {...state,task:{status:'scouted',has_finding:true}}]) assert.equal(M.scoutEligible(invalid),false);
  assert.equal(M.scoutEligible({...state,task:{status:'scouted',has_finding:false}}),true);
  assert.equal(M.badge({status:'scouted',has_finding:false}),false);
  assert.equal(M.badge({status:'scouted',has_finding:true}),true);
  assert.equal(M.canAccept({status:'review',source_kind:'scout',review_digest:'hash'},false,true),false);
});

test('previous results distinguish repairs from checks and label reliable start age', () => {
  const now = 1800000000000;
  const task = {status:'failed',kind:'bug',source_kind:'scout_fix',started_at_ms:now-3*86400000};
  assert.equal(M.previousTitle(task),'Previous repair · Couldn’t finish');
  assert.equal(M.previousTitle({...task,source_kind:'scout'}),'Previous check · Couldn’t finish');
  assert.equal(M.startedText(task,now),'Started 3 days ago');
  assert.equal(M.startedText({...task,started_at_ms:undefined},now),'Start time unavailable');
  assert.equal(M.startedText({...task,started_at_ms:now+1},now),'Start time unavailable');
  for (const status of ['running','preparing','review','applying']) assert.equal(M.finished({...task,status}),false);
  assert.equal(M.finished({status:'scouted',has_finding:true}),false);
});

test('advanced history uses an absolute start date rather than a frozen relative clock', () => {
  const at = 1800000000000;
  assert.equal(M.startedDate({started_at_ms:at}), 'Started ' + new Date(at).toLocaleString());
  for (const invalid of [undefined,null,0,-1,Infinity,NaN,8640000000000001,1.5])
    assert.equal(M.startedDate({started_at_ms:invalid}), 'Start time unavailable');
});

test('selected executor and immutable job origin stay distinct, including historical Codex jobs', () => {
  const state = {provider:{id:'opencode',name:'OpenCode',available:false,reason:'Sign in first'}};
  assert.equal(M.clientId(state),'opencode');
  assert.equal(M.clientName(state),'OpenCode');
  assert.equal(M.taskClientName({client_id:'claude-code',client_name:'Claude Code'}),'Claude Code');
  assert.equal(M.taskClientName({id:'historical-job'}),'Codex');
  assert.equal(M.taskClientName({client_id:'future-client',client_name:'Future Client'}),'Future Client');
  assert.equal(M.clientId(null),'');
  assert.equal(M.clientName(null),'Checking…');
});
