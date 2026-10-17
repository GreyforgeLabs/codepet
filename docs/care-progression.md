> **Current v0.4 loop, 2026-10-17:**
> Accepted bug fixes feed the pet, research quenches thirst, and building lifts mood;
> accepted jobs earn XP. Happy/awake/energy gates replace the historical full-bars gate.
> See [decision 0003](decisions/0003-care-driven-real-work.md) and [work.md](work.md).
> Older mock/observer-only supersession notes below are historical.


## Historical care-only implementation

The following record describes the superseded mock-task loop. Current behavior
and real-work rewards are in [work.md](work.md).

Care gets the pet ready. Explicitly accepted simulated work earns **demo XP**, never
verified-work XP. This change does not enable real providers, project writes, verification
commands, apply, or additional permissions. Constants below are provisional playtest
values, not measured findings.

## Manual loop

1. Use food, water, play and wash to fill the four meters. Each action fills its meter.
   The care routine covers the needed care; care never launches a task or earns XP.
2. After two successful starts, use the tree for a bathroom break. Washing alone cannot
   clear the bathroom requirement.
3. Review a task and its preflight summary, then explicitly Start. All meters must equal
   100, the pet must be awake, no nap or bathroom break may be due, and the existing
   provider, revision, scope and single-slot checks must pass.
4. Start deducts nourishment 10, hydration 10, mood 5 and hygiene 10 exactly once in the
   same transaction that acquires the active slot. Changed care invalidates preflight;
   care requirements never interrupt the task already running.
5. Inspect the actual simulated result and acknowledge its missing verification before
   accepting. A normal accepted mock artifact earns 20 demo XP once in its category;
   failed simulated checks earn none. Acceptance does not refill care.
6. After three sessions or 45 minutes accumulated execution, explicitly put the pet to
   bed for two minutes. No task starts on waking. Sleep resets rest counters, not care.

A start that fails before entering Running refunds its debit/session exactly once. Work
that entered Running uses the session even if it fails, is cancelled or interrupted.
There is no absence decay, sickness, death, daily streak, guilt or missed-day penalty.

> **Superseded in v0.2.0 (2026-10-15).** Armed v0.2 lives decay during attended time and can
> become critical; in permadeath mode neglect can end a life, in gentle mode the companion
> faints. There is still no offline or overnight decay, sickness, streak, guilt or
> missed-day penalty. See [lifecycle.md](lifecycle.md) and
> [decisions/0001-v0.2-lifecycle.md](decisions/0001-v0.2-lifecycle.md).

## Durable commands

All use the existing v1 JSON envelope. New methods:

- `pet.care`, params `{ "action": "food" }`; actions are `food`, `water`, `play`,
  `wash`, `bathroom`, `routine`
- `pet.sleep`, params `{}`
- `pet.wake`, params `{}`

The envelope request ID is the idempotency key. Reusing it with the same action returns
the original receipt; reusing it for a different mutation is refused. A later deliberate
action needs a new ID. Care receipts (`pet.care`, `pet.sleep`, `pet.wake`) are bounded:
each new one prunes all but the most recent 256 in the same transaction, so ordinary
clicks cannot grow the database without limit. A retry of a request older than that
window runs as a new action. Acceptance receipts are never pruned. The response includes
the action, whether it changed state, meters and care state. `state.get` additionally returns `care` and `progression` snapshots.

Sleep is explicit and timestamp-backed; it can finish while the application is closed.
Closing an awake pet does not put it to bed. Early wake preserves remaining sleep credit
and still requires the outstanding rest. Settlement is command-driven, with conservative
clock handling and no per-second writes, idle Rust process or automatic job resumption.
UI countdowns are presentation only; the next command reconciles authoritative state.
Naps are measured with CLOCK_BOOTTIME, so time the computer spends suspended counts as
rest. Execution accounting uses CLOCK_MONOTONIC, which excludes suspend, so suspending a
laptop mid-run is never charged as supervised execution or forces a nap.

`state.get` (and `codepet state`) is therefore not always a pure read. When a nap is in
progress or a live supervisor is running or verifying, it takes the writer lock, settles
the persisted rest, and checkpoints the execution segment; the protocol classifies it as
mutating. Recording execution, whether by a snapshot checkpoint or a supervisor step,
never invalidates preflight approvals (the busy slot already blocks other starts and Start
re-checks readiness, as with the start debit) and moves the care revision only when it
makes a nap due. With nothing to settle, `state.get` is a plain WAL read
that does not wait for the writer lock.

## Progression boundaries

Real and demo progression are separate. Existing mock work uses the small 20-XP template.
The 35/50-XP tiers and real verification-backed rewards remain future work. Demo levels
use lifetime thresholds 0, 80, 200, 400 and 700; the four mastery tracks are Bugfix,
Quality & Performance, Learning & Specification, and Cleanup. Category wire names stay
compatible (`hunger`, `thirst`, `mental_health`, `hygiene`).

A level is a cosmetic/guidance milestone. It cannot change repository access, provider
availability, task scope, approvals, concurrency or model capabilities. Higher guided
work remains subject to the same unsupported real-provider gates. No retroactive XP is
invented for old acceptance receipts, and no unsupported verification is called passed.

## Migration and evidence

Schema v2 migrates atomically, preserving task history, artifacts, original meter balances
and old acceptance receipts. Existing users are offered an explicit welcome care routine.
Older application builds refuse the newer schema; keep a normal consistent SQLite backup
before upgrading if rollback is required (including WAL data where applicable). The
migration does not promise a reverse schema conversion or silently delete history.

Schema v3 is additive: it adds the `review_repairs` audit table used when explicit
recovery has to replace missing or malformed review metadata (see milestone-1.md).

The UI's task timeline describes recorded lifecycle transitions. It does not claim files
were read, commands were run or tests passed by the mock. Real action-level execution
receipts, paged logs, diffs and verification integration are still future provider work.
Common credential patterns and terminal/bidirectional controls are filtered from newly
captured text before persistence/review hashing. Filtering is conservative and best-effort:
unknown secrets may not be recognized, and prior stored content is not silently rewritten.
Never include credentials in instructions. There is no unredacted raw-log export.

Reduced motion and quiet presentation preserve state and evidence. Any idle animation is
bounded and stopped while hidden. Resource goals and live Omarchy accessibility still
require measurement; source review and offscreen tests are not desktop certification.
