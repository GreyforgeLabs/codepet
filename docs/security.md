# Security and privacy (v0.4)

CodePet-owned work and external coding-session observations have different data
boundaries. The observer never reads chats or source. Owned jobs intentionally
read a bounded source copy and may send its context to the signed-in provider.

## Owned work boundary

Automatic checks are enabled after the current life starts and can be paused in
**Details → Checks off**. Metadata discovery searches conventional development
roots without running Git, loading hooks or following symlinks/mount crossings.
Source preflight is separately bounded and may skip an unsuitable checkout. A
scout runs read-only for up to five minutes and returns at most one suspected bug;
only explicit Fix starts a repair and explicit reviewed Apply changes originals.

Codex 0.163.0 uses existing ChatGPT subscription authentication, with no API-key
billing route. Jobs have network access. A fresh private provider environment
loads no user hooks, rules or custom MCP integrations. Native filesystem policies
deny tool reads of the provider/auth paths, including tested aliases; the provider
itself retains the credential access needed to authenticate. Temporary auth
snapshots are removed on completion/recovery. Escalation requests are denied.

User-systemd enforces memory/process/lifetime limits. Bubblewrap gives each job its
own PID/mount namespace; no original home, project, desktop sockets or CodePet
state is mounted into it. Cancellation and deadlines terminate the namespace,
including detached descendants. Missing containment refuses work. Code work uses
bounded scratch space; research/scouting source is mounted read-only.

Known credential files/directories, build caches and (for scouts/scout repairs)
binary media assets are omitted. This is limited coverage and **not a general
secret detector**: ordinary source can contain secrets and be sent to Codex. Input,
traversal, manifests, messages, changed files and review text have explicit caps;
oversized or binary results fail rather than being silently accepted. Exact limits
and omissions are in [work.md](work.md).

Private work storage contains source copies, prompts, provider reports, reviewed
changes and recovery data under the protected state directory. Display exports
omit this content and expose only status, readiness, a finding flag and generic
preparation feedback. Full work RPC responses are private local UI data. Untrusted
text is rendered plainly, never as executable markup or shell interpolation.

Apply validates the displayed review digest and current destination bytes, refuses
conflicts/unsafe destinations, and journals original content. It replaces files
atomically one at a time, not as one multi-file transaction; concurrent outside
writers cannot be locked by CodePet. Keep editors/writers idle during application.
A permanent life-bound receipt prevents duplicate XP after retry or recovery.
Provider-reported checks require explicit acknowledgement, not a claim of
independent verification. Scouting and failed/cancelled work earn nothing.

## Hook boundary

Every hook path returns zero with empty stdout and stderr. The shell shim has a
bounded input drain and checks Off/disconnection before starting CodePet. The Rust
hook has an alarm deadline, takes only nonblocking locks, performs no network
request, opens no SQLite database and creates no directories. Invalid or oversized
payloads are discarded. A missing state directory stays missing.

Normalized inbox records contain only enums, counts, flags, timestamps and
installation-salted HMAC keys. They exclude prompts, responses, tool names,
inputs/outputs, paths, project names, cwd, transcript paths, errors, models and
approval commands. Raw client payloads are not kept. File ownership, permissions,
symlinks, sizes and event counts are checked before use.

The fold acquires `fold.lock` before the database. It writes agent tables,
client-hook state and agent metadata, and exports the committed result. It cannot
settle decay, revive, arm or end a life. Test events cannot earn or impersonate a
real client's first appearance. Treat earning and spending are durable and bounded.

## Connecting and disconnecting

Only explicit Connect/Disconnect jobs change client integration files. Status and
Doctor read detection metadata and only the client configuration keys named by the
contract; hooks, folds and exports never read client configuration.

- Claude Code uses its own plugin CLI. It records CodePet's marketplace and enabled
  plugin entries in its configuration. An already disabled plugin stays disabled
  until the user enables it in Claude.
- Hermes uses its own plugin CLI; that CLI may normalize `config.yaml`. A before/
  after check protects pre-existing hooks, approvals and permission keys.
- OpenCode and Grok use CodePet-owned plugin/hook files.
- Codex groups are merged without granting trust or changing unrelated handlers.
  Foreign trust fingerprints are checked before writing; the last write step
  compares the original bytes again. Symlinked hook files are not replaced.
  Review CodePet's entries in Codex's `/hooks` UI yourself.

Connect is refused for the bar's test-state override. A shim serving another
existing installation is not silently repointed. An explicit CLI
`codepet connect --repoint <client>` is the recovery route after checking that
installation. Interrupted jobs are journaled and bounded; recovery resumes only a
job whose process is no longer alive.

Disconnect preserves a silent tombstone if removing CodePet's entry would shift a
foreign Codex trust index. Never delete a shim a client can still invoke. The
uninstaller disconnects clients and preserves pet data and recovery backups.

## Power and retention

Stop active owned execution or finish application recovery before turning Off.
Off saves and freezes the pet, clears live activity and prevents earning. There is
no catch-up when enabled again. Codex itself may still start a login shell before
executing the shim; disconnecting removes CodePet's remaining event cost.

**Forget coding history** clears the activity history and jar. It preserves recent
coarse earning timestamps until their limits expire, so forgetting cannot reset
the earning caps. It does not remove pet growth or life memories. See the exact
retention rules in [contract notes](v0.3/contracts/notes.md).

The bar export is a bounded display subset, atomically replaced with monotonic
state versions. `pet.memories` exposes only display event kinds, pet names/looks,
stages and dates; raw life-event JSON and task/request identifiers stay private.
Pet names and untrusted text are rendered as plain text.

## Limits

This is a local convenience companion, not an anti-cheat or security boundary
against another process running as the same user. Such a process can forge
presence, edit local state or invoke the CLI. The optional helper-counting setting
includes headless activity; it is off by default. Owned work uses the existing
provider sign-in and can transmit selected source context to that provider. CodePet has no separate cloud account or telemetry service.

The old mock task pipeline is unreachable in the default build. Its non-default
`legacy-tasks` feature retains mock behavior only, with no real command execution.
The separate owned-work controller is documented above. The [original
specification](implementation-spec.md) preserves earlier goals; current behavior
and qualification are in [work](work.md) and [validation](validation.md).
