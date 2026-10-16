-- CodePet schema v5 (SPEC-v0.3 §6.6, §6.4, §5.10). Contract for WP1 (migration owner) and
-- WP2 (agent tables). `MIGRATIONS = [v1, v2, v3, v4, v5]`; v5 runs inside the existing
-- migration transaction after `VACUUM INTO codepet.sqlite3.bak-v4-<ms>`.
--
-- Additions beyond the spec's column lists are marked "(WP0)" and explained in notes.md.
-- Hash-like keys are 32 lowercase hex characters: HMAC-SHA256(meta.hook_salt, ...)[..16].
-- Client ids: claude-code, codex, opencode, hermes, grok-cli.

-- 1. meta additions -------------------------------------------------------------------
ALTER TABLE meta ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1));
ALTER TABLE meta ADD COLUMN count_helpers INTEGER NOT NULL DEFAULT 0 CHECK (count_helpers IN (0,1));
-- 32 random bytes from sys::random_bytes, written by the v5 migration itself.
ALTER TABLE meta ADD COLUMN hook_salt BLOB CHECK (hook_salt IS NULL OR length(hook_salt) = 32);
ALTER TABLE meta ADD COLUMN events_dropped INTEGER NOT NULL DEFAULT 0 CHECK (events_dropped >= 0);
ALTER TABLE meta ADD COLUMN finished_seq INTEGER NOT NULL DEFAULT 0 CHECK (finished_seq >= 0);
-- (WP0) the export's finished_* / hello_* fields need durable sources.
ALTER TABLE meta ADD COLUMN finished_at_ms INTEGER;
ALTER TABLE meta ADD COLUMN finished_app TEXT
  CHECK (finished_app IS NULL OR finished_app IN ('claude-code','codex','opencode','hermes','grok-cli'));
ALTER TABLE meta ADD COLUMN finished_treat INTEGER NOT NULL DEFAULT 0 CHECK (finished_treat IN (0,1));
ALTER TABLE meta ADD COLUMN finished_jar_full INTEGER NOT NULL DEFAULT 0 CHECK (finished_jar_full IN (0,1));
ALTER TABLE meta ADD COLUMN hello_seq INTEGER NOT NULL DEFAULT 0 CHECK (hello_seq >= 0);
ALTER TABLE meta ADD COLUMN hello_at_ms INTEGER;
ALTER TABLE meta ADD COLUMN hello_app TEXT
  CHECK (hello_app IS NULL OR hello_app IN ('claude-code','codex','opencode','hermes','grok-cli'));
ALTER TABLE meta ADD COLUMN retention_at_ms INTEGER;
ALTER TABLE meta ADD COLUMN test_state TEXT
  CHECK (test_state IS NULL OR test_state IN ('running','done','failed'));
ALTER TABLE meta ADD COLUMN test_at_ms INTEGER;

-- 2. agent_events: one row per inbox record seen by the fold (dedupe + audit), 7 days ----
CREATE TABLE agent_events (
  id INTEGER PRIMARY KEY,
  dedupe_key TEXT NOT NULL UNIQUE CHECK (length(dedupe_key) = 32),
  client TEXT NOT NULL CHECK (client IN ('claude-code','codex','opencode','hermes','grok-cli')),
  kind TEXT NOT NULL CHECK (kind IN ('session_start','turn_start','activity','waiting','idle_prompt',
    'turn_done','turn_failed','turn_cancelled','session_end')),
  -- notification or error class enum; never free text
  class TEXT CHECK (class IS NULL OR class IN ('permission_prompt','elicitation_dialog',
    'elicitation_url_dialog','agent_needs_input','idle_prompt','ask_user','rate_limit',
    'authentication_failed','invalid_request','server_error','max_output_tokens','overloaded',
    'billing_error','unknown')),
  at_ms INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('folded','dropped_off','dropped_old','dropped_child',
    'test','no_life')),
  -- (WP0) session_hmac: distinct-tool counting per (session, turn) needs it.
  session_hmac TEXT CHECK (session_hmac IS NULL OR length(session_hmac) = 32),
  turn_hmac TEXT CHECK (turn_hmac IS NULL OR length(turn_hmac) = 32),
  tool_hmac TEXT CHECK (tool_hmac IS NULL OR length(tool_hmac) = 32),
  interactive INTEGER NOT NULL CHECK (interactive IN (0,1)),
  test INTEGER NOT NULL DEFAULT 0 CHECK (test IN (0,1))
);
CREATE INDEX agent_events_at ON agent_events(at_ms);
CREATE INDEX agent_events_client_at ON agent_events(client, at_ms);
CREATE INDEX agent_events_turn ON agent_events(session_hmac, turn_hmac) WHERE turn_hmac IS NOT NULL;

