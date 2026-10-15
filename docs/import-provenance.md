# Verified source import

The repository starts from a verified import of the initial scaffold; earlier history is
not included. `docs/source-manifest.sha256` records the imported files' hashes and is
intentionally stale: later commits changed those files.

All 22 imported source paths were validated as safe relative paths, and their UTF-8
contents matched their recorded SHA-256 hashes before they were written. All 22 files
were checked again after writing. Existing different files and symlinks were refused.

## Local validation, 2026-10-14 UTC

Existing toolchain: rustc 1.98.1 (48a229cea 2026-09-01), cargo 1.98.1,
rustfmt 1.9.0-stable, clippy 0.1.98. No packages or toolchains were installed.
Cargo network access was disabled and build concurrency was bounded to two jobs.

- `cargo test --workspace --locked`: 24 passed, 0 failed (21 core, 3 CLI).
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.

The import commit uses `[skip ci]` so initial publication does not launch the
prepared hosted workflow or install its toolchain. No hosted CI result is claimed.
No real coding provider, resident service, desktop preview, or live Omarchy test
was launched. Portable Linux with Omarchy integration by default remains the target;
the current scaffold is mock-only and QML remains disconnected from Rust.
