# Provider compatibility

| Provider | Milestone 1 status | Execution |
| --- | --- | --- |
| Mock | Available | Per-job mock supervisor; spawns no process, runs no command, touches no project files |
| Codex | Unsupported | Never launched |
| OpenCode | Unsupported | Never launched |
| Hermes | Unsupported | Never launched |
| Claude Code | Unsupported | Never launched |

A draft may name any provider and is stored, but preflight and start refuse every provider except `mock` with `UNSUPPORTED_PROVIDER`, whether or not the CLI is installed. `state.get` and `providers.inspect` list the four real providers as "Not yet supported on this version".

The mock offers three outcomes: an artifact for review (`not_verified`), a provider failure (terminal `failed`, no review), and an artifact with a simulated failed check. No mock outcome claims `passed`.

No provider executable or CLI flag has been assumed, probed or run. There are no real invocation builders and no shell fallback. Before enabling an adapter, pin and test its version, consult official documentation, prove config/hook/plugin isolation and confinement, and add success/error/auth/approval fixtures plus cancellation tests. Installation alone must not enable it. The project supplies no API keys or authentication flow.
