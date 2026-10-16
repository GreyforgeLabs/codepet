# Security and privacy (v0.3)

CodePet is an observer. It does not launch, signal, block or answer coding apps,
read transcripts, inspect source folders, or store prompts and code.

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
includes headless activity; it is off by default. No authentication is managed by
CodePet and no remote service receives its data.

The old task pipeline is unreachable in the default v0.3 build. Its non-default
`legacy-tasks` feature retains mock behavior only, with no real command execution.
The [original specification](implementation-spec.md) describes additional historic
task/sandbox goals, not implemented capabilities of this release.
