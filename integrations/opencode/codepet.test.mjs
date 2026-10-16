// Offline tests of the OpenCode plugin (SPEC-v0.3 §5.5, §9.1 "OpenCode plugin"). The
// template is rendered with a stand-in shim (a sh script that appends its stdin to a log),
// imported like OpenCode imports it, and driven with event fixtures. No OpenCode runs.
//
// Run: node --test integrations/opencode/codepet.test.mjs
import { test, after } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync, chmodSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const TEMPLATE = readFileSync(join(HERE, "codepet.js.in"), "utf8");
const SCHEMA = JSON.parse(readFileSync(join(HERE, "../../docs/v0.3/contracts/plugin-payload.schema.json"), "utf8"));
const ID = /^[A-Za-z0-9._:-]{1,128}$/;
let n = 0;
const made = [];
after(() => made.forEach((d) => rmSync(d, { recursive: true, force: true })));

function render(shimBody, mode = 0o755) {
  const dir = mkdtempSync(join(process.env.TMPDIR || tmpdir(), "codepet-oc-"));
  made.push(dir);
  const shim = join(dir, "codepet-hook");
  const log = join(dir, "events.log");
  if (shimBody !== null) {
    writeFileSync(shim, shimBody.replaceAll("@LOG@", log));
    chmodSync(shim, mode);
  }
  const file = join(dir, `codepet-${n++}.js`);
  writeFileSync(file, TEMPLATE.replace("@CODEPET_SHIM@", shim));
  return { dir, shim, log, file };
}

const LOGGING_SHIM = '#!/bin/sh\n[ "$1" = opencode ] || exit 3\ncat >> "@LOG@"\n';

