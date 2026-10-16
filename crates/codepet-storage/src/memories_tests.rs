use super::*;
use serde_json::json;

#[test]
fn memories_are_bounded_complete_and_do_not_acknowledge_or_export_raw_data() {
    let env = Env::new();
    let store = env.open();
    let id = life_id(&store);
    let c = env.raw();
    for n in 0..45 {
        crate::life::journal(
            &c,
            &id,
            "evolved",
            None,
            &json!({"to":2,"private":"not a display field"}),
            None,
            None,
            n,
        )
        .unwrap();
    }
    crate::life::journal(
        &c,
        &id,
        "configured",
        None,
        &json!({"private":"not a display field"}),
        None,
        None,
        100,
    )
    .unwrap();
    let version = env.count("SELECT state_version FROM meta");
    let seen = env.count("SELECT count(*) FROM life_events WHERE seen_at_ms IS NOT NULL");
    let mut cursor = None;
    let mut seqs = Vec::new();
    loop {
        let page = store.memories(cursor, None).unwrap();
        assert!(page.entries.len() <= 20);
        for e in &page.entries {
            assert!(cursor.is_none_or(|seq| e.seq < seq));
            assert_ne!(e.kind, "configured");
            seqs.push(e.seq);
        }
        let serialized = serde_json::to_string(&page).unwrap();
        for raw in [
            "private",
            "not a display field",
            "old-task",
            "task_id",
            "seen_at_ms",
        ] {
            assert!(!serialized.contains(raw));
        }
        cursor = page.next_before_seq;
        if cursor.is_none() {
            break;
        }
    }
    assert!(seqs.windows(2).all(|p| p[0] > p[1]));
    assert_eq!(
        seqs.len(),
        env.count("SELECT count(*) FROM life_events WHERE kind IN ('born','evolved')") as usize
    );
    assert_eq!(env.count("SELECT state_version FROM meta"), version);
    assert_eq!(
        env.count("SELECT count(*) FROM life_events WHERE seen_at_ms IS NOT NULL"),
        seen
    );
    assert!(store.memories(Some(0), None).unwrap().entries.is_empty());
    for (cursor, limit) in [(Some(-1), None), (None, Some(0)), (None, Some(21))] {
        assert!(matches!(
            store.memories(cursor, limit),
            Err(StorageError::InvalidParams(_))
        ));
    }
}
