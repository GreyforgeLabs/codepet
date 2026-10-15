# Companion lifecycle (v0.2)

This page documents ruleset `lifecycle-v1`: evolution by XP, the humane permadeath lifecycle, attended-time accounting, and how storage enforces them. It is the reference for the storage layer (`crates/codepet-storage`) and for UI copy.

The product decisions behind it (D1 to D12) are recorded in `docs/decisions/0001-v0.2-lifecycle.md`. They are delegated defaults for a private testing prerelease.

**Verification status.** Everything below about storage is covered by the `codepet-storage` unit tests (`life_tests.rs`, `lifecycle_tests.rs`, `presence_tests.rs`, `bar_export_tests.rs`, `migration_v4_tests.rs`). The tests exercise the documented SQLite behaviour with a manual clock. They do not exercise the bar plugin's recorder on a live Omarchy session, the CLI wiring, or the QML UI. Those have their own tests and live QA, recorded in `docs/validation.md`.

## Rules (`lifecycle-v1`)

Every number is a ruleset value from `codepet_core::lifecycle::RULES_V1`. Its canonical JSON (both paces, fixed key order) is stored in the `rulesets` table with its SHA-256 and is what `lifecycle.rules` returns. Each life copies its effective parameters into its own row when it is adopted or migrated.

| Parameter | `steady` (default) | `playtest` |
|---|---|---|
| Decay per attended hour [food, water, play, wash] | 10, 12.5, 6, 5 points | 200, 250, 120, 100 points |
| Satiety hold after care fills a need | 1 h | 3 min |
| Warning when any meter is at or below | 30 | 30 |
| Lethal needs | hunger, thirst | hunger, thirst |
| Critical after a lethal meter has been empty for | 2 h | 6 min |
| Grace once critical (frozen at entry) | 6 h | 18 min |
| Credit cap per settlement | 8 h | 8 h |
| Presence tick / valid gap / idle timeout | 60 s / 50 to 90 s / 180 s | same |
| Level XP thresholds (L1 to L5) | 0, 80, 200, 400, 700 | same |
| Stage minimum levels (young, grown, mature) | 1, 2, 4 | same |

Mood and hygiene only ever warn. Bathroom and nap requirements still block Start, as in v0.1, and are never neglect.

All durations are **attended (active) time**: time the bar plugin observed you at the computer while the companion's clock was running. Nothing counts offline or overnight.

### Worst case under `steady`

These are attended hours after the last water or food, with every warning ignored.

| Need | Warning | Empty | Critical | End (permadeath) |
|---|---|---|---|---|
| Thirst | 6.6 h | 9 h | 11 h | 17 h |
| Hunger | 8 h | 11 h | 13 h | 19 h |

Under `playtest`, thirst empties after 27 minutes, turns critical at 33 minutes and the life ends at about 51 minutes.

### Leniency (`pet.configure_life`)

A living companion's settings can only become more lenient:

- a longer grace;
- a longer critical-after period;
- slower decay (`decay_percent` 25 to 100 scales the current rates down).

A stricter value is refused. If a raised critical-after period would exceed the grace, the grace is raised to match in the same transaction. A frozen grace that is already running becomes `max(frozen, new)`, so a deadline can only move later. Pace is chosen at adoption and never changes; triggers refuse any change to a life's pace, ruleset, species, ordinal, origin or adoption time.

## Evolution

Stage is always derived from the life's current XP. XP is per life: exactly the sum of that life's `demo_xp_rewards` rows. Account progression (`state.get` `progression`) keeps its lifetime meaning and is unchanged for existing data.

- Young: level 1 (0 XP). Grown: level 2 (80 XP, four accepted mock tasks). Mature: level 4 (400 XP, twenty accepted mock tasks).
- An acceptance that crosses a stage journals `evolved` once per life and stage (`dedup_key = 'stage:<n>'`). A replay or a duplicate acceptance never journals again.
- The UI that performed the acceptance celebrates from its response (`receipt.evolved_to`) and then sends `pet.events_seen`. Other UIs see the event in `unseen_events` and celebrate on the next explicit interaction. `seen_at_ms` is durable and can be set only once, so a restart never replays a celebration.
- If XP is ever reversed (no v0.2 path does this), the derived stage drops and the journal is kept.
- Changing the companion's look (`pet.select`) is cosmetic. It never touches XP, needs, grace, sleep, tasks or approvals.

