-- CodePet schema v5 (SPEC-v0.3 §6.6, §6.4, §5.10). Contract for WP1 (migration owner) and
-- WP2 (agent tables). `MIGRATIONS = [v1, v2, v3, v4, v5]`; v5 runs inside the existing
-- migration transaction after `VACUUM INTO codepet.sqlite3.bak-v4-<ms>`.
--
-- Additions beyond the spec's column lists are marked "(WP0)" and explained in notes.md.
-- Hash-like keys are 32 lowercase hex characters:
--   hex(HMAC-SHA256(meta.hook_salt, domain || 0x1f || client || 0x1f || part [|| 0x1f || part ...]))[..32]
-- with one domain per key kind (agent_vectors.json "hmac_keys" has the exact parts and vectors):
--   codepet-session-v1  (session)            agent_sessions.session_key (approval = 0), activity_turns.session_key,
--                                            agent_events.session_hmac
--   codepet-approval-v1 (approval_key)       agent_sessions.session_key (approval = 1)
--   codepet-turn-v1     (session, turn)      open/last_done turn keys, activity_turns.turn_key, agent_events.turn_hmac
--   codepet-tool-v1     (session, tool)      agent_events.tool_hmac
--   codepet-dedupe-v1   (kind, session field, dig, bucket)   agent_events.dedupe_key
-- The client id is in every input, so two clients sharing a raw session id never collide, and
-- the domain tag keeps a session key and an approval key with the same raw text apart.
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
-- (WP0) agent.test.client: the first tested client in client order. Only the clients job
-- writes test_state/test_at_ms/test_client; the fold never does (agent_vectors.json fold).
ALTER TABLE meta ADD COLUMN test_client TEXT
  CHECK (test_client IS NULL OR test_client IN ('claude-code','codex','opencode','hermes','grok-cli'));

-- 2. agent_events: one row per inbox record seen by the fold (dedupe + audit), 7 days ----
-- events_7d (clients.status) = COUNT(*) WHERE client = ? AND test = 0
--   AND outcome IN ('folded','no_life') AND at_ms > now - 7 days.
-- Dropped rows (off, old, child, test) never count.
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
  -- (WP0) dropped_test: a test record while no ready check is running (late or replayed).
  outcome TEXT NOT NULL CHECK (outcome IN ('folded','dropped_off','dropped_old','dropped_child',
    'test','dropped_test','no_life')),
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
  -- sentinel instead of NULL, so the caps simply stop counting the row), except that an
  -- earn from the last 24 h keeps ceil_hour + 1 ms until it leaves the day (notes.md §7).
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

-- 6. client_hooks: one row per supported client ------------------------------------------
-- clients.status upserts all five rows on every run (file-only detection), so the table is
-- empty only before the first Helpers/Connect-card open; Connect/Disconnect jobs and the fold
-- upsert their own client's row. Rows are never deleted. Exports list every row present.
CREATE TABLE client_hooks (
  client TEXT PRIMARY KEY NOT NULL CHECK (client IN ('claude-code','codex','opencode','hermes','grok-cli')),
  -- (WP0) written by clients.status: binary + config dir found, the detected version and when.
  installed INTEGER NOT NULL DEFAULT 0 CHECK (installed IN (0,1)),
  detected_version TEXT CHECK (detected_version IS NULL OR length(detected_version) <= 64),
  checked_at_ms INTEGER,
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
  -- job ids are 'job-' + 24 lowercase hex (protocol.json job_id_pattern).
  job_id TEXT CHECK (job_id IS NULL OR (length(job_id) = 28 AND substr(job_id, 1, 4) = 'job-'
    AND substr(job_id, 5) NOT GLOB '*[^0-9a-f]*')),
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
--     with a journal note: for a living life with death_pending = 1, INSERT INTO life_events
--     (life_id, kind, dedup_key, data, at_ms) VALUES (<life>, 'recovered', 'v5_migration',
--     '{"reason":"v5_migration"}', <migration time>) - a kind the v4 CHECK already allows, so
--     life_events is not rebuilt; normal settlement re-evaluates the life afterwards;
--   * zero pet_care.work_sessions, execution_ms, rest_required, starts_since_bathroom;
--   * lives, journal, memorials and historic tables untouched.
--
-- 8. Evolution by treats (WP1, treats.give): the generalized life::record_evolution journals
--    each crossed stage exactly as v4 does - kind 'evolved', dedup_key 'stage:<n>' (UNIQUE per
--    life, so a stage is journaled once whatever earned it), data {"from","to","xp","level",
--    "source":"treat"}, task_id NULL, request_id = the treats.give request id, seen_at_ms NULL
--    (it appears in the export's unseen_events until pet.events_seen).
