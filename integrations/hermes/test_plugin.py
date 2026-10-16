#!/usr/bin/env python3
"""Offline tests of the Hermes plugin (SPEC-v0.3 §5.6, §9.1 "Hermes plugin").

The template is rendered with a stand-in shim (a sh script that appends its stdin to a
log) and loaded like Hermes loads a plugin: `register(ctx)` with a fake context. Covered:
platform memory per session, the approval `surface` filter, approval keys, clarify, tool
counting, the on_session_end mapping, reaping of finished children, return values (None),
no logging and no repr of kwargs, spawn failures, payloads valid against
plugin-payload.schema.json. No Hermes runs.

Run: python3 integrations/hermes/test_plugin.py
"""
import importlib.util
import json
import logging
import os
import shutil
import tempfile
import time
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE = (HERE / "__init__.py.in").read_text()
SCHEMA = json.loads((HERE / "../../docs/v0.3/contracts/plugin-payload.schema.json").read_text())
LOGGING_SHIM = '#!/bin/sh\n[ "$1" = hermes ] || exit 3\ncat >> "@LOG@"\n'


class Ctx:
    def __init__(self):
        self.hooks = {}

    def register_hook(self, name, cb):
        self.hooks[name] = cb


class BadRepr:
    def __repr__(self):
        raise AssertionError("kwargs must never be repr()'d")

    __str__ = __repr__