## States

```text
 (no living life) ──pet.adopt (explicit, acknowledged)──► healthy
 migration ─────────────────────────────────────────────► healthy|warning (unarmed: no decay until pet.rules)

 healthy ──meter ≤ 30, or a lethal meter empty──► warning ──lethal need empty for critical-after──► critical
    ▲                                               │ (rest of that interval discarded; grace frozen)
    └──────────── matching care ◄───────────────────┴──── matching care clears every critical need

 critical ──grace elapsed──┬─ gentle ─────► critical + fainted (Start blocked; care revives)
                           └─ permadeath ─┬─ slot free ─► ended (memorial; no living companion)
                                          └─ slot busy ─► critical + pending ──slot freed──► ended
 healthy|warning ──pet.retire (typed name, slot free)──► retired (memorial)
 ended and retired lives are immutable history and are never revived
```

| State | Care, sleep, wake | Pause | Preflight and Start | Change look | Retire | Review, accept, discard, cancel, recover |
|---|---|---|---|---|---|---|
| healthy, warning | yes | yes | existing gates | yes | yes | yes |
| critical (including fainted) | yes | yes | refused | yes | no | yes |
| pending end | no | no | refused | no | no | yes (the job is preserved) |
| no living companion | `NO_ACTIVE_PET` | `NO_ACTIVE_PET` | `NO_ACTIVE_PET` | `NO_ACTIVE_PET` | n/a | yes |

The Start gate checks, in order: no living companion, a pending end, a critical need. The existing sleep, bathroom and meter checks follow. `care.readiness_reasons` carries the matching plain-text reason first.

### What care does

The matching care action (or a routine that fills the need) sets the meter to full, starts that need's satiety hold and clears that need's neglect. Unrelated care changes nothing else. A critical period clears only when no lethal need is still past its critical-after period. Caring for one of two critical needs moves the critical record to the other need and keeps the elapsed time and the frozen grace: no fresh grace is ever granted.

A Start debit ends every satiety hold. A refund that restores a meter to full gives that need its hold back.

### Mode and arming

- A life's clock is **unarmed** until its rules are acknowledged (`pet.rules` with `acknowledge_rules`, or adoption). Unarmed means no decay, no neglect and no countdown. Migration and first run create life #1 unarmed.
- Permadeath to gentle is allowed at any time while the companion is living and not pending. Gentle to permadeath needs the acknowledgement and a healthy companion. Neglect, carries and holds are never reset by a mode change.
- Pause/Travel (`pet.pause`) freezes decay, neglect and grace, grants nothing, refills nothing and does not block care or Start. Resuming restores nothing.

## Time accounting

### The recorder (bar plugin)

Only the bar plugin's leader instance records presence (`omarchy-plugin/PresenceRecorder.qml`). It credits 60-second ticks whose wall gap is 50 to 90 seconds, only after 180 seconds of continued activity, and only while `bar-state.json` says `presence.wanted` is true. It writes `$XDG_STATE_HOME/codepet/presence.json`:

```json
{"v":1,"session":"bar-<id>","life_id":"life-<24 hex>","started_wall_ms":0,"updated_wall_ms":0,"attended_ms":0,"ticks":0}
```

The overlay and the standalone host record nothing. With the bar widget absent, time is frozen.

### Settlement (storage)

Every write transaction, and every `state.get` that has something to settle, runs `lifecycle_store::settle_presence` first, under the `BEGIN IMMEDIATE` lock, with the time sampled after the lock:

