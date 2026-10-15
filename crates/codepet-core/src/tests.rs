use super::*;
fn draft() -> Draft {
    Draft {
        category: Category::MentalHealth,
        provider: Provider::Mock,
        folder: "/example/project".into(),
        scope: vec!["src".into()],
        instructions: "Explain the task state machine".into(),
    }
}
fn running(engine: &mut Engine) -> JobId {
    let id = engine.create(draft()).unwrap();
    let token = engine.preflight(id).unwrap();
    engine.start(id, token).unwrap();
    engine.mock_prepared(id).unwrap();
    id
}
fn review(engine: &mut Engine, verification: Verification) -> (JobId, u64) {
    let id = running(engine);
    engine.mock_exited(id, true).unwrap();
    let digest = engine
        .mock_review(id, "Mock explanation artifact".into(), verification)
        .unwrap();
    (id, digest)
}
#[test]
fn idle_never_starts() {
    let mut e = Engine::default();
    assert_eq!(e.active_job(), None);
    assert!(!e.settings().decay_enabled);
    for c in [
        Category::Hunger,
        Category::Thirst,
        Category::MentalHealth,
        Category::Hygiene,
    ] {
        assert_eq!(e.meters().hundredths(c), 7000);
    }
    let id = e.create(draft()).unwrap();
    e.preflight(id).unwrap();
    assert_eq!(e.task(id).unwrap().state, State::Draft);
    assert_eq!(e.active_job(), None);
}
#[test]
fn changed_draft_invalidates_start() {
    let mut e = Engine::default();
    let id = e.create(draft()).unwrap();
    let token = e.preflight(id).unwrap();
    e.update(id, 1, draft()).unwrap();
    assert_eq!(e.start(id, token), Err(Error::InvalidToken));
    assert_eq!(e.update(id, 1, draft()), Err(Error::StaleRevision));
}
#[test]
fn changed_settings_invalidate_all_preflights() {
    let mut e = Engine::default();
    let first = e.create(draft()).unwrap();
    let second = e.create(draft()).unwrap();
    let first_token = e.preflight(first).unwrap();
    let second_token = e.preflight(second).unwrap();
    e.set_settings(Settings {
        timeout_seconds: 7200,
        ..Settings::default()
    })
    .unwrap();
    assert_eq!(e.start(first, first_token), Err(Error::InvalidToken));
    assert_eq!(e.start(second, second_token), Err(Error::InvalidToken));
    assert_eq!(e.active_job(), None);
    assert_eq!(e.task(first).unwrap().state, State::Draft);
    let fresh = e.preflight(first).unwrap();
    e.start(first, fresh).unwrap();
}

