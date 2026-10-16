# Live metadata evidence, October 16, 2026

These are projections of the authorized tiny interactive turns, not full raw
client payloads. The original synthetic fixture directories remain intact.
Each JSON records its source capture file's SHA-256 for provenance. All captured
identifier hashes were replaced with new deterministic `qa-<client>-NNN` values,
preserving equality only within that file. No original identifiers, identifier
hashes, prompts, replies, tool arguments/results, paths or credential values are
included. Source capture digests are artifact provenance, not session identities.

| Client | Observed result | Captured coverage |
|---|---|---|
| Claude Code 2.1.294 | Subscription weekly limit; no completed model turn | SessionStart, UserPromptSubmit, StopFailure `rate_limit`, SessionEnd; hook ENTRYPOINT `cli` |
| Codex 0.163.0 | One completed turn | UserPromptSubmit, PostToolUse `Bash` with tool-use id, Stop, SessionEnd |
| OpenCode 1.19.0 | One completed turn on free Muse route | Actual plugin emitted start/done/end; upstream event facts include one tool part progressing pending/running/completed and busy/idle |
| Hermes 0.21.3 | One completed turn | Actual plugin emitted start/done/end; upstream `on_session_end` has completed=true, failed=false, interrupted=false, session/turn ids, platform=cli |
| Grok CLI 1.0.60 | One completed turn after resuming the same pre-submission session | Native hook environment present, camel/snake aliases, genuine end_turn distinct from shutdown, idle notification |

Grok's live version was 1.0.60; earlier readiness research observed 1.0.52. Its
initial start/shutdown occurred before a model submission and is retained in the
evidence, followed by `source=load` and the completed turn. Codex's six new hooks
were reviewed and trusted through its native `/hooks` UI according to the run
manifest; that UI fact is not encoded in hook metadata.

## Projection limits

- `other` is the observer's replacement for any non-allowlisted enum, not proof
  that the native client sent the literal `other`. For example, some Grok camel
  event names and Hermes tool/status/exit names were lost. The surviving Grok
  PascalCase `hook_event_name` is retained without reconstruction.
- The generic observer did not retain `codepet_v`, `tools`, counts or Claude
  `prompt_id`. Plugin output projections therefore cannot serve as complete
  wire fixtures. Native projection tests do not add missing turn ids.
- OpenCode upstream records are flattened facts: their original property nesting
  was not retained. The adapter test constructs an explicitly synthetic SDK
  envelope around the observed values, and does not claim native-layout replay.
- Hermes upstream records retain the hook name and a keyword-argument projection.
  The adapter test replays those fields; it compares only captured output fields.
- Adapter tests omit output `codepet_v` and `tools` during comparison because the
  capture omitted them. CodePet itself emits those fields during offline replay;
  the test does not claim their values were captured live. OpenCode's generated
  turn id is replaced with the fixture's synthetic turn id solely for comparison.
- `capture_and_shim_us` measures the QA observer plus invoked shim. It excludes
  native scheduler/loader/login-shell overhead and cannot qualify native total
  hook latency or CodePet's standalone performance budgets.

Still unverified: Claude successful tool/Stop behavior; Grok native session-mode
field and Claude-plugin compatibility loading/duplicate probe; Hermes approval
`session_key` relationship; native total latency for all clients. No permission,
cancellation or failure branches beyond Claude's rate limit are newly qualified.

## Focused offline checks

```sh
cargo test -p codepet --test live_projections --locked --offline
python3 integrations/hermes/test_plugin.py Plugin.test_live_metadata_projection
node --test --test-name-pattern='live metadata projection' integrations/opencode/codepet.test.mjs
```

The Rust tests replay retained native metadata through the real hook, assert the
failure/completion distinction, and ensure incomplete plugin projections remain
rejected. The adapter checks test observed event values against existing mapping
behavior. These tests launch no coding clients and change no client configuration.