1. **Clock guard.** Without a CLOCK_BOOTTIME reading, nothing is credited or consumed. Otherwise the cap is the boot time elapsed since the previous settlement plus 60 s (same boot) or 8 h (first settlement of a boot), never more than 8 h. The cursor is then moved to now.
2. **The file.** It must be a regular file (never a symlink), owned by the effective UID, not group- or other-writable, at most 2048 bytes, and its `(dev, inode)` must match between `lstat` and the opened handle. It is parsed strictly (unknown fields refused) and validated by `codepet_core::presence::validate`. A missing file does nothing. Any other refusal increments `meta.presence_rejections`, charges nothing, and the command still succeeds.
3. **Ledger.** `presence_sessions` is append-only and never pruned (a trigger refuses deletes). Each session's attended time is consumed exactly once, so a replayed, restored or old file never charges twice. A shrinking session counts as a rejection.
4. **Cap.** The unconsumed time is credited up to the cap and the rest is discarded.
5. **Blockers.** Everything is discarded when: no companion is living, the file names another life, the life is unarmed, Pause/Travel is on, a task holds the slot, or an end is pending. Every state change is itself a settling write, so the state at settlement held for the whole interval.
6. **Nap.** During a nap, the remaining nap time is discarded and the rest credited. The nap state is read before `settle_sleep` runs, so a finished nap never freezes the clock.
7. **Charge.** `codepet_core::lifecycle::advance` charges the credit. Meters go to `pet_state`, clock state to `life_vitals`, and the status is re-derived. Entering critical, fainting and a due end are journaled. A meter change bumps the care revision, which invalidates pending approvals as care already did.

The ledger records `credited_ms` (what was charged) and `discarded_ms` (capped, blocked, napping, or discarded by `advance` when critical was entered), and their sum always equals `consumed_ms`.

**Clock safety.** A wall-clock jump, DST or time-zone change cannot exceed the boot-time cap and is rejected as a future-dated file when the wall clock moves back. A reboot loses at most the uncommitted tail. A missing `/proc`, a missing or corrupt file, or a foreign owner freezes or discards time. XP is never time-based.

**Residual risk.** A process running as the same user can forge `presence.json` within these bounds, and deleting the file avoids decay. This is a convenience clock, not a tamper-proof economy.

## Transaction order

Every write transaction runs, in this order:

1. settle presence;
2. settle sleep;
3. the command body;
4. `life::finalize_if_due` (a pending end becomes final once no task holds the slot);
5. `meta.state_version += 1`;
6. commit.

`Store::write()` does steps 1 and 2. `store::finish()` (or `seal()` plus commit, when an outcome must reflect the end) does steps 4 to 6. As a result, XP granted by an acceptance always lands before an end finalized in the same transaction. The end is guarded by `UPDATE lives … WHERE status = 'critical'`, so it happens exactly once.

When steps 1 and 2 themselves run a grace out with the slot free, the end is final whatever the command body would decide. `Store::write()` then runs steps 4 to 6 at once, commits the end on its own, and continues the command in a fresh transaction (re-sampling time under the writer lock). A body that then refuses cannot roll the end back. Care, sleep, pause, rules, appearance, leniency and retirement refuse with `NO_ACTIVE_PET` and the message that the companion passed away; an adoption sent in that same request is refused the same way, so the memorial is seen first. The CLI republishes `bar-state.json` after a refused write as well as after a successful one.

`state.get` keeps its plain-read fast path when nothing needs settling: no nap, no live supervised execution, no presence file, and no end that is due. Otherwise it takes the writer lock, runs the same prologue and epilogue, and may commit a settlement or an end.

An ending touches only `lives`, `life_events`, `life_vitals`, `meta`, the presence tables, the same transaction's `pet_state` meters and the care revision (`pet_care.revision` and pending `preflight_tokens`). Tasks, events, artifacts, checks, acceptances, rewards, demo XP rows, receipts, settings and project folders are never changed. The lifecycle modules contain no `DELETE FROM` (a test checks this).

## Storage

### Schema v4

The lifecycle migration is **schema v4**, because v3 is the `review_repairs` audit table. It is additive. No existing row is deleted and no meter changes. Before migrating, `VACUUM INTO codepet.sqlite3.bak-v<old>-<ms>` backs up the database. A newer or corrupt database is refused unchanged, and a failure mid-migration rolls back to the old version.

New tables:

