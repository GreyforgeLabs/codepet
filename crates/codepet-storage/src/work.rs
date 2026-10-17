//! Accepted CodePet jobs share the pet transaction, but never the retired mock tables.
//! Permanent `work.accept` receipts are the reward ledger: their primary key is the job
//! id, so a crash between project application and reward publication cannot double XP.
use crate::{care, life, lifecycle_store, store, Result, Snapshot, StorageError, Store};
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkReadiness {
    pub ready: bool,
    pub reason: String,
    pub energy: u32,
    pub sleeping: bool,
    pub sleep_remaining_ms: i64,
}

pub fn work_readiness(s: &Snapshot) -> WorkReadiness {
    let used = (s.care.work_sessions * 100 / care::SESSION_LIMIT)
        .max((s.care.execution_ms.max(0) * 100 / care::EXECUTION_LIMIT_MS) as u32);
    let reason = if !s.enabled {
        "Turn CodePet on first"
    } else if s.life.is_none() {
        "Choose your pet first"
    } else if s.care.sleeping {
        "Your pet is napping"
    } else if used >= 100 {
        "Time for a two-minute nap"
    } else if s
        .life
        .as_ref()
        .is_some_and(|l| l.fainted || l.death_pending)
    {
        "Give your pet a snack or drink to help it recover"
    } else if s.meters.mental_health < 2500 {
        "A little play will help"
    } else {
        "Ready for a job"
    };
    WorkReadiness {
        ready: reason == "Ready for a job",
        reason: reason.into(),
        energy: 100 - used.min(100),
        sleeping: s.care.sleeping,
        sleep_remaining_ms: if s.care.sleeping {
            s.care.sleep_remaining_ms
        } else {
            0
        },
    }
}

#[derive(Serialize, Deserialize)]
struct Reward {
    life_id: String,
    amount: u32,
    kind: String,
}

