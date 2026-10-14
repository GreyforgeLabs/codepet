# Scaffold validation

Verified in the Linux cloud build workspace on 2026-10-14. This is not Omarchy/Qt runtime validation.

## Passed

Both official Rust toolchains were installed locally outside the repository, without changing the host shell profile:

- rustc 1.99.0 (b940084d7 2026-09-28)
- rustc 1.85.0 (4d91de4e4 2025-02-17), matching the declared minimum and CI

On **both** toolchains:

- `cargo fmt --all --check`: passed
- `cargo test --workspace --locked`: **24 passed, 0 failed** (21 core unit tests, 3 CLI integration tests; no doctests)
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed
- Explicit `mock-demo`: reached ReviewReady, retained mental health at 70, stated NOT VERIFIED and exited without acceptance

The `providers` smoke check lists Mock available and all four real providers unsupported. CLI tests additionally prove no-argument invocation only shows help and the `codex` command is rejected.

Core tests cover unchanged startup, stale/replaced/cross-engine/cross-job tokens, unsupported providers, single active slot, waiting, review, cancellation acknowledgement, duplicate acceptance, immutable review identity, override reasons, category-only reward and clamp, failure/discard behavior, restart interruption, preserved receipt semantics, stale token recovery, bounded history/events/frames, resync, version/UTF-8 checks, lexical paths, settings, and invalid transitions.

## Not run / not established

- QML lint/runtime: `qmllint` wrapper resolves to missing `/usr/lib/qt5/bin/qmllint`; no usable Qt 6 runtime/linter found. QML received static source review only.
- No Omarchy, Quickshell, X11, Wayland or distribution compatibility test
- No UI screenshots, rendering, accessibility or interaction test on a live host
- No connected Rust/QML end-to-end test; these remain separate prototypes
- No production provider, filesystem isolation/apply, real persistence/crash or cancellation test
- No CPU/RSS/binary-size measurements; resource budgets remain design targets
- GitHub Actions workflow is prepared, but no hosted CI run is claimed

The spec copy was compared byte-for-byte with the latest source specification at this milestone. This report describes this scaffold, not the full target specification's acceptance matrix.