- `meta`: `state_version`, `presence_rejections`.
- `rulesets`: canonical `lifecycle-v1` JSON and its SHA-256.
- `lives`: one row per life, with copied rules, mode, pace, arming, status and the memorial summary. One living life at most (partial unique index). Triggers make ended and retired lives immutable, refuse deletes, and freeze identity, pace and ruleset.
- `life_vitals`: the living companion's holds, carries, neglect, critical period, fainted, pending and attended total.
- `presence_clock`: the boot-time cursor.
- `presence_sessions`: the append-only ledger.
- `life_events`: the journal (`born`, `armed`, `mode_changed`, `configured`, `health_changed`, `evolved`, `appearance_changed`, `paused`, `resumed`, `critical`, `recovered`, `fainted`, `death_pending`, `died`, `retired`). Rows cannot be deleted, and only `seen_at_ms` may be set, once.
- `tasks.life_id` and `demo_xp_rewards.life_id`: the performing life.

The migration also does the following:

- It creates life #1, "Ginger" the cat, unarmed, `steady`, `permadeath` preselected. Its origin is `migrated` when any task or demo XP exists, otherwise `first_run`. Its status is `warning` when a meter is at or below 30.
- It attaches every historic task and demo XP row to life #1, so its XP and level equal the old totals.
- It journals already reached stages as seen history.
- It records the presence cursor, so presence from before the upgrade is never charged.

Legacy `settings.decay_enabled` and `decay_per_hour` remain readable but are ignored. `pet_state.decay_paused` now means Pause/Travel.

### Store API

| Method | Errors |
|---|---|
| `select_appearance(request_id, life_id, pet_id) -> LifeOutcome` | `NO_ACTIVE_PET`; `STALE_REVISION` (another life); `INVALID_STATE` (pending); `INVALID_PARAMS` (unknown look) |
| `adopt(request_id, &AdoptRequest{species, name, mode, pace, acknowledge_rules}) -> LifeOutcome` | `INVALID_STATE` (a living companion or a busy slot); `ACKNOWLEDGEMENT_REQUIRED`; `INVALID_PARAMS` |
| `set_rules(request_id, life_id, mode, acknowledge_rules) -> LifeOutcome` | `ACKNOWLEDGEMENT_REQUIRED` (arming, or switching to permadeath); `INVALID_STATE` (permadeath while not healthy, or pending) |
| `configure_life(request_id, life_id, expected_revision, &Leniency) -> LifeOutcome` | `STALE_REVISION`; `INVALID_PARAMS` ("settings can only become more lenient for a living companion"); `INVALID_STATE` (pending) |
| `retire(request_id, life_id, confirm_name) -> LifeOutcome` | `INVALID_PARAMS` (name mismatch); `INVALID_STATE` (busy slot, critical, pending) |
| `events_seen(request_id, life_id, through_seq) -> LifeOutcome` | `STALE_REVISION` (unknown life) |
| `memorial(before_ordinal, limit) -> MemorialPage` | `INVALID_PARAMS` (limit outside 1 to 20); read-only |
| `export_bar_state() -> bool` | `false` when the file is already as new |

Every mutating method is idempotent by the envelope request id. The same id with the same parameters returns the stored outcome, and with different parameters it returns `IDEMPOTENCY_CONFLICT`. Frequent selection, acknowledgement, rules and leniency receipts are bounded like care receipts. Adoption, retirement and acceptance receipts are kept.

`pet.care`, `pet.sleep` and `pet.wake` return `NO_ACTIVE_PET` without a living companion and `INVALID_STATE` for a pending end. `CareOutcome` carries the resulting `lifecycle`. `pet.pause` returns `NO_ACTIVE_PET` or `INVALID_STATE` the same way, and its result adds `life_id`.

Acceptance receipts add `life_id`, `level_before`, `level_after`, `evolved_to` and `xp_withheld` (`"life_ended"` when an eligible reward could not be given because the performing life had ended). Receipts are rebuilt from durable rows, so a fresh acceptance, a duplicate under another request id and `task.get` agree. Receipts stored before v0.2 replay unchanged, and the new fields take their serde defaults.

