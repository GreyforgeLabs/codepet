# Verified source import

The initial local Git commit is a new import, not a continuation of the cloud Git history.
The source transfer identifies cloud commit `7095314c28ec6d20a06c5a50007f2b54ec9d4de8`.
No Git objects or original commit history were transported, and that commit is not claimed
as part of this repository's history.

All 22 supplied source paths were validated as safe relative paths, and their UTF-8
contents matched their supplied SHA-256 hashes before materialization. All 22 files
were checked again after writing. Existing different files and symlinks are refused.
The original source files remain byte-for-byte unchanged. `source-manifest.sha256`
records their expected hashes and can be checked from the repository root.

Internal transfer SHA-256:
`7f9bd55e70c8e5dd5b50f4958a77c045a8a88329c1df7e367024f0495f29dd93`

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
