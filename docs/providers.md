# Provider compatibility (v0.5)

Choose the next executor from **Client** on the pet's compact status strip or in
Work details. The choice persists across reopening and restarting. Changing it
never starts work, retries a failure, resets a cooldown, changes an existing job,
or switches to another client automatically. Each job retains its client, adapter
version and authentication route. A stale Start/Fix click is refused if selection
changed in another window.

| Executor | Qualified execution boundary | Requirements |
|---|---|---|
| Codex | Read-only scouting/research, isolated commands and reviewed code changes | Native **0.163.0**, existing ChatGPT subscription sign-in |
| Claude Code | Restricted Read/Glob/Grep/WebSearch; code jobs also Edit/Write. No shell, subagents or command-based tests | Native **2.1.294**, Claude Pro/Max subscription access token valid for at least 25 minutes |
| OpenCode | Isolated commands and reviewed code changes, explicit **opencode/big-pickle** model | Native **1.19.0**; no account credentials copied or used |
| Muse, Grok, Hermes, pi | Catalog entries with detection and a clear unavailable reason; owned execution is not qualified | Installing or signing in does not enable an unsafe adapter |

All owned execution currently requires Linux user systemd, Bubblewrap and
`prlimit`. Native versions are checked inside containment before execution.
Unknown versions and update-launcher scripts are refused; CodePet does not update
coding clients. Detection uses conventional native installation locations and
native executables on PATH; cold status checks do not execute clients or contact
providers. An available entry still needs version/sign-in preflight at Start.

Codex uses its ChatGPT subscription route, not API-key billing. Claude receives a
short-lived copy of access-token metadata **without its refresh token**; a job
cannot rotate the user's stored login. If that access token is too close to
expiry, refresh the sign-in in Claude Code before starting. Claude's command
checks are unexecuted and must not be presented as verified tests.

OpenCode uses the pinned free Big Pickle model and literal public provider authorization
in a fresh private configuration/cache. It does not borrow OpenCode Go, other
providers' credentials, or the user's configured model, and has no model fallback.
Remote availability or free-tier limits may change; a provider refusal fails the
job visibly. CodePet does not promise a model will remain free or available forever.

All jobs have network access and selected source context can reach the named
provider. Original projects and host home/configuration stay outside the job.
Known client configuration is excluded at every source depth before staging.
This exclusion is essential for OpenCode: native pure/config flags alone still
import some local configuration plugins in the qualified version. No user hooks,
custom MCPs or plugins are supplied to the owned job. Native dummy-provider gates
exercise actual file tools, credential aliases, configuration canaries and the
production staging filter; they do not contact a model.

A scout may start after Let's go! when checks are enabled, readiness and durable
cooldowns allow it. Pause future checks from **Details → Checks off**. Fix, custom
work and Apply remain explicit. Successful process exit alone is insufficient:
code jobs need a bounded reviewable change; research needs a nonempty report.
Reports and test claims remain provider-reported until separately verified.

Observer connections are independent of this selector: external Claude Code,
Codex, OpenCode, Hermes and Grok sessions can still contribute metadata through
Helpers. T3 Code is a cockpit for underlying clients, not a separate executor.
The retired mock pipeline remains non-default `legacy-tasks` compatibility only.
See [work](work.md), [security](security.md) and [qualification](validation.md).