impl Store {
    /// A single durable marker freezes attended decay through execution and review.
    /// The supervisor clears it on failure/cancellation; recovery clears interrupted
    /// jobs. Keeping the marker on uncertainty can only under-charge the companion.
    pub fn set_work_active(&mut self, task_id: Option<&str>) -> Result<()> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM request_results WHERE method='work.active')",
            [],
            |r| r.get(0),
        )?;
        if task_id.is_none() && !exists {
            return Ok(());
        }
        let (tx, at, _) = self.write_settled()?;
        match task_id {
            Some(id) => {
                if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(StorageError::InvalidParams("invalid work id"));
                }
                tx.execute("INSERT INTO request_results(request_id,method,params_sha256,response,created_at_ms) VALUES('work.active','work.active',?1,?2,?3) ON CONFLICT(request_id) DO UPDATE SET response=excluded.response, params_sha256=excluded.params_sha256, created_at_ms=excluded.created_at_ms",
                    params![id, serde_json::json!({"task_id":id}).to_string(),at.wall])?;
            }
            None => {
                tx.execute("DELETE FROM request_results WHERE request_id='work.active' AND method='work.active'",[])?;
            }
        }
        store::seal(&tx, &at)?;
        tx.commit()?;
        Ok(())
    }
    /// The caller has sealed and explicitly accepted a real result. Replays return the
    /// original receipt even if a new companion has since been adopted.
    pub fn accept_work(
        &mut self,
        task_id: &str,
        life_id: &str,
        kind: &str,
        elapsed_ms: i64,
    ) -> Result<Snapshot> {
        if task_id.len() != 64 || !task_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StorageError::InvalidParams("invalid work id"));
        }
        let action = match kind {
            "bug" => "food",
            "research" => "water",
            "build" => "play",
            _ => return Err(StorageError::InvalidParams("invalid work kind")),
        };
        let request_id = format!("work.accept:{task_id}");
        let binding = crate::sha256_hex(format!("{life_id}:{kind}").as_bytes());
        let (tx, at, _) = self.write_settled()?;
        let (_, replayed) = store::replay_or_run::<Reward, _>(
            &tx,
            &request_id,
            "work.accept",
            &binding,
            at.wall,
            || {
                let living = life::active_life(&tx)?.ok_or(StorageError::NoActivePet)?;
                if living.life_id != life_id {
                    return Err(StorageError::LifeNotFound);
                }
                if lifecycle_store::load(&tx)?.death_pending {
                    return Err(StorageError::PetNeedsCare);
                }
                let (old_xp, _) = life::life_xp(&tx, life_id)?;
                store::fill_care(&tx, &at, action, false)?;
                tx.execute("UPDATE pet_care SET work_sessions = MIN(work_sessions + 1, 3), execution_ms = MIN(execution_ms + ?1, ?2) WHERE id = 1",
                    params![elapsed_ms.clamp(0, care::EXECUTION_LIMIT_MS), care::EXECUTION_LIMIT_MS])?;
                care::bump(&tx)?;
                life::record_evolution(
                    &tx,
                    life_id,
                    life::EvolvedBy::Work {
                        request_id: &request_id,
                    },
                    old_xp,
                    20,
                    at.wall,
                )?;
                store::seal(&tx, &at)?;
                Ok(Reward {
                    life_id: life_id.into(),
                    amount: 20,
                    kind: kind.into(),
                })
            },
        )?;
        if !replayed {
            tx.commit()?;
        } else {
            drop(tx);
        }
        self.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManualClock, StatePaths};
    use std::sync::Arc;

    fn fixture() -> (tempfile::TempDir, Store, Arc<ManualClock>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = StatePaths::resolve(|key| match key {
            "XDG_STATE_HOME" => Some(dir.path().join("state").into()),
            "HOME" => Some(dir.path().join("home").into()),
            _ => None,
        })
        .unwrap();
        let clock = Arc::new(ManualClock::new(1_000_000));
        let store = Store::open_with_first_run_placeholder(&paths, clock.clone()).unwrap();
        (dir, store, clock)
    }
    #[test]
    fn accepted_jobs_reward_the_bound_life_once_and_naps_restore_energy() {
        let (_dir, mut s, clock) = fixture();
        let life = s.snapshot().unwrap().life.unwrap().life_id;
        for n in 1..=3 {
            let id = format!("{n:064x}");
            s.accept_work(&id, &life, "bug", 1000).unwrap();
            let duplicate = s.accept_work(&id, &life, "bug", 1000).unwrap();
            assert_eq!(duplicate.life.unwrap().xp, n * 20);
            assert_eq!(duplicate.care.work_sessions, n);
            assert_eq!(duplicate.meters.hunger, 10000);
        }
        let tired = s.snapshot().unwrap();
        assert!(!work_readiness(&tired).ready);
        assert_eq!(work_readiness(&tired).energy, 0);
        s.sleep("nap").unwrap();
        clock.advance(crate::SLEEP_MS);
        let rested = s.snapshot().unwrap();
        assert!(work_readiness(&rested).ready);
        assert_eq!(work_readiness(&rested).energy, 100);
        assert_eq!(rested.life.unwrap().xp, 60);
    }
    #[test]
    fn low_food_and_water_are_not_work_gates_but_mood_sleep_and_off_are() {
        let (_dir, mut s, _) = fixture();
        s.conn
            .execute(
                "UPDATE pet_state SET hunger=100, thirst=100, mental_health=2500",
                [],
            )
            .unwrap();
        assert!(work_readiness(&s.snapshot().unwrap()).ready);
        s.conn
            .execute("UPDATE pet_state SET mental_health=2499", [])
            .unwrap();
        assert!(!work_readiness(&s.snapshot().unwrap()).ready);
        s.care("play", "play").unwrap();
        assert!(work_readiness(&s.snapshot().unwrap()).ready);
        s.sleep("nap").unwrap();
        assert!(!work_readiness(&s.snapshot().unwrap()).ready);
        s.wake("wake").unwrap();
        s.set_enabled("off", false).unwrap();
        assert!(!work_readiness(&s.snapshot().unwrap()).ready);
    }
    #[test]
    fn wrong_life_cannot_be_rewarded_and_start_marker_earns_nothing() {
        let (_dir, mut s, _) = fixture();
        let id = "1".repeat(64);
        s.set_work_active(Some(&id)).unwrap();
        let busy = s.snapshot().unwrap();
        assert_eq!(busy.life.unwrap().xp, 0);
        assert!(life::slot_busy(&s.conn).unwrap());
        assert!(s.accept_work(&id, "another-life", "research", 100).is_err());
        s.set_work_active(None).unwrap();
        assert!(!life::slot_busy(&s.conn).unwrap());
        assert_eq!(s.snapshot().unwrap().life.unwrap().xp, 0);
    }
}