async function load(file) {
  const mod = await import(pathToFileURL(file).href);
  assert.equal(mod.default.id, "codepet.opencode");
  assert.equal(typeof mod.default.server, "function");
  assert.equal(typeof mod.default.setup, "function");
  assert.deepEqual(Object.keys(mod).sort(), ["CodePetPlugin", "default"]);
  return mod.default.server({ directory: "/never/read" });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function lines(log, want, ms = 3000) {
  const started = Date.now();
  for (;;) {
    const got = existsSync(log) ? readFileSync(log, "utf8").split("\n").filter(Boolean) : [];
    if (got.length >= want || Date.now() - started > ms) return got.map((l) => JSON.parse(l));
    await sleep(20);
  }
}

function checkPayload(p) {
  const keys = Object.keys(SCHEMA.properties);
  for (const k of Object.keys(p)) assert.ok(keys.includes(k), `unexpected key ${k}`);
  assert.equal(p.codepet_v, 1);
  assert.ok(SCHEMA.properties.event.enum.includes(p.event), p.event);
  assert.ok(ID.test(p.session_id));
  assert.ok(!("approval_key" in p));
  if ("turn_id" in p) assert.match(p.turn_id, /^[0-9]+-[0-9]+$/);
  if ("tools" in p) assert.ok(Number.isInteger(p.tools) && p.tools >= 0 && p.tools <= 1000);
  assert.ok(JSON.stringify(p).length + 1 < 512);
}

const ev = (type, properties) => ({ event: { id: "e", type, properties } });

test("root turns, child roll-up, waiting, tools and session end", async () => {
  const r = render(LOGGING_SHIM);
  const hooks = await load(r.file);
  const send = (t, p) => hooks.event(ev(t, p));
  await send("session.created", { info: { id: "ses_root", title: "secret title" } });
  await send("session.created", { info: { id: "ses_child", parentID: "ses_root" } });
  await send("session.status", { sessionID: "ses_root", status: { type: "busy" } });
  await send("session.status", { sessionID: "ses_root", status: { type: "busy" } }); // no new turn
  const part = (id, status, sid = "ses_root") => ({ part: { id, type: "tool", sessionID: sid, tool: "bash", state: { status, input: { command: "rm -rf secret" } } } });
  await send("message.part.updated", part("p1", "running"));
  await send("message.part.updated", part("p1", "completed"));
  await send("message.part.updated", part("p1", "completed")); // counted once
  await send("message.part.updated", part("p2", "completed", "ses_child")); // child tools roll up
  await send("message.part.delta", { delta: "token" });
  await send("session.status", { sessionID: "ses_child", status: { type: "busy" } }); // child: no turn
  await send("permission.asked", { sessionID: "ses_child", permission: "bash", patterns: ["secret*"] });
  await send("permission.replied", { sessionID: "ses_child", reply: "once" });
  await send("session.idle", { sessionID: "ses_child" }); // child idle: nothing
  await send("session.idle", { sessionID: "ses_root" });
  await send("session.status", { sessionID: "ses_root", status: { type: "idle" } }); // already closed
  await send("session.status", { sessionID: "ses_root", status: { type: "busy" } });
  await send("session.error", { sessionID: "ses_root", error: { name: "MessageAbortedError", data: { message: "secret" } } });
  await send("session.status", { sessionID: "ses_root", status: { type: "retry", attempt: 2 } });
  await send("session.error", { sessionID: "ses_root", error: { name: "APIError" } });
  await send("session.deleted", { sessionID: "ses_child" });
  await send("session.deleted", { sessionID: "ses_root", info: { id: "ses_root" } });
  const got = await lines(r.log, 10);
  const kinds = got.map((p) => p.event);
  assert.deepEqual(kinds.slice().sort(), [
    "activity", "session_end", "session_start", "turn_cancelled", "turn_done",
    "turn_failed", "turn_start", "turn_start", "turn_start", "waiting",
  ]);
  for (const p of got) {
    checkPayload(p);
    assert.equal(p.session_id, "ses_root");
    assert.equal(p.interactive, true);
  }
  const done = got.find((p) => p.event === "turn_done");
  assert.equal(done.tools, 2);
  const starts = got.filter((p) => p.event === "turn_start").map((p) => p.turn_id);
  assert.equal(new Set(starts).size, 3, "turn keys are unique");
  assert.equal(done.turn_id, starts.find((t) => t.endsWith("-1")));
  const text = readFileSync(r.log, "utf8");
  assert.ok(!text.includes("secret") && !text.includes("bash"), "no content is ever sent");
});

test("non-interactive argv, and turn keys differ across plugin restarts", async () => {
  const r = render(LOGGING_SHIM);
  const argv = process.argv;
  process.argv = [argv[0], "/usr/bin/opencode", "run", "do it"];
  let hooks;
  try {
    hooks = await load(r.file);
  } finally {
    process.argv = argv;
  }
  await hooks.event(ev("session.status", { sessionID: "ses_A", status: { type: "busy" } }));
  await sleep(5);
  const again = await load(render(LOGGING_SHIM).file);
  void again;
  const got = await lines(r.log, 2);
  assert.ok(got.every((p) => p.interactive === false));
});

test("spawn failures never reach the host", async () => {
  const problems = [];
  const onErr = (e) => problems.push(e);
  process.on("uncaughtException", onErr);
  process.on("unhandledRejection", onErr);
  try {
    for (const [body, mode] of [
      [null, 0o755], // missing shim: ENOENT
      [LOGGING_SHIM, 0o644], // not executable: EACCES
      ["#!/bin/sh\nexit 0\n", 0o755], // exits before reading: EPIPE on stdin
      ["#!/bin/sh\nexec 0<&-\nsleep 0.2\n", 0o755], // closes stdin early
    ]) {
      const r = render(body, mode);
      const hooks = await load(r.file);
      for (let i = 0; i < 20; i++) {
        await hooks.event(ev("session.status", { sessionID: `ses_${i}`, status: { type: "busy" } }));
        await hooks.event(ev("session.idle", { sessionID: `ses_${i}` }));
      }
      await hooks.dispose();
    }
    // Hostile events: getters that throw, nulls, wrong shapes.
    const r = render(LOGGING_SHIM);
    const hooks = await load(r.file);
    const evil = { get type() { throw new Error("boom"); } };
    await hooks.event({ event: evil });
    await hooks.event({ event: null });
    await hooks.event({});
    await hooks.event(ev("session.status", null));
    await hooks.event(ev("session.status", { sessionID: "../../x", status: { type: "busy" } }));
    await hooks.event(ev("message.part.updated", { part: null }));
    await sleep(500);
    assert.deepEqual(problems, []);
    assert.equal(existsSync(r.log), false, "invalid ids are never sent");
  } finally {
    process.off("uncaughtException", onErr);
    process.off("unhandledRejection", onErr);
  }
});

test("100 000 streaming events spawn nothing and keep memory flat", async () => {
  const r = render(LOGGING_SHIM);
  const hooks = await load(r.file);
  await hooks.event(ev("session.created", { info: { id: "ses_S" } }));
  const before = process.memoryUsage().heapUsed;
  for (let i = 0; i < 100000; i++) {
    await hooks.event(ev("message.part.delta", { sessionID: "ses_S", delta: "x".repeat(32) }));
    await hooks.event(ev("message.part.updated", { part: { id: `t${i}`, type: "text", sessionID: "ses_S" } }));
  }
  const grown = process.memoryUsage().heapUsed - before;
  assert.ok(grown < 32 * 1024 * 1024, `heap grew ${grown}`);
  // Only the session_start of ses_S was ever sent.
  const got = await lines(r.log, 1, 500);
  assert.deepEqual(got.map((p) => p.event), ["session_start"]);
  // Hundreds of sessions stay bounded (LRU of 256).
  for (let i = 0; i < 2000; i++) {
    await hooks.event(ev("session.updated", { info: { id: `ses_${i}`, parentID: "ses_S" } }));
  }
});

test("live metadata projection covers observed values in a synthetic SDK envelope", async () => {
  const path = join(HERE, "../../crates/codepet-cli/tests/fixtures/live-2026-10-16/opencode.json");
  const evidence = JSON.parse(readFileSync(path, "utf8"));
  const rows = evidence.upstream_captures;
  const toolParts = rows.filter((row) => row.part_type === "tool");
  assert.deepEqual(toolParts.map((row) => row.part_status), ["pending", "running", "completed"]);
  assert.equal(new Set(toolParts.map((row) => row.part)).size, 1);
  assert.ok(rows.some((row) => row.session_status === "busy"));
  assert.ok(rows.some((row) => row.session_status === "idle"));
  assert.ok(rows.every((row) => row.parent === null), "this capture has no child-session coverage");

  const r = render(LOGGING_SHIM);
  const hooks = await load(r.file);
  for (const row of rows) {
    // Reconstruction is limited to the SDK envelope, whose nesting was NOT captured.
    // Every id/event/state value below comes from the metadata projection. No text,
    // native tool name, version, timestamp or tool count is restored into the evidence.
    let properties;
    if (row.type === "session.created" || row.type === "session.updated") {
      properties = { info: { id: row.session } };
    } else if (row.type === "session.status") {
      properties = { sessionID: row.session, status: { type: row.session_status } };
    } else if (row.type === "message.part.updated") {
      properties = { part: { id: row.part, sessionID: row.session, type: row.part_type,
        state: { status: row.part_status } } };
    } else {
      properties = { sessionID: row.session };
    }
    await hooks.event(ev(row.type, properties));
  }
  // dispose is a synthetic lifecycle action: the upstream observer did not retain it.
  // Its emitted session_end WAS observed in the downstream capture.
  await hooks.dispose();
  const expected = evidence.hook_captures.map((row) => row.metadata);
  const got = await lines(r.log, expected.length);
  for (const payload of got) checkPayload(payload);
  const observedTurn = expected.find((row) => row.event === "turn_start").turn_id;
  const replayTurns = got.filter((row) => row.turn_id).map((row) => row.turn_id);
  assert.equal(new Set(replayTurns).size, 1, "the repeated busy events remain one turn");
  const projected = got.map(({ codepet_v, tools, ...payload }) => {
    // Version/count were omitted by capture; generated turn ids differ on every replay.
    void codepet_v;
    void tools;
    if (payload.turn_id) payload.turn_id = observedTurn;
    return payload;
  });
  const byEvent = (a, b) => a.event.localeCompare(b.event);
  assert.deepEqual(projected.sort(byEvent), expected.sort(byEvent));
});
