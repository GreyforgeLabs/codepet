import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const M=vm.createContext({});
vm.runInContext(readFileSync(new URL('../../omarchy-plugin/UpdateModel.js',import.meta.url),'utf8').replace(/^\.pragma library$/m,''),M);
const state={schema:'codepet.update.v1',current:'0.4.0',latest:'0.4.1',state:'available',message:'A new version is ready.',checked_at:123};
test('helper display response is schema-checked and bounded',()=>{
  assert.equal(M.parse(JSON.stringify(state)).latest,'0.4.1');
  for (const bad of ['bad','[]',JSON.stringify({...state,schema:'unknown'}),JSON.stringify({...state,current:7}),JSON.stringify({...state,latest:'<b>0.4.1</b>'}),JSON.stringify({...state,state:'arbitrary'}),JSON.stringify({...state,message:'x'.repeat(4097)}),JSON.stringify({...state,checked_at:-1})]) assert.equal(M.parse(bad),null);
});
test('only known available release enables install; busy phases poll and terminal ones stop',()=>{
  assert.equal(M.canInstall(state,false),true);
  assert.equal(M.canInstall(state,true),false);
  for (const phase of ['idle','checking','current','downloading','verifying','installing','succeeded','error']) assert.equal(M.canInstall({...state,state:phase},false),false);
  for (const phase of ['checking','downloading','verifying','installing']) assert.equal(M.active({...state,state:phase}),true);
  for (const phase of ['idle','available','current','succeeded','error']) assert.equal(M.active({...state,state:phase}),false);
});
test('pending restart is independent of release comparison and only explicit restart starts it',()=>{
  for (const phase of ['available','current','succeeded','error']) {
    const pending={...state,state:phase,restart_required:true,restart_state:'idle'};
    assert.equal(M.canRestart(M.parse(JSON.stringify(pending)),false),true);
    assert.equal(M.canRestart(pending,true),false);
    assert.equal(M.active(pending),false);
  }
  assert.equal(M.canRestart({...state,restart_required:false},false),false);
  const running={...state,restart_required:true,restart_state:'restarting'};
  assert.equal(M.active(running),true);
  assert.equal(M.canRestart(running,false),false);
  assert.equal(M.canInstall(running,false),false);
  for (const bad of [{restart_required:'true'},{restart_state:'random'},{restart_message:42}])
    assert.equal(M.parse(JSON.stringify({...state,...bad})),null);
});
