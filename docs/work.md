# Useful work

CodePet discovers local repositories and uses the selected coding client
to look for one suspected bug in a read-only isolated source copy. Inspect
a finding on the pet, choose **Fix**, then review and apply the resulting repair.
Discovery and scouting grant no reward. Code changes remain in the isolated copy
until explicit Apply. Research and custom coding tasks remain available through
the advanced window with an explicit project, task and Start action.

Accepting a useful result gives 20 XP exactly once and fills food (bug), water
(research) or mood (build). A successful process with no code changes is not an
accepted bug/build result. Reports and checks remain provider-reported: the app
shows an explicit verification acknowledgement and does not claim independent
verification. Research sources should be inspected by the user.

## Choose a coding client

The compact pet strip has a visible **Client** button. It expands an inline list
from the backend's supported catalog, including each client's availability and
setup explanation. The advanced Work view uses the same selector. A name in the
list is not a claim that the client is installed, authenticated or ready: selecting
an unavailable client saves that explicit preference, shows why it cannot work,
and blocks new work until setup is ready. There is no fallback to another client.
See [adapter support and limits](providers.md) for the current execution routes.
The expanded chooser shows the selected adapter's capability limits even when it
is available. Full job results retain their original capability note; for example,
a file-only adapter does not claim to have run command-based checks.

The choice is for the next job. A pending save keeps the previous confirmed label;
a failed save does not claim success. Closing and reopening reads the saved choice.
Changing it does not launch a check or repair, change the checks toggle, or reset
scouting cooldowns. A later normal readiness/deadline transition can make an
automatic check eligible under the existing rules.

Every job retains the client that created it. The strip shows **Job: …** separately
from the next-client button; the expanded selector gives the full origin. Historical
jobs without a stored client are Codex jobs. Findings retain their originating
client, while **Fix** uses the currently selected client shown in the chooser.
Start and Fix include that expected client identity, so a concurrent selection
change is rejected rather than running the wrong executor.

**Helpers** is separate: those connections watch external sessions that you launch.
They do not select the executor for CodePet-owned work. T3 is a cockpit using the
underlying client, not a separate CodePet executor.

## Care and energy

Mood must be at least 25%; Play restores it. A sleeping or tired pet cannot start a
new job. Three accepted jobs or 45 minutes of accepted work use its energy; a full
two-minute nap restores it. Failed and cancelled jobs do not spend work energy or
award XP. Hunger and thirst suggest jobs without blocking them. Free snacks/drinks
remain available to recover a fainted pet, and ordinary care awards no XP.

Existing external coding-app observations and treats stay independent. CodePet
never controls sessions launched outside CodePet. Scouting is the sole automatic
provider action; there are no automatic repairs, retries, queues, commits, pushes
or deployments.

## Discovery and scouting

Metadata discovery searches `~/Projects`, `~/Work/workspaces`, `~/Code`, `~/code`,
`~/src`, `~/repos` and `~/Developer`. It starts from those roots and searches four
directory levels down, stopping at a discovered checkout. Hidden, cache, system
and mount directories are excluded; symlinks and mount crossings are refused by
`openat2`. `.git` directories and bounded `gitdir:` files identify candidates. A
gitfile target is never followed. Discovery reads no project source or Git config,
runs no Git process, loads no hooks, and creates no state or provider session.

A scan examines at most 12,000 directory entries, returns at most 200 repositories
in a 128 KiB response, and checks a two-second budget between metadata operations.
Paths are bounded to 4,096 bytes. Results sort by case-insensitive name then path;
physical directory identities are deduplicated, leaving separate checkouts intact.
The `incomplete` flag reports a budget limit; inaccessible directories are counted.
Discovery is limited coverage, not evidence that a bug exists or that staging will
succeed. It does not search inside an already discovered checkout for nested repos.

