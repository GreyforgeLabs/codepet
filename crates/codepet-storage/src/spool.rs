//! The hook inbox (spec §5.1, §5.8): bounded event records written by `codepet hook`
//! and drained by the fold. Owned by WP2; WP0 declares the module only.
//!
//! Invariants for WP2: the writer never opens SQLite, never creates directories, writes
//! `.tmp-<event_id>-<pid>` with `O_CREAT|O_EXCL|O_NOFOLLOW` (0600, ≤ 512 B) and publishes
//! it with `renameat2(RENAME_NOREPLACE)`; the reader ignores dotfiles and unlinks poison.
