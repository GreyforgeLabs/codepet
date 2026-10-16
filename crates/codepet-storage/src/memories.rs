//! Kid-facing journal projection. Reading memories never settles time, acknowledges a
//! celebration, or exposes the journal's task ids, request ids or arbitrary data JSON.
use crate::{life, Result, StorageError, Store};
use codepet_core::evolution::stage_for_xp;
use codepet_core::lifecycle::RULES_V1;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    pub seq: i64,
    pub kind: String,
    pub at_ms: i64,
    pub name: String,
    pub appearance: String,
    pub stage: u8,
    pub past: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoriesPage {
    pub entries: Vec<Memory>,
    pub next_before_seq: Option<i64>,
}

impl Store {
    /// All lives, newest first, with an exclusive sequence cursor. A page is at most 20
    /// rows, well below one protocol frame. Only the UI's closed event vocabulary leaves
    /// storage; historical task-pipeline and configuration events stay private.
    pub fn memories(&self, before_seq: Option<i64>, limit: Option<u32>) -> Result<MemoriesPage> {
        let limit = limit.unwrap_or(life::MAX_MEMORIAL_PAGE);
        if limit == 0 || limit > life::MAX_MEMORIAL_PAGE {
            return Err(StorageError::InvalidParams(
                "limit must be between 1 and 20",
            ));
        }
        if before_seq.is_some_and(|seq| seq < 0) {
            return Err(StorageError::InvalidParams(
                "before_seq must not be negative",
            ));
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut stmt = tx.prepare(
            "SELECT e.seq, e.kind, e.at_ms, l.name, l.appearance,
                    l.status IN ('dead','retired'), l.life_id
             FROM life_events e JOIN lives l ON l.life_id = e.life_id
             WHERE e.seq < ?1 AND e.kind IN
               ('born','armed','evolved','appearance_changed','paused','resumed',
                'critical','recovered','fainted','died','retired')
             ORDER BY e.seq DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(
                params![before_seq.unwrap_or(i64::MAX), i64::from(limit) + 1],
                |r| {
                    Ok((
                        Memory {
                            seq: r.get(0)?,
                            kind: r.get(1)?,
                            at_ms: r.get(2)?,
                            name: r.get(3)?,
                            appearance: r.get(4)?,
                            stage: 1,
                            past: r.get(5)?,
                        },
                        r.get::<_, String>(6)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let more = rows.len() > limit as usize;
        let mut entries = Vec::with_capacity(limit as usize);
        for (mut memory, life_id) in rows.into_iter().take(limit as usize) {
            // Use the life's current/final look and stage, as the memorial does. Never
            // deserialize or export arbitrary historic journal data for display.
            memory.stage = stage_for_xp(life::life_xp(&tx, &life_id)?.0, &RULES_V1.steady);
            entries.push(memory);
        }
        let next_before_seq = if more {
            entries.last().map(|e| e.seq)
        } else {
            None
        };
        tx.commit()?;
        Ok(MemoriesPage {
            entries,
            next_before_seq,
        })
    }
}