Automatic scouting is enabled by default for the requested icon workflow and can
be paused. It requires Let's Go to have started the current life and the ordinary
On/happy/awake/energy gates. One attempt per six hours globally and one per repo per
24 hours are persisted before any staging/provider preparation, including failed
preparation. The bounded attempts map contains at most 200 hashed paths. Among
eligible candidates, a recent safe directory or Git index/HEAD-log metadata time
is preferred, with path order breaking ties. Metadata age is only a relevance hint.
Before choosing a provider job, one attempt checks at most ten cooldown-eligible
checkouts against the existing source rules, skipping those that cannot be staged.
These checks share a five-second budget (checked between filesystem operations),
40,000 directory entries and 64 MiB of input reads. Oversized files are rejected
from metadata first. Every checked repo consumes its daily receipt, including
skipped repos; only one eligible source tree can proceed to a provider launch.
The selected baseline is reused and its hashes are checked again while copying.
A failed preparation remains available as a Previous check notice until dismissed;
its bounded private reason is retained locally, while the display export contains
only a generic notice. Dismissal does not retry the check or change its cooldown.

Each scout has a five-minute execution deadline (350-second final cgroup lifetime).
It can return at most one structured, provider-reported suspected bug, or no finding.
The final message is validated separately from commentary; malformed, oversized or
out-of-scope findings fail. A finding keeps the slot until Fix or Dismiss. Fix is
explicit and idempotent, binds the original report and repo, and starts an ordinary
bug repair. Scouting cannot be accepted for XP. A pass with no finding releases
the slot for a later eligible attempt and does not assert that all code is correct.

## Runtime and scope

- Linux user systemd with enforced cgroup memory/process limits, Bubblewrap and
  `prlimit` are required. Missing containment fails before provider work.
- Codex uses its existing ChatGPT subscription sign-in. API-key billing is refused.
  Native permission profiles deny task-tool reads of `/auth` and `/provider`, including
  aliases, while the provider process can authenticate. Dummy-credential tests exercise
  that boundary without using a real token. User configuration, hooks, custom MCPs,
  project `.codex` and `.claude` configuration
  are not loaded. Requested escalations are denied; no permission is silently granted.
- The selected project is copied without `.git`, dependency/build caches and known
  credential files/directories (`.env*`, private-key files, `.ssh`, `.aws`, etc.).
  Symbolic links, hard-linked source files, mount crossings and special files are
  refused. A project must not contain or lie inside CodePet's private state.
  Scout and scout-fix source
  copies additionally omit known binary media/art, fonts and archives by extension;
  omitted original assets are never deleted as a consequence. This is limited source
  coverage, not a full repository audit. Generated output still receives the full
  size/binary validation. Manual jobs retain their original staging limits.
  This is not a general secret detector: staged source context can be sent to the
  signed-in provider. Pausing automatic scouting prevents future scout launches.
- The provider sees system tools and the staged project, not the original project,
  home, desktop session, sockets or CodePet database. Research/scouting mount the project
  read-only. Code work uses a 128 MiB tmpfs; provider scratch is 32 MiB and temporary
  files 64 MiB. The whole job has 1 GiB memory, no swap, at most 128 processes and a
  20-minute execution limit (the cgroup has a final 1,250-second lifetime limit).
- Selected input is at most 64 MiB, 10,000 files and 2 MiB per file. Encoded filename
  metadata is limited to 2 MiB before a job starts; saved job state is capped at 4 MiB.
  Project traversal also caps depth at 32 and entries at 40,000, checking a 30-second
  budget between operations. Reviews are limited to 100 changed UTF-8 text files and
  384 KiB of combined report/before/after text. Both complete file versions count,
  even for a small edit. The JSON-encoded review has a separate 512 KiB limit;
  escaping can reach that limit first. Reports are capped at 64 KiB, individual
  events at 1 MiB and the complete provider stream at 2 MiB. These limits are
  explained to the provider before work starts. Binary changes and oversized
  reviews are refused as a whole; changed files are never silently omitted.
  Export errors identify CodePet's review limit, with the filename where available.
  Large jobs need a smaller selected folder/task.
- A helper exports bounded text changes after the coding client exits. Provider writes cannot
  grow the host staging directory. Cancellation kills the complete PID namespace,
  including children that create new sessions.

## Left-click status and previous results

A normal desktop-pet click opens a compact status strip, with exact XP, current-life
age, numeric food/water/mood/cleanliness and work energy. Current readiness comes
first. **Fix**, **Stop** or **Review** appears when that action is relevant; **Care**,
**Details** and **Updates** remain explicit controls. Details expands the current
context, finding or saved report. Full Work opens only through an explicit advanced
work or Review action; accepting/applying still requires the full review safeguards.

