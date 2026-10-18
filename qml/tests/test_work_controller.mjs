import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

// Run the controller's actual JavaScript handlers without a Qt/Quickshell host.
// The fake transport preserves WorkRpc's callback-before-busy-clears ordering.
const source = readFileSync(new URL('../omarchy/WorkController.qml', import.meta.url), 'utf8');
const model = readFileSync(new URL('../omarchy/WorkModel.js', import.meta.url), 'utf8');
function handler(name) {
  const match = source.match(new RegExp(`^    function ${name}\\([^\\n]*\\) \\{[\\s\\S]*?^    \\}`, 'm'));
  assert.ok(match, `controller handler ${name} exists`);
  return match[0];
}
function harness() {
  const Work = vm.createContext({});
  vm.runInContext(model, Work);
  const requests = [];
  const context = vm.createContext({
    Work, busy: false, error: '', state: null, opened: true,
    refreshPending: false, refreshPendingPreserveError: false,
    observeClient() {}, visibleState(next) { return next; },
  });
  context.controller = context;
  context.rpc = {
    request(method, params, callback) {
      assert.equal(context.busy, false);
      context.busy = true;
      requests.push({method, params, callback});
    },
  };
  vm.runInContext(['consume', 'refresh', 'action'].map(handler).join('\n'), context);
  function complete(response) {
    const request = requests.shift();
    assert.ok(request, 'a request must be pending');
    request.callback(response);
    context.busy = false;
    if (context.refreshPending) context.refresh(context.refreshPendingPreserveError);
    return request;
  }
  return {context, requests, complete};
}
const state = status => ({
  readiness: {ready: true, sleeping: false}, provider: {available: true},
  task: {id: 'recovery-fixture', status, review_digest: 'digest', error: ''},
});

test('failed Accept hydrates recovery once and automatic reads retain its feedback', () => {
  const {context: c, requests, complete} = harness();
  c.state = state('review');
  c.action('accept', {task_id: 'recovery-fixture', review_digest: 'digest', acknowledge_unverified: true});
  assert.equal(complete({ok: false, error: {message: 'Destination changed: a.txt'}}).method, 'accept');
  assert.equal(requests.length, 1, 'failed Accept schedules one status read');
  assert.equal(complete({ok: true, result: state('applying')}).method, 'get');
  assert.equal(c.state.task.status, 'applying', 'partial application is immediately labelled recovery');
  assert.equal(c.error, 'Destination changed: a.txt');
  assert.equal(requests.length, 0, 'hydration does not loop');
  for (let i = 0; i < 2; i++) {
    c.refresh(true);
    complete({ok: true, result: state('applying')});
    assert.equal(c.error, 'Destination changed: a.txt', 'automatic reads cannot erase the action failure');
  }
  c.action('accept', {task_id: 'recovery-fixture', review_digest: 'digest', acknowledge_unverified: true});
  complete({ok: true, result: state('accepted')});
  assert.equal(c.error, '', 'successful explicit retry clears the error');
  assert.equal(c.state.task.status, 'accepted');
  assert.equal(requests.length, 0);
});

test('explicit Refresh clears old feedback, including when coalesced with automatic reads', () => {
  const {context: c, requests, complete} = harness();
  c.error = 'Earlier action failed';
  c.refresh(true);
  c.refresh(true);
  c.refresh();
  c.refresh(true);
  complete({ok: true, result: state('applying')});
  assert.equal(c.error, 'Earlier action failed', 'in-flight automatic read retains feedback');
  assert.equal(requests.length, 1, 'queued reads coalesce');
  complete({ok: true, result: state('applying')});
  assert.equal(c.error, '', 'the coalesced explicit request still clears feedback');
  assert.equal(requests.length, 0);
});

test('awake applying recovery does not poll; execution and visible sleep still do', () => {
  const {context: c} = harness();
  const timer = source.slice(source.indexOf('property Timer pollTimer:'));
  const expression = timer.match(/^        running: (.*)$/m)[1];
  const polling = (status, sleeping = false, opened = true) => {
    c.state = state(status); c.state.readiness.sleeping = sleeping;
    c.task = c.state.task; c.opened = opened;
    return vm.runInContext(expression, c);
  };
  assert.equal(polling('applying'), false);
  assert.equal(polling('review'), false);
  assert.equal(polling('accepted'), false);
  assert.equal(polling('running'), true);
  assert.equal(polling('preparing'), true);
  assert.equal(polling('applying', true), true, 'sleep countdown still updates');
  assert.equal(polling('running', false, false), false);
  assert.match(timer, /onTriggered: controller\.refresh\(true\)/);
});
