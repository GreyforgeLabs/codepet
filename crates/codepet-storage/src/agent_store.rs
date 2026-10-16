//! Agent tables of schema v5 (spec §6.4, `docs/v0.3/contracts/schema-v5.sql`): the fold's
//! per-event transitions, the treat check, retention and the `agent` export block. Owned
//! by WP2; WP0 declares the module only.
//!
//! Invariant E1 for WP2: the fold writes only `agent_events`, `agent_sessions`,
//! `activity_turns`, `client_hooks`, the agent fields of `meta` and the export.