-- 3. agent_sessions: one row per (client session) and per Hermes approval key ------------
CREATE TABLE agent_sessions (
  session_key TEXT PRIMARY KEY NOT NULL CHECK (length(session_key) = 32),
  client TEXT NOT NULL CHECK (client IN ('claude-code','codex','opencode','hermes','grok-cli')),
  -- (WP0) 1 = a lightweight Hermes approval row keyed by HMAC(approval_key); it only
  -- drives waiting (cleared by activity with the same key, or stale).
  approval INTEGER NOT NULL DEFAULT 0 CHECK (approval IN (0,1)),
  interactive INTEGER NOT NULL CHECK (interactive IN (0,1)),
  state TEXT NOT NULL CHECK (state IN ('idle','working','waiting','error','ended')),
  state_since_ms INTEGER NOT NULL,
  last_event_ms INTEGER NOT NULL,
  open_turn_key TEXT CHECK (open_turn_key IS NULL OR length(open_turn_key) = 32),
  open_turn_started_ms INTEGER,
  open_turn_tools INTEGER NOT NULL DEFAULT 0 CHECK (open_turn_tools >= 0),
  last_done_turn_key TEXT CHECK (last_done_turn_key IS NULL OR length(last_done_turn_key) = 32),
  -- (WP0) start of the last finished turn, for the late-tool treat re-check's duration.
  last_done_started_ms INTEGER,
  last_done_ms INTEGER,
  last_done_tools INTEGER NOT NULL DEFAULT 0 CHECK (last_done_tools >= 0),
  last_done_earned INTEGER NOT NULL DEFAULT 0 CHECK (last_done_earned IN (0,1)),
  -- (WP0) the late-tool rule re-runs the treat check at most once per finished turn.
  last_done_rechecked INTEGER NOT NULL DEFAULT 0 CHECK (last_done_rechecked IN (0,1)),
  -- (WP0) turn_failed "records error_at_ms" (§6.4); the export's error_at_ms is the max.
  error_at_ms INTEGER,
  CHECK ((open_turn_key IS NULL) = (open_turn_started_ms IS NULL)),
  CHECK ((last_done_turn_key IS NULL) = (last_done_ms IS NULL)),
  CHECK (approval = 0 OR open_turn_key IS NULL)
);
CREATE INDEX agent_sessions_live ON agent_sessions(state, last_event_ms);

-- 4. activity_turns: the treat jar. One row per earning turn (X1); never deleted --------
CREATE TABLE activity_turns (
  id INTEGER PRIMARY KEY,
  session_key TEXT NOT NULL CHECK (length(session_key) = 32),
  turn_key TEXT NOT NULL CHECK (length(turn_key) = 32),
  client TEXT NOT NULL CHECK (client IN ('claude-code','codex','opencode','hermes','grok-cli')),
  life_id TEXT NOT NULL REFERENCES lives(life_id),
  -- Coarsened to the hour after 24 h. "Forget coding history" sets it to 0 (WP0: a
  -- sentinel instead of NULL, so the caps simply stop counting the row).
  earned_at_ms INTEGER NOT NULL CHECK (earned_at_ms >= 0),
  -- NULL = still in the jar. Forget sets a given row's value to 0, never back to NULL.
  given_at_ms INTEGER CHECK (given_at_ms IS NULL OR given_at_ms >= 0),
  UNIQUE (session_key, turn_key)
);
CREATE INDEX activity_turns_jar ON activity_turns(life_id, id) WHERE given_at_ms IS NULL;
CREATE INDEX activity_turns_earned ON activity_turns(earned_at_ms);
CREATE INDEX activity_turns_session ON activity_turns(session_key, earned_at_ms);
CREATE TRIGGER activity_turns_never_deleted BEFORE DELETE ON activity_turns
BEGIN SELECT RAISE(ABORT, 'treat history is never deleted'); END;
CREATE TRIGGER activity_turns_given_once BEFORE UPDATE OF given_at_ms ON activity_turns
WHEN OLD.given_at_ms IS NOT NULL AND NEW.given_at_ms IS NULL
BEGIN SELECT RAISE(ABORT, 'a given treat never returns to the jar'); END;
CREATE TRIGGER activity_turns_identity BEFORE UPDATE ON activity_turns
WHEN NEW.id IS NOT OLD.id OR NEW.session_key IS NOT OLD.session_key
  OR NEW.turn_key IS NOT OLD.turn_key OR NEW.client IS NOT OLD.client
  OR NEW.life_id IS NOT OLD.life_id
