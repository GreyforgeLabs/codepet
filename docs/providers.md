# Provider compatibility

| Provider | Scaffold status | Execution |
| --- | --- | --- |
| Mock | Available | In-memory transitions only |
| Codex | Unsupported | Never launched |
| OpenCode | Unsupported | Never launched |
| Hermes | Unsupported | Never launched |
| Claude Code | Unsupported | Never launched |

No provider executable or CLI flag has been assumed, probed or run. There are no real invocation builders. Before enabling an adapter, pin and test its version, consult official documentation, prove config/hook/plugin isolation and confinement, and add success/error/auth/approval fixtures plus cancellation tests. Installation alone must not enable it. The project itself supplies no API keys or authentication flow.