### `state.get` additions

`schema_version` is 4. The snapshot adds `state_version`, `life` (or `null`), `lifecycle` (ruleset, frozen reason, attended total, the four needs with attended-time windows, the critical period and the effective rules), `unseen_events` and `memorial` (`count` and the latest summary). The frozen reason is one of `none`, `no_life`, `pending`, `unarmed`, `paused`, `task` or `sleeping`. `until_end_ms` is `null` for gentle or unarmed lives.

## `bar-state.json`

`Store::export_bar_state()` writes `$XDG_STATE_HOME/codepet/bar-state.json` (mode 0600, at most 16 KiB). The file contains a bar-safe subset:

- schema `codepet.bar.v1`, `state_version`, `db_id`, app version and ruleset;
- the companion's look, stage, level and status;
- the meters, the need windows (`until_warning_ms`, `until_critical_ms` and `until_end_ms` per need, in active time) and the critical period;
- the care summary, with at most 8 readiness reasons of at most 160 bytes each;
- the slot task's state and category only;
- `presence` (`wanted`, `life_id`, tick and idle timeout), the unseen evolutions and the memorial count with the latest name.

It never contains task instructions, folders, artifacts, logs or provider error text.

The export takes the writer lock, reads committed rows without settling, and skips the write when the existing file already holds the same or a newer `state_version` **of the same database**. `db_id` names the database (a digest of its first life's random id, which is created with the database and never changes); `state.get` reports the same `db_id`. A file from another database, or one without `db_id`, is always replaced, so restoring a backup, deleting and recreating the database, or switching state copies never pins the bar to the previous database's pet until the counter catches up. Readers (the overlay's `BarStateWatcher` and `HostAdapter`) likewise order versions only within one `db_id`. Otherwise it creates a fresh temporary file exclusively (never following a symlink; a random suffix, so a stale temporary file left by a crash cannot block it), syncs it and renames it while the lock is held. The file therefore only moves forward. Readers never launch Rust in response to a change of this file.

`presence.wanted` is Rust's half of the recorder's eligibility: a living, armed, unpaused companion, no task in the slot and no pending end. Sleep deliberately does not clear it.

## Copy rules

These rules apply to every user-facing string about the lifecycle (bar, overlay, standalone UI, CLI messages):

- Calm and factual. No sounds, flashing, notifications or blame, and no daily counters.
- Every duration is labelled "active time".
- Predictions never show an end. Past a prediction window, say "Ginger may need you urgently. Open CodePet to check."
- Name the need and the action that helps.

Templates:

- **Warning:** "Ginger is getting thirsty. Water will help. About 2 h 40 min of active time before it becomes serious."
- **Critical (permadeath):** "Ginger urgently needs water. Care now keeps them safe. Without water, Ginger may pass away after about 5 h 10 min of active time."
- **Fainted:** "Ginger fainted from thirst. Water will revive them; work waits until then."
- **Grace run out, task still open (`death_pending`):** "Ginger's grace time has run out. They will pass away when the current task is finished. The task and its result are kept safe: review, accept, discard or cancel it as usual. New work waits until then." The life panel and the bar tooltip use exactly these words (`LifeText.pendingText`; `test_bar_model.mjs` checks the bar copy matches). The companion has not passed away yet, so no surface says it has.
- **Ended:** "Ginger has passed away. They reached level 3 as a Desk Cat and helped with 14 accepted tasks. Your work, tasks and history are unchanged. You can adopt a new companion whenever you're ready."
- **Frozen clock:** "Paused: time isn't counting (on a break / task running / not set up)."

Storage readiness reasons follow the same rules, for example "Ginger urgently needs water; care before starting work" and "Adopt a companion to start work".

## Limits

- Neglect accrues only while the bar widget runs on Omarchy and the user is active. With the overlay alone, or on another desktop, time is frozen.
- An end settles only on the next settling command: opening the popup, care, Refresh, the CLI, a task action or bar startup.
- Presence is not tamper-proof (see the residual risk above).
- Evolution is driven by demo (mock) XP only, and no capability grows with stage or level.