class Plugin(unittest.TestCase):
    def load(self, shim_body=LOGGING_SHIM, mode=0o755):
        tmp = tempfile.mkdtemp(prefix="codepet-hermes-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        shim = Path(tmp) / "codepet-hook"
        self.log = Path(tmp) / "events.log"
        if shim_body is not None:
            shim.write_text(shim_body.replace("@LOG@", str(self.log)))
            shim.chmod(mode)
        src = Path(tmp) / "plugin.py"
        src.write_text(TEMPLATE.replace("@CODEPET_SHIM@", str(shim)))
        spec = importlib.util.spec_from_file_location(f"codepet_plugin_{id(self)}", src)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        ctx = Ctx()
        mod.register(ctx)
        self.mod = mod
        return ctx.hooks

    def events(self, want, timeout=3.0):
        deadline = time.time() + timeout
        while True:
            lines = self.log.read_text().splitlines() if self.log.exists() else []
            if len(lines) >= want or time.time() > deadline:
                return [json.loads(l) for l in lines]
            time.sleep(0.02)

    def check(self, p):
        props = SCHEMA["properties"]
        self.assertTrue(set(p) <= set(props), p)
        self.assertEqual(p["codepet_v"], 1)
        self.assertIn(p["event"], props["event"]["enum"])
        self.assertEqual(("session_id" in p) + ("approval_key" in p), 1, p)
        if "approval_key" in p:
            self.assertIn(p["event"], ("waiting", "activity"))
        self.assertNotEqual(p.get("interactive"), False, "Hermes never sends interactive:false")
        self.assertLess(len(json.dumps(p, separators=(",", ":"))) + 1, 512)

    def test_registers_the_nine_hooks_and_returns_none(self):
        hooks = self.load()
        self.assertEqual(sorted(hooks), sorted([
            "on_session_start", "pre_llm_call", "pre_tool_call", "post_tool_call",
            "pre_approval_request", "post_approval_response", "on_session_end",
            "agent_loop_stopped", "on_session_finalize"]))
        for name, cb in hooks.items():
            self.assertIsNone(cb(session_id="s", platform="cli", turn_id="t", user_message="secret"), name)

    def test_a_cli_turn_with_clarify_tools_and_its_end(self):
        hooks = self.load()
        handler = logging.Handler()
        records = []
        handler.emit = records.append
        logging.getLogger().addHandler(handler)
        self.addCleanup(logging.getLogger().removeHandler, handler)
        secret = BadRepr()
        hooks["on_session_start"](session_id="s1", model="m", platform="cli")
        hooks["pre_llm_call"](session_id="s1", turn_id="t1", platform="cli", user_message=secret,
                              conversation_history=[secret])
        hooks["post_tool_call"](session_id="s1", tool_name="terminal", result=secret)
        hooks["post_tool_call"](session_id="s1", tool_name="write_file", args=secret)
        hooks["pre_tool_call"](session_id="s1", tool_name="clarify", args=secret)
        hooks["post_tool_call"](session_id="s1", tool_name="clarify")
        hooks["on_session_end"](session_id="s1", turn_id="t1", completed=True, failed=False,
                                interrupted=False, turn_exit_reason="x", model="m", platform="cli")
        hooks["pre_llm_call"](session_id="s1", turn_id="t2", platform="cli")
        hooks["on_session_end"](session_id="s1", turn_id="t2", completed=False, failed=True, interrupted=False)
        hooks["pre_llm_call"](session_id="s1", turn_id="t3", platform="cli")
        hooks["on_session_end"](session_id="s1", turn_id="t3", completed=False, failed=False, interrupted=True)
        hooks["on_session_finalize"](session_id="s1", platform="cli")
        got = self.events(10)
        time.sleep(0.2)
        got = self.events(10)
        self.assertEqual(sorted(p["event"] for p in got),
                         sorted(["session_start", "turn_start", "waiting", "activity", "turn_done",
                                 "turn_start", "turn_failed", "turn_start", "turn_cancelled", "session_end"]))
        for p in got:
            self.check(p)
            self.assertEqual(p["session_id"], "s1")
        done = next(p for p in got if p["event"] == "turn_done")
        self.assertEqual((done["turn_id"], done["tools"]), ("t1", 2))
        self.assertEqual(records, [], "the plugin never logs")
        self.assertNotIn("secret", self.log.read_text())

    def test_platform_memory_drops_other_platforms(self):
        hooks = self.load()
        hooks["on_session_start"](session_id="g1", platform="telegram")
        hooks["pre_llm_call"](session_id="g1", turn_id="t", platform="telegram")
        hooks["on_session_end"](session_id="g1", turn_id="t", completed=True)
        hooks["on_session_start"](session_id="u1", platform=None)
        hooks["pre_llm_call"](session_id="u1", turn_id="t")
        hooks["pre_llm_call"](session_id="tui1", turn_id="t", platform="tui")
        hooks["on_session_end"](session_id="tui1", turn_id="t", completed=True)  # platform remembered
        hooks["agent_loop_stopped"](session_key="k", platform="tui")  # no session id: dropped
        got = self.events(2)
        time.sleep(0.2)
        got = self.events(2)
        self.assertEqual(sorted(p["event"] for p in got), ["turn_done", "turn_start"])
        self.assertTrue(all(p["session_id"] == "tui1" for p in got))

    def test_approval_surfaces_and_keys(self):
        hooks = self.load()
        for surface in ("cli", "transport:telegram", "smart", "gateway", None):
            hooks["pre_approval_request"](surface=surface, session_key=f"k-{surface}", command=BadRepr(),
                                          description="rm -rf secret", turn_id="t")
        hooks["post_approval_response"](surface="cli", session_key="k-cli", choice="once")
        hooks["pre_approval_request"](surface="cli", session_key="../bad key")
        got = self.events(3)
        time.sleep(0.2)
        got = self.events(3)
        self.assertEqual(len(got), 3, got)
        for p in got:
            self.check(p)
        self.assertEqual(sorted((p["event"], p["approval_key"]) for p in got),
                         [("activity", "k-cli"), ("waiting", "k-cli"), ("waiting", "k-transport:telegram")])
        self.assertNotIn("secret", self.log.read_text())

    def test_children_are_reaped_and_bounded(self):
        hooks = self.load('#!/bin/sh\nsleep 0.3\ncat >/dev/null\n')
        for i in range(40):
            hooks["on_session_start"](session_id=f"s{i}", platform="cli")
        self.assertLessEqual(len(self.mod._live), 16)
        time.sleep(0.6)
        hooks["on_session_start"](session_id="late", platform="cli")
        self.assertLessEqual(len(self.mod._live), 1, "finished children are reaped on the next callback")
        for _ in range(300):
            hooks["on_session_start"](session_id="x", platform="cli")
            if len(self.mod._platform) > 256:
                break
        for i in range(300):
            self.mod._session({"session_id": f"m{i}"}, "cli")
        self.assertLessEqual(len(self.mod._platform), 256)

    def test_a_missing_or_broken_shim_never_raises(self):
        for body, mode in ((None, 0o755), (LOGGING_SHIM, 0o644), ("#!/bin/sh\nexit 0\n", 0o755)):
            hooks = self.load(body, mode)
            for name, cb in hooks.items():
                self.assertIsNone(cb(session_id="s", platform="cli", turn_id="t", completed=True,
                                     surface="cli", session_key="k", tool_name="clarify"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
