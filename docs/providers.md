# Provider compatibility (v0.4)

| Route | Qualified scope | Requirements |
|---|---|---|
| Pet-owned Codex work | Read-only bug scouting/research and isolated code candidates; explicit reviewed Apply | Codex **0.163.0**, existing ChatGPT subscription sign-in, Linux user systemd, Bubblewrap and `prlimit` |
| Claude Code, OpenCode, Hermes, Grok and external Codex sessions | Optional metadata-only observation | Explicit Helpers connection; client-specific hook support |
| Retired mock pipeline | Non-default compatibility tests only | `legacy-tasks`; not used by pet-owned work |

Other Codex versions are refused until qualified. No other client is a CodePet-owned
execution adapter. Installing or connecting another coding client does not make
CodePet launch it for jobs.

The Codex adapter uses the existing subscription OAuth route; API-key billing is
refused. A scout can start automatically after Let's go! when readiness and durable
cooldowns allow it. Pause future checks from **Details → Checks off**. Fix, custom
work and Apply remain explicit. Selected source context can be sent to the provider,
and the contained job can access the network.

Every owned job uses fresh private configuration, native filesystem permission
profiles, resource limits and a PID/mount namespace. No user hooks/custom MCPs
load, no permission answer is automatically approved, and the original project
is absent during execution. Version/startup checks do not invoke a model.

Real disposable qualifications exercised read-only scouting, explicit Fix,
research and reviewed code application. Tests also cover credential read denial,
process-tree cancellation, conflict refusal and exactly-once rewards. A successful
provider process alone is insufficient: code jobs need a reviewable change and
research needs a nonempty report. Checks remain provider-reported until separately
verified; the UI requires the user's acknowledgement.

See [work limits and recovery](work.md), [security](security.md) and
[qualification](validation.md). The frozen v0.3 client adapter fixtures describe
the independent observer pipeline, not owned-work permissions.