Finished work is a small **Previous repair/check/research/build** cue, not the pet's
current status. Details labels the saved timestamp **Started … ago**; the advanced window shows
the absolute start date/time. Neither claims an end time. **Dismiss** durably acknowledges that exact finished result.
The saved job status, error, output, changes and evidence remain under its private
job directory, while the compact card, reopened Work window and display export no
longer repeat it. The advanced window's **Dismiss result** has the same behavior.
Active work, review, interrupted application and actual suspected findings cannot
be acknowledged this way; their existing Stop/Review/Discard controls remain.
A new failure is a new notice and is not covered by an earlier acknowledgement.

Saved preparation errors are separate Previous check notices. Their dismissal is
bound to the exact saved attempt and preserves the private error, scouting setting
and cooldown. Fresh action/transport errors are shown as **Last action**, rather
than treating a saved preparation failure as a new error on every refresh. Dismissal
starts no job, changes no care/XP and does not enable automatic checks.

## Review, application and recovery

Keep editors and other writers idle while applying. Every destination must still
match its baseline; a conflict preserves the user's edit and refuses application.
Parent links and hard-linked destination files are refused. Changes replace files
atomically one at a time, not as an atomic multi-file transaction. Concurrent
outside writers cannot be locked by CodePet and remain an explicit limitation.

Private state is under `$XDG_STATE_HOME/codepet/work` (normally
`~/.local/state/codepet/work`). Each job has `task.json`, original `base/`, a bounded
`project/` candidate and, once application begins, `apply-journal.json`. Files are
protected by the private parent directory. No source enters the content-free
observer export. `work-state.json` contains only display status/readiness.
It includes the scout toggle/deadline and `has_finding`, but excludes the finding's
text/path, raw provider messages, prompts, source bindings and errors. The private
`scout-settings.json` retains at most 1,024 bytes of the last preparation failure;
full `get` returns this as `scout.last_error`, and the redacted export omits it.

A crash during application leaves the job in **applying**. Reopen Work, review and
retry Accept: each destination must match either its original or reviewed content;
changed unrelated content is never overwritten. The original `base/` and journal
remain available for manual recovery. The permanent `work.accept:<task-id>` SQLite
receipt prevents a retry after a reward commit from granting XP twice. Results
remain bound to the original companion.

An interrupted execution is marked failed on the next work/status refresh and is
never resumed automatically. Its temporary provider/auth directory is removed;
normal completion removes it immediately. Closing the work window leaves the
bounded supervisor running. Turning Off or starting a nap during execution is
refused until the user cancels it. Cancel/discard/recovery are separate from new-job
readiness. The historical mock-task tables and observer privacy rules are preserved.

## Local commands

`codepet-overlay card` opens the pet card, falling back to a compact native window
without changing desktop-pet visibility. `codepet-work [compact|bug|research|build]`
opens the compact or advanced native window. `codepet work-rpc` reads
one JSON request from stdin and prints one JSON response. Its methods are `status`,
`start`, `get`, `cancel`, `accept`, `discard`, `dismiss`, `repos`, `scout`, `scout-settings`, `client-select` and
`fix`; this protocol is separate from the
retired `task.*` mock protocol. `work-run` and `work-sandbox` are internal supervised
entry points, not alternate ways around readiness, cooldown or explicit repair gates.

`client-select` takes `{ "client_id": "<catalog id>" }`. Status/get return the
selected `provider` and the `clients` catalog, each with `id`, `name`, `available`
`reason` and `detail`. Start/Fix carry the expected `client_id`; task records keep
their own `client_id`, `client_name` and `client_detail` independently of this preference.

The durable age timestamp is `life.adopted_at_ms`, now present in bar-state as well
as state.get. It is the current life's existing creation time; names, appearances,
sleep and rewards do not reset it. `generated_at_ms` supplies the export clock.

`dismiss` takes exactly `{ "task_id": "<current finished task id>" }`, or
`{ "scout_due_ms": <current saved next_due_ms> }` for a preparation notice. The
existing work lock serializes it with job changes. Acknowledgement is a
backward-compatible default-false flag in the private task/settings record; no
SQLite migration is required. It does not delete task evidence or clear a pending
finding. Old or mismatched identities are refused.