#[test]
fn rejected_settings_preserve_preflight_and_settings() {
    let mut e = Engine::default();
    let id = e.create(draft()).unwrap();
    let token = e.preflight(id).unwrap();
    assert_eq!(
        e.set_settings(Settings {
            timeout_seconds: 0,
            ..Settings::default()
        }),
        Err(Error::InvalidSettings)
    );
    assert_eq!(e.settings(), &Settings::default());
    e.start(id, token).unwrap();
}
#[test]
fn superseded_preflight_is_invalid() {
    let mut e = Engine::default();
    let id = e.create(draft()).unwrap();
    let old = e.preflight(id).unwrap();
    let new = e.preflight(id).unwrap();
    assert_eq!(e.start(id, old), Err(Error::InvalidToken));
    e.start(id, new).unwrap();
}
#[test]
fn token_cannot_cross_engine_or_job() {
    let mut a = Engine::default();
    let mut b = Engine::default();
    let x = a.create(draft()).unwrap();
    let y = b.create(draft()).unwrap();
    let t = a.preflight(x).unwrap();
    b.preflight(y).unwrap();
    assert_eq!(b.start(y, t), Err(Error::InvalidToken));
    let z = a.create(draft()).unwrap();
    let t = a.preflight(x).unwrap();
    a.preflight(z).unwrap();
    assert_eq!(a.start(z, t), Err(Error::InvalidToken));
}
#[test]
fn unsupported_providers_fail_closed() {
    for p in [
        Provider::Codex,
        Provider::OpenCode,
        Provider::Hermes,
        Provider::ClaudeCode,
    ] {
        let mut e = Engine::default();
        let mut d = draft();
        d.provider = p;
        let id = e.create(d).unwrap();
        assert!(matches!(e.preflight(id), Err(Error::UnsupportedProvider)));
        assert_eq!(e.active_job(), None);
    }
}
#[test]
fn single_slot_including_wait_review_cancel() {
    let mut e = Engine::default();
    let id = running(&mut e);
    let other = e.create(draft()).unwrap();
    let token = e.preflight(other).unwrap();
    e.mock_wait(id).unwrap();
    assert_eq!(e.start(other, token.clone()), Err(Error::Busy));
    e.mock_continue_after_user(id).unwrap();
    e.mock_exited(id, true).unwrap();
    e.mock_review(id, "result".into(), Verification::Passed)
        .unwrap();
    assert_eq!(e.start(other, token.clone()), Err(Error::Busy));
    e.discard(id).unwrap();
    e.start(other, token).unwrap();
    e.cancel(other).unwrap();
    assert_eq!(e.active_job(), Some(other));
    e.mock_confirm_stopped(other).unwrap();
    assert_eq!(e.active_job(), None);
}
#[test]
fn acceptance_is_explicit_and_exactly_once() {
    let mut e = Engine::default();
    let (id, hash) = review(&mut e, Verification::Passed);
    assert_eq!(e.meters().hundredths(Category::MentalHealth), 7000);
    assert_eq!(
        e.accept_artifact(id, hash, false, None),
        Err(Error::AcknowledgementRequired)
    );
    assert_eq!(
        e.accept_artifact(id, hash + 1, true, None),
        Err(Error::ReviewChanged)
    );
    let receipt = e.accept_artifact(id, hash, true, None).unwrap();
    assert_eq!(e.accept_artifact(id, hash, true, None).unwrap(), receipt);
    assert_eq!(e.meters().hundredths(Category::MentalHealth), 9000);
    assert_eq!(e.meters().hundredths(Category::Hunger), 7000);
    assert_eq!(e.active_job(), None);
}
#[test]
fn failed_or_missing_checks_need_override() {
    for verification in [Verification::Failed, Verification::NotVerified] {
        let mut e = Engine::default();
        let (id, hash) = review(&mut e, verification);
        for reason in [None, Some(" ".into()), Some("x".repeat(513))] {
            assert_eq!(
                e.accept_artifact(id, hash, true, reason),
                Err(Error::OverrideRequired)
            );
        }
        let receipt = e
            .accept_artifact(
                id,
                hash,
                true,
                Some("Useful explanation; no executable checks".into()),
            )
            .unwrap();
        assert_eq!(receipt.verification, verification);
        assert!(receipt.override_reason.is_some());
    }
}
#[test]
fn meter_reward_clamps() {
    let mut e = Engine::default();
    for _ in 0..3 {
        let (id, h) = review(&mut e, Verification::Passed);
        e.accept_artifact(id, h, true, None).unwrap();
    }
    assert_eq!(e.meters().hundredths(Category::MentalHealth), 10000);
}
#[test]
fn cancel_is_idempotent_and_does_not_reward() {
    let mut e = Engine::default();
    let id = running(&mut e);
    e.cancel(id).unwrap();
    e.cancel(id).unwrap();
    assert_eq!(e.mock_exited(id, true), Err(Error::InvalidTransition));
    e.mock_confirm_stopped(id).unwrap();
    e.cancel(id).unwrap();
    assert_eq!(e.meters(), &Meters::default());
    assert_eq!(e.task(id).unwrap().state, State::Cancelled);
    assert!(e.preflight(id).is_err());
}
#[test]
fn failure_and_discard_have_no_reward() {
    let mut e = Engine::default();
    let id = running(&mut e);
    e.mock_exited(id, false).unwrap();
    assert_eq!(e.task(id).unwrap().state, State::Failed);
    assert_eq!(e.active_job(), None);
    let (id, h) = review(&mut e, Verification::Passed);
    e.discard(id).unwrap();
    assert_eq!(
        e.accept_artifact(id, h, true, None),
        Err(Error::InvalidTransition)
    );
    assert_eq!(e.meters(), &Meters::default());
}
#[test]
fn recovery_interrupts_without_restart() {
    for phase in 0..5 {
        let mut e = Engine::default();
        let id = e.create(draft()).unwrap();
        let token = e.preflight(id).unwrap();
        e.start(id, token).unwrap();
        if phase > 0 {
            e.mock_prepared(id).unwrap();
        }
        if phase == 2 {
            e.mock_wait(id).unwrap();
        }
        if phase == 3 {
            e.mock_exited(id, true).unwrap();
        }
        if phase == 4 {
            e.cancel(id).unwrap();
        }
        let e = e.recover_snapshot();
        assert_eq!(e.task(id).unwrap().state, State::Interrupted);
        assert_eq!(e.active_job(), None);
        assert_eq!(e.meters(), &Meters::default());
    }
}
#[test]
fn recovery_blocks_unvalidated_review() {
    let mut e = Engine::default();
    let (id, h) = review(&mut e, Verification::Passed);
    let mut e = e.recover_snapshot();
    assert_eq!(e.task(id).unwrap().state, State::RecoveryRequired);
    assert_eq!(e.active_job(), Some(id));
    assert_eq!(
        e.accept_artifact(id, h, true, None),
        Err(Error::InvalidTransition)
    );
    e.mock_abandon_recovery(id).unwrap();
    assert_eq!(e.active_job(), None);
}
#[test]
fn recovery_preserves_receipt_without_second_reward() {
    let mut e = Engine::default();
    let (id, h) = review(&mut e, Verification::Passed);
    let receipt = e.accept_artifact(id, h, true, None).unwrap();
    let mut e = e.recover_snapshot();
    assert_eq!(e.accept_artifact(id, h, true, None).unwrap(), receipt);
    assert_eq!(e.meters().hundredths(Category::MentalHealth), 9000);
}
#[test]
fn recovery_invalidates_old_start_tokens() {
    let mut e = Engine::default();
    let id = e.create(draft()).unwrap();
    let old = e.preflight(id).unwrap();
    let mut e = e.recover_snapshot();
    assert_eq!(e.start(id, old.clone()), Err(Error::InvalidToken));
    e.preflight(id).unwrap();
    assert_eq!(e.start(id, old), Err(Error::InvalidToken));
}
#[test]
fn events_are_ordered_job_scoped_and_bounded() {
    let mut e = Engine::default();
    let id = running(&mut e);
    let events = e.events_after(0).unwrap();
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|ev| ev.job_id == id));
    assert!(events.windows(2).all(|w| w[1].seq == w[0].seq + 1));
    for _ in 0..MAX_EVENTS {
        e.create(draft()).unwrap();
    }
    assert_eq!(e.events_after(0), Err(Error::ResyncRequired));
    assert_eq!(e.events_after(u64::MAX), Err(Error::ResyncRequired));
    assert_eq!(e.events.len(), MAX_EVENTS);
}
#[test]
fn task_capacity_is_bounded_without_history_deletion() {
    let mut e = Engine::default();
    for _ in 0..MAX_TASKS {
        e.create(draft()).unwrap();
    }
    assert_eq!(e.create(draft()), Err(Error::Capacity));
    assert!(e.task(1).is_ok());
}
#[test]
fn validates_paths_and_inputs() {
    for folder in [
        "/",
        "relative",
        "/proc/1",
        "/sys",
        "/dev/null",
        "/ok/../bad",
    ] {
        let mut d = draft();
        d.folder = folder.into();
        assert_eq!(d.validate(), Err(Error::InvalidPath));
    }
    for scope in ["", "..", "../secrets", "/etc", "src/../../etc", "."] {
        let mut d = draft();
        d.scope = vec![scope.into()];
        assert_eq!(d.validate(), Err(Error::InvalidPath));
    }
    let mut d = draft();
    d.instructions = "x".repeat(MAX_PROMPT_BYTES + 1);
    assert_eq!(d.validate(), Err(Error::InvalidDraft));
    d.instructions = "ok".into();
    assert!(d.validate().is_ok());
}
#[test]
fn settings_bounds_are_enforced() {
    let mut e = Engine::default();
    let bad = Settings {
        timeout_seconds: 0,
        ..Settings::default()
    };
    assert_eq!(e.set_settings(bad), Err(Error::InvalidSettings));
    let bad = Settings {
        decay_per_hour: [501, 0, 0, 0],
        ..Settings::default()
    };
    assert_eq!(e.set_settings(bad), Err(Error::InvalidSettings));
    assert_eq!(e.settings(), &Settings::default());
}
#[test]
fn protocol_frames_are_bounded_versioned_utf8_single_line() {
    assert!(validate_frame(1, b"{}\n").is_ok());
    for bytes in [
        &b""[..],
        &b"{}"[..],
        &b"{}\n{}\n"[..],
        &b"\xff\n"[..],
        &b"\0\n"[..],
        &b"\n"[..],
    ] {
        assert_eq!(validate_frame(1, bytes), Err(Error::InvalidFrame));
    }
    assert_eq!(validate_frame(2, b"{}\n"), Err(Error::InvalidFrame));
    assert_eq!(
        validate_frame(1, &vec![b'x'; MAX_FRAME_BYTES + 1]),
        Err(Error::InvalidFrame)
    );
}
#[test]
fn out_of_order_or_wrong_job_events_fail_closed() {
    let mut e = Engine::default();
    let id = e.create(draft()).unwrap();
    assert_eq!(e.mock_exited(id, true), Err(Error::InvalidTransition));
    assert_eq!(e.mock_prepared(999), Err(Error::UnknownJob));
    assert_eq!(e.cancel(id), Err(Error::InvalidTransition));
}
#[test]
fn string_forms_round_trip_and_are_stable() {
    let states: Vec<_> = State::ALL.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        states,
        [
            "draft",
            "preparing",
            "running",
            "waiting_for_user",
            "verifying",
            "review_ready",
            "cancelling",
            "cancelled",
            "failed",
            "interrupted",
            "accepted_artifact",
            "discarded",
            "recovery_required"
        ]
    );
    for s in State::ALL {
        assert_eq!(State::parse(s.as_str()), Some(s));
    }
    let cats: Vec<_> = Category::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(cats, ["hunger", "thirst", "mental_health", "hygiene"]);
    for (i, c) in Category::ALL.into_iter().enumerate() {
        assert_eq!(Category::parse(c.as_str()), Some(c));
        assert_eq!(c.index(), i);
    }
    let providers: Vec<_> = Provider::ALL.iter().map(|p| p.as_str()).collect();
    assert_eq!(
        providers,
        ["mock", "codex", "opencode", "hermes", "claude_code"]
    );
    let labels: Vec<_> = Provider::ALL.iter().map(|p| p.label()).collect();
    assert_eq!(
        labels,
        ["Mock", "Codex", "OpenCode", "Hermes", "Claude Code"]
    );
    for p in Provider::ALL {
        assert_eq!(Provider::parse(p.as_str()), Some(p));
        assert_eq!(p.supported(), p == Provider::Mock);
    }
    let v: Vec<_> = Verification::ALL.iter().map(|v| v.as_str()).collect();
    assert_eq!(v, ["passed", "failed", "not_verified"]);
    for x in Verification::ALL {
        assert_eq!(Verification::parse(x.as_str()), Some(x));
    }
    for bad in [
        "", "Draft", "DRAFT", " draft", "draft\0", "accepted", "applying",
    ] {
        assert_eq!(State::parse(bad), None);
    }
    assert_eq!(Category::parse("mental health"), None);
    assert_eq!(Provider::parse("claude-code"), None);
    assert_eq!(Verification::parse("verified"), None);
}
#[test]
fn transition_table_matches_spec() {
    use State::*;
    let allowed = [
        (Draft, Preparing),
        (Preparing, Running),
        (Preparing, Failed),
        (Preparing, Cancelling),
        (Preparing, Interrupted),
        (Running, WaitingForUser),
        (Running, Verifying),
        (Running, Failed),
        (Running, Cancelling),
        (Running, Interrupted),
        (WaitingForUser, Running),
        (WaitingForUser, Failed),
        (WaitingForUser, Cancelling),
        (WaitingForUser, Interrupted),
        (Verifying, ReviewReady),
        (Verifying, Failed),
        (Verifying, Cancelling),
        (Verifying, Interrupted),
        (Cancelling, Cancelled),
        (Cancelling, Interrupted),
        (ReviewReady, AcceptedArtifact),
        (ReviewReady, Discarded),
        (ReviewReady, RecoveryRequired),
        (RecoveryRequired, Discarded),
    ];
    for from in State::ALL {
        for to in State::ALL {
            assert_eq!(
                State::allowed_transition(from, to),
                allowed.contains(&(from, to)),
                "{from:?} -> {to:?}"
            );
        }
    }
    for s in State::ALL {
        if s.is_terminal() {
            assert!(!s.occupies_slot());
            assert!(State::ALL
                .iter()
                .all(|to| !State::allowed_transition(s, *to)));
        }
        assert!(!State::allowed_transition(s, Draft));
    }
    assert!(!Draft.is_terminal() && !Draft.occupies_slot());
    let terminal: Vec<_> = State::ALL.into_iter().filter(|s| s.is_terminal()).collect();
    assert_eq!(
        terminal,
        [Cancelled, Failed, Interrupted, AcceptedArtifact, Discarded]
    );
    // Review-ready and recovery are not terminal: they still hold the slot.
    assert!(ReviewReady.occupies_slot() && RecoveryRequired.occupies_slot());
}
#[test]
fn pure_reward_helper_is_fixed_and_clamped() {
    assert_eq!(Meters::apply_reward(METER_START), 9000);
    assert_eq!(Meters::apply_reward(0), REWARD_HUNDREDTHS);
    assert_eq!(Meters::apply_reward(8000), METER_MAX);
    assert_eq!(Meters::apply_reward(9999), METER_MAX);
    assert_eq!(Meters::apply_reward(METER_MAX), METER_MAX);
    assert_eq!(Meters::apply_reward(u16::MAX), METER_MAX);
}