BEGIN SELECT RAISE(ABORT, 'treat identity is immutable'); END;

-- 5. activity_xp: 5 XP per given treat (treats.give, same transaction); never deleted ---
CREATE TABLE activity_xp (
  id INTEGER PRIMARY KEY,
  life_id TEXT NOT NULL REFERENCES lives(life_id),
  turn_row_id INTEGER NOT NULL UNIQUE REFERENCES activity_turns(id),
  amount INTEGER NOT NULL CHECK (amount = 5),
  granted_at_ms INTEGER NOT NULL,
  -- (WP0) the treats.give request id, for receipt replay and the journal.
  request_id TEXT
);
CREATE INDEX activity_xp_life ON activity_xp(life_id);
CREATE TRIGGER activity_xp_never_deleted BEFORE DELETE ON activity_xp
BEGIN SELECT RAISE(ABORT, 'xp history is never deleted'); END;
CREATE TRIGGER activity_xp_immutable BEFORE UPDATE ON activity_xp
BEGIN SELECT RAISE(ABORT, 'xp rows are immutable'); END;

-- 6. client_hooks: one row per client CodePet has touched -----------------------------
CREATE TABLE client_hooks (
  client TEXT PRIMARY KEY NOT NULL CHECK (client IN ('claude-code','codex','opencode','hermes','grok-cli')),
  connected INTEGER NOT NULL DEFAULT 0 CHECK (connected IN (0,1)),
  needs_step TEXT CHECK (needs_step IS NULL OR needs_step IN ('codex_trust','codex_hooks_off')),
  installed_version TEXT CHECK (installed_version IS NULL OR length(installed_version) <= 64),
  installed_at_ms INTEGER,
  last_seen_ms INTEGER,
  first_seen_ms INTEGER,
  events_total INTEGER NOT NULL DEFAULT 0 CHECK (events_total >= 0),
  -- (WP0) last job result, so Helpers can show "Couldn't connect" + its reason later.
  last_outcome TEXT CHECK (last_outcome IS NULL OR last_outcome IN ('connected','needs_step','failed','disconnected')),
  last_reason TEXT CHECK (last_reason IS NULL OR last_reason IN ('not_installed','too_old','blocked',
    'settings_changed','ready_check')),
  -- (WP0) the running job, mirrored into the export as clients.<id>.job.
  job_id TEXT CHECK (job_id IS NULL OR length(job_id) BETWEEN 1 AND 64),
  job_kind TEXT CHECK (job_kind IS NULL OR job_kind IN ('connect','disconnect','test')),
  CHECK ((job_id IS NULL) = (job_kind IS NULL))
);

-- 7. Data steps of v5 (WP1, see SPEC §6.6; not DDL):
--   * meta.hook_salt = 32 random bytes;
--   * close leftover slot tasks with close_task_for_migration (no reward):
--       preparing|running|verifying|cancelling|waiting_for_user -> interrupted,
--       review_ready|recovery_required -> discarded;
--   * UPDATE preflight_tokens SET status = 'invalidated' WHERE status = 'pending';
--   * clear life_vitals.death_pending (the spec says pet_care; v4 keeps it in life_vitals)
--     with a journal note; normal settlement re-evaluates the life afterwards;
--   * zero pet_care.work_sessions, execution_ms, rest_required, starts_since_bathroom;
--   * lives, journal, memorials and historic tables untouched.
