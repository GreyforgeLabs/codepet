use super::*;
use serde_json::json;
use std::io::{BufReader, Cursor, Read};

fn frames(input: &[u8]) -> Cursor<Vec<u8>> {
    Cursor::new(input.to_vec())
}
fn req(text: &str) -> Result<Request, ProtocolError> {
    parse_request(text)
}
fn code_of(text: &str) -> (ErrorCode, Option<String>) {
    let e = req(text).unwrap_err();
    (e.code, e.id)
}

// ---- error codes ----

#[test]
fn every_error_code_serializes_screaming_snake() {
    let expected = [
        "BUSY",
        "INVALID_STATE",
        "STALE_REVISION",
        "UNSUPPORTED_PROVIDER",
        "AUTH_UNAVAILABLE",
        "SANDBOX_UNAVAILABLE",
        "SCOPE_CHANGED",
        "BASELINE_CONFLICT",
        "DISK_LIMIT",
        "OUTPUT_LIMIT",
        "CANCEL_INCOMPLETE",
        "APPLY_RECOVERY_REQUIRED",
        "PROTOCOL_MISMATCH",
        "INVALID_REQUEST",
        "UNKNOWN_METHOD",
        "INVALID_PARAMS",
        "NOT_FOUND",
        "TOKEN_INVALID",
        "TOKEN_EXPIRED",
        "REVIEW_CHANGED",
        "ACKNOWLEDGEMENT_REQUIRED",
        "OVERRIDE_REQUIRED",
        "IDEMPOTENCY_CONFLICT",
        "RESYNC_REQUIRED",
        "CAPACITY",
        "STORAGE_UNAVAILABLE",
        "SCHEMA_UNSUPPORTED",
        "INTERNAL",
        "NO_ACTIVE_PET",
        "NOT_IMPLEMENTED",
        "ALREADY_ARMED",
        "NAME_INVALID",
        "NO_TREATS",
        "PET_NEEDS_CARE",
        "TEST_STATE",
    ];
    assert_eq!(ErrorCode::ALL.len(), expected.len());
    assert_eq!(ErrorCode::ALL.len(), 35);
    for (code, name) in ErrorCode::ALL.into_iter().zip(expected) {
        assert_eq!(serde_json::to_value(code).unwrap(), json!(name));
        assert_eq!(code.as_str(), name);
        assert_eq!(code.to_string(), name);
        assert_eq!(ErrorCode::parse(name), Some(code));
        let back: ErrorCode = serde_json::from_value(json!(name)).unwrap();
        assert_eq!(back, code);
    }
    assert_eq!(ErrorCode::parse("busy"), None);
    assert!(serde_json::from_value::<ErrorCode>(json!("NOPE")).is_err());
}

#[test]
fn retryable_defaults() {
    for code in ErrorCode::ALL {
        let expected = matches!(
            code,
            ErrorCode::Busy | ErrorCode::Capacity | ErrorCode::StorageUnavailable
        );
        assert_eq!(code.retryable(), expected, "{code}");
    }
}

// ---- methods ----

#[test]
fn every_method_parses_and_round_trips() {
    let names = [
        "hello",
        "state.get",
        "events.subscribe",
        "providers.inspect",
        "task.create",
        "task.get",
        "task.update",
        "task.preflight",
        "task.start",
        "task.cancel",
        "task.review",
        "task.logs",
        "task.apply",
        "task.accept_artifact",
        "task.discard",
        "task.rollback",
        "task.recover",
        "history.list",
        "settings.update",
        "pet.pause",
        "pet.care",
        "pet.sleep",
        "pet.wake",
        "pet.select",
        "pet.adopt",
        "pet.rules",
        "pet.configure_life",
        "pet.retire",
        "pet.events_seen",
        "pet.memorial",
        "lifecycle.rules",
        "pet.start",
        "pet.rename",
        "treats.give",
        "app.set_enabled",
        "app.set_helpers",
        "agent.forget_history",
        "clients.status",
        "clients.connect",
        "clients.disconnect",
        "clients.test",
    ];
    assert_eq!(Method::ALL.len(), names.len());
    assert_eq!(Method::ALL.len(), 41);
    for name in names {
        let m = Method::parse(name).unwrap();
        assert_eq!(m.as_str(), name);
        assert_eq!(m.to_string(), name);
        assert_eq!(serde_json::to_value(m).unwrap(), json!(name));
    }
    assert_eq!(Method::parse("Hello"), None);
    assert_eq!(Method::parse("task.delete"), None);
    assert_eq!(Method::parse(""), None);
}

#[test]
fn mutating_and_implemented_classification() {
    // state.get may settle a persisted nap or checkpoint live execution, so it is not a read.
    let reads = [
        Method::Hello,
        Method::EventsSubscribe,
        Method::ProvidersInspect,
        Method::TaskGet,
        Method::TaskReview,
        Method::TaskLogs,
        Method::HistoryList,
        Method::PetMemorial,
        Method::LifecycleRules,
    ];
    for m in Method::ALL {
        assert_eq!(m.is_mutating(), !reads.contains(&m), "{m}");
    }
    for m in [Method::TaskLogs, Method::TaskApply, Method::TaskRollback]
        .into_iter()
        .chain(Method::V03_ADDED)
    {
        assert!(!m.is_implemented());
        assert!(req(&format!(
            r#"{{"v":1,"id":"a","method":"{m}","params":{{}}}}"#
        ))
        .is_ok());
    }
    assert!(Method::TaskRecover.is_implemented());
    assert!(Method::StateGet.is_mutating());
    assert!(Method::TaskAcceptArtifact.is_mutating());
    assert!(Method::TaskPreflight.is_mutating());
}

// ---- request parsing ----

#[test]
fn request_round_trip() {
    let text = r#"{"v":1,"id":"req-1","method":"task.start","params":{"task_id":"t","revision":3,"preflight_token":"x"}}"#;
    let r = req(text).unwrap();
    assert_eq!(r.v, 1);
    assert_eq!(r.id, "req-1");
    assert_eq!(r.method, Method::TaskStart);
    let frame = r.to_frame().unwrap();
    assert!(frame.ends_with('\n'));
    assert_eq!(frame.matches('\n').count(), 1);
    assert_eq!(req(&frame).unwrap(), r);
    let built = Request::new("req-1", Method::TaskStart, r.params.clone());
    assert_eq!(built, r);
}

#[test]
fn missing_params_become_empty_object() {
    let r = req(r#"{"v":1,"id":"a","method":"state.get"}"#).unwrap();
    assert_eq!(r.params, json!({}));
    let _: EmptyParams = parse_params(r.params).unwrap();
}

#[test]
fn ids_are_validated() {
    for good in ["a", "A.b_c:d-9", &"x".repeat(128)] {
        assert!(valid_id(good), "{good}");
        let text = format!(r#"{{"v":1,"id":"{good}","method":"hello"}}"#);
        assert_eq!(req(&text).unwrap().id, good);
    }
    for bad in ["", "a b", "a/b", "é", "a\"", &"x".repeat(129)] {
        assert!(!valid_id(bad), "{bad}");
    }
    for text in [
        r#"{"v":1,"id":"","method":"hello"}"#,
        r#"{"v":1,"id":"a b","method":"hello"}"#,
        r#"{"v":1,"id":"a/b","method":"hello"}"#,
        r#"{"v":1,"id":7,"method":"hello"}"#,
        r#"{"v":1,"id":null,"method":"hello"}"#,
        r#"{"v":1,"method":"hello"}"#,
    ] {
        assert_eq!(code_of(text), (ErrorCode::InvalidRequest, None), "{text}");
    }
    let long = format!(r#"{{"v":1,"id":"{}","method":"hello"}}"#, "x".repeat(129));
    assert_eq!(code_of(&long), (ErrorCode::InvalidRequest, None));
}

#[test]
fn bad_versions() {
    let id = Some("abc".to_string());
    for v in ["2", "0", "-1", "1.5", "99999999999999999999", "1e0"] {
        let text = format!(r#"{{"v":{v},"id":"abc","method":"hello"}}"#);
        assert_eq!(
            code_of(&text),
            (ErrorCode::ProtocolMismatch, id.clone()),
            "{v}"
        );
    }
    // A newer client with new fields still gets a version answer, not INVALID_REQUEST.
    assert_eq!(
        code_of(r#"{"v":2,"id":"abc","method":"x.y","trace":1}"#),
        (ErrorCode::ProtocolMismatch, id.clone())
    );
    // Version mismatch with an unrecoverable id.
    assert_eq!(
        code_of(r#"{"v":2,"id":"bad id","method":"hello"}"#),
        (ErrorCode::ProtocolMismatch, None)
    );
    for v in [r#""1""#, "null", "true", "[1]"] {
        let text = format!(r#"{{"v":{v},"id":"abc","method":"hello"}}"#);
        assert_eq!(
            code_of(&text),
            (ErrorCode::InvalidRequest, id.clone()),
            "{v}"
        );
    }
    assert_eq!(
        code_of(r#"{"id":"abc","method":"hello"}"#),
        (ErrorCode::InvalidRequest, id)
    );
}

#[test]
fn unknown_top_level_fields_rejected() {
    let e = req(r#"{"v":1,"id":"a","method":"hello","params":{},"extra":true}"#).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidRequest);
    assert_eq!(e.id.as_deref(), Some("a"));
    assert!(e.message.contains("extra"));
    let huge = "k".repeat(5000);
    let e = req(&format!(
        r#"{{"v":1,"id":"a","method":"hello","{huge}":1}}"#
    ))
    .unwrap_err();
    assert!(!e.message.contains(&huge));
    assert!(e.message.len() < 200);
    // The derived type also refuses unknown fields on its own.
    assert!(serde_json::from_str::<Request>(r#"{"v":1,"id":"a","method":"hello","x":1}"#).is_err());
}

#[test]
fn unknown_and_malformed_methods() {
    assert_eq!(
        code_of(r#"{"v":1,"id":"a","method":"task.delete"}"#),
        (ErrorCode::UnknownMethod, Some("a".into()))
    );
    assert_eq!(
        code_of(r#"{"v":1,"id":"a","method":7}"#),
        (ErrorCode::InvalidRequest, Some("a".into()))
    );
    assert_eq!(
        code_of(r#"{"v":1,"id":"a"}"#),
        (ErrorCode::InvalidRequest, Some("a".into()))
    );
}

#[test]
fn params_must_be_objects() {
    for p in ["[]", "null", "1", r#""x""#] {
        let text = format!(r#"{{"v":1,"id":"a","method":"hello","params":{p}}}"#);
        assert_eq!(
            code_of(&text),
            (ErrorCode::InvalidRequest, Some("a".into()))
        );
    }
}

#[test]
fn malformed_json_and_non_objects() {
    for text in ["", "{", "[]", "1", "null", r#""s""#, "{\"v\":1,}", "{} {}"] {
        assert_eq!(code_of(text), (ErrorCode::InvalidRequest, None), "{text:?}");
    }
}

#[test]
fn duplicate_keys_rejected_at_any_depth() {
    assert_eq!(
        code_of(r#"{"v":1,"id":"a","id":"b","method":"hello"}"#),
        (ErrorCode::InvalidRequest, None)
    );
    let e =
        req(r#"{"v":1,"id":"a","method":"task.cancel","params":{"task_id":"x","task_id":"y"}}"#)
            .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidRequest);
    assert!(e.message.contains("duplicate"));
}

#[test]
fn parse_request_rejects_nul_newline_and_oversize() {
    assert_eq!(
        code_of("{\"v\":1,\"id\":\"a\",\"method\":\"hello\"}\0"),
        (ErrorCode::InvalidRequest, None)
    );
    assert_eq!(
        code_of("{\"v\":1,\n\"id\":\"a\",\"method\":\"hello\"}"),
        (ErrorCode::InvalidRequest, None)
    );
    let ok = req("{\"v\":1,\"id\":\"a\",\"method\":\"hello\"}\n").unwrap();
    assert_eq!(ok.method, Method::Hello);
    let pad = "x".repeat(MAX_FRAME_BYTES);
    let text = format!(r#"{{"v":1,"id":"a","method":"hello","params":{{"client":"{pad}"}}}}"#);
    assert_eq!(code_of(&text), (ErrorCode::InvalidRequest, None));
}

// ---- responses and events ----

#[test]
fn ok_response_shape() {
    let r = Response::ok("a", json!({"x": 1}));
    let frame = r.to_frame().unwrap();
    assert_eq!(
        frame,
        "{\"v\":1,\"id\":\"a\",\"ok\":true,\"result\":{\"x\":1}}\n"
    );
    let back: Response = serde_json::from_str(&frame).unwrap();
    assert_eq!(back, r);
}

#[test]
fn error_response_shape() {
    let r = Response::err(Some("a".into()), ErrorCode::ScopeChanged, "Review it.");
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        json!({"v":1,"id":"a","ok":false,"error":{"code":"SCOPE_CHANGED","message":"Review it.","retryable":false}})
    );
    let busy = Response::err(None, ErrorCode::Busy, "busy");
    assert_eq!(
        busy.to_frame().unwrap(),
        "{\"v\":1,\"id\":null,\"ok\":false,\"error\":{\"code\":\"BUSY\",\"message\":\"busy\",\"retryable\":true}}\n"
    );
    let back: Response = serde_json::from_str(&busy.to_frame().unwrap()).unwrap();
    assert_eq!(back, busy);
}

#[test]
fn protocol_error_to_response_keeps_id() {
    let e = req(r#"{"v":3,"id":"z","method":"hello"}"#).unwrap_err();
    let r = e.to_response();
    assert_eq!(r.id.as_deref(), Some("z"));
    assert_eq!(r.error.unwrap().code, ErrorCode::ProtocolMismatch);
    assert!(e.to_string().starts_with("PROTOCOL_MISMATCH: "));
}

#[test]
fn messages_are_bounded() {
    let r = Response::err(None, ErrorCode::Internal, "é".repeat(5000));
    let msg = r.error.unwrap().message;
    assert!(msg.len() <= MAX_MESSAGE_BYTES);
    assert!(msg.ends_with('…'));
    assert_eq!(bounded_message("short".into()), "short");
    let exact = "a".repeat(MAX_MESSAGE_BYTES);
    assert_eq!(bounded_message(exact.clone()), exact);
}

#[test]
fn frames_escape_control_characters() {
    let r = Response::ok("a", json!({"s": "line\nbreak\0nul"}));
    let frame = r.to_frame().unwrap();
    assert_eq!(frame.matches('\n').count(), 1);
    assert!(!frame.contains('\0'));
    let mut cur = frames(frame.as_bytes());
    let line = read_frame(&mut cur).unwrap().unwrap();
    let back: Response = serde_json::from_str(&line).unwrap();
    assert_eq!(back, r);
}

/// Overhead of `{"v":1,"id":"a","ok":true,"result":"…"}` around the string body.
fn ok_frame_with_len(total: usize) -> Response {
    let overhead = Response::ok("a", json!("")).to_frame().unwrap().len();
    Response::ok("a", json!("x".repeat(total - overhead)))
}

#[test]
fn writer_refuses_frames_over_limit() {
    let fits = ok_frame_with_len(MAX_FRAME_BYTES);
    let frame = fits.to_frame().unwrap();
    assert_eq!(frame.len(), MAX_FRAME_BYTES);
    assert_eq!(
        read_frame(&mut frames(frame.as_bytes()))
            .unwrap()
            .unwrap()
            .len(),
        MAX_FRAME_BYTES - 1
    );

    let over = ok_frame_with_len(MAX_FRAME_BYTES + 1);
    let e = over.to_frame().unwrap_err();
    assert_eq!(e.code, ErrorCode::OutputLimit);
    assert_eq!(e.id.as_deref(), Some("a"));
    let fallback = over.to_frame_or_limit();
    let back: Response = serde_json::from_str(&fallback).unwrap();
    assert_eq!(back.id.as_deref(), Some("a"));
    assert_eq!(back.error.unwrap().code, ErrorCode::OutputLimit);
    assert_eq!(fits.to_frame_or_limit(), frame);
}

#[test]
fn event_shape() {
    let e = Event::new(
        "task.state",
        18,
        Some("t1".into()),
        json!({"state":"running"}),
    );
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        json!({"v":1,"event":"task.state","seq":18,"task_id":"t1","data":{"state":"running"}})
    );
    let frame = e.to_frame().unwrap();
    assert_eq!(serde_json::from_str::<Event>(&frame).unwrap(), e);
    let no_task = Event::new("settings", 2, None, json!({}));
    assert!(!no_task.to_frame().unwrap().contains("task_id"));
    let huge = Event::new("x", 1, None, json!("y".repeat(MAX_FRAME_BYTES)));
    assert_eq!(huge.to_frame().unwrap_err().code, ErrorCode::OutputLimit);
}

// ---- framing ----

#[test]
fn reads_frames_and_clean_eof() {
    let mut cur = frames(b"one\ntwo\n");
    assert_eq!(read_frame(&mut cur).unwrap().as_deref(), Some("one"));
    assert_eq!(read_frame(&mut cur).unwrap().as_deref(), Some("two"));
    assert!(read_frame(&mut cur).unwrap().is_none());
    assert!(read_frame(&mut cur).unwrap().is_none());
    assert!(read_frame(&mut frames(b"")).unwrap().is_none());
}

#[test]
fn frame_limit_is_exact() {
    // 65536 bytes including the newline: accepted.
    let mut input = vec![b'a'; MAX_FRAME_BYTES - 1];
    input.push(b'\n');
    assert_eq!(input.len(), MAX_FRAME_BYTES);
    let got = read_frame(&mut frames(&input)).unwrap().unwrap();
    assert_eq!(got.len(), MAX_FRAME_BYTES - 1);
    // 65537 bytes including the newline: rejected.
    let mut input = vec![b'a'; MAX_FRAME_BYTES];
    input.push(b'\n');
    assert!(matches!(
        read_frame(&mut frames(&input)),
        Err(FrameError::TooLarge)
    ));
}

#[test]
fn frame_limit_is_exact_with_small_buffers() {
    for cap in [1, 7, 4096, MAX_FRAME_BYTES + 10] {
        let mut input = vec![b'a'; MAX_FRAME_BYTES - 1];
        input.push(b'\n');
        input.extend(vec![b'b'; MAX_FRAME_BYTES]);
        input.extend_from_slice(b"\nnext\n");
        let mut r = BufReader::with_capacity(cap, Cursor::new(input));
        assert_eq!(
            read_frame(&mut r).unwrap().unwrap().len(),
            MAX_FRAME_BYTES - 1
        );
        assert!(
            matches!(read_frame(&mut r), Err(FrameError::TooLarge)),
            "{cap}"
        );
        assert_eq!(read_frame(&mut r).unwrap().as_deref(), Some("next"));
        assert!(read_frame(&mut r).unwrap().is_none());
    }
}

#[test]
fn oversize_frame_is_drained_then_next_frame_reads() {
    let mut input = vec![b'z'; 1_000_000];
    input.extend_from_slice(b"\n{\"v\":1,\"id\":\"a\",\"method\":\"hello\"}\n");
    let mut r = FrameReader::new(BufReader::new(Cursor::new(input)));
    let e = r.read_frame().unwrap_err();
    assert!(matches!(e, FrameError::TooLarge));
    assert_eq!(e.code(), ErrorCode::InvalidRequest);
    let next = r.read_frame().unwrap().unwrap();
    assert_eq!(req(&next).unwrap().method, Method::Hello);
    assert!(r.read_frame().unwrap().is_none());
}

#[test]
fn oversize_frame_at_eof_reports_too_large() {
    let input = vec![b'z'; MAX_FRAME_BYTES + 5];
    let mut cur = frames(&input);
    assert!(matches!(read_frame(&mut cur), Err(FrameError::TooLarge)));
    assert!(read_frame(&mut cur).unwrap().is_none());
}

#[test]
fn rejects_nul_utf8_blank_and_missing_newline() {
    let mut cur = frames(b"a\0b\n\xff\xfe\n\n  \t\r\nok\npartial");
    assert!(matches!(read_frame(&mut cur), Err(FrameError::Nul)));
    assert!(matches!(read_frame(&mut cur), Err(FrameError::InvalidUtf8)));
    assert!(matches!(read_frame(&mut cur), Err(FrameError::Blank)));
    assert!(matches!(read_frame(&mut cur), Err(FrameError::Blank)));
    assert_eq!(read_frame(&mut cur).unwrap().as_deref(), Some("ok"));
    assert!(matches!(
        read_frame(&mut cur),
        Err(FrameError::MissingNewline)
    ));
    assert!(read_frame(&mut cur).unwrap().is_none());
}

#[test]
fn frame_errors_map_to_protocol_errors() {
    let e: ProtocolError = FrameError::Nul.into();
    assert_eq!(e.code, ErrorCode::InvalidRequest);
    assert!(e.id.is_none());
    let io = FrameError::Io(std::io::Error::other("boom"));
    assert_eq!(io.code(), ErrorCode::Internal);
    assert_eq!(
        FrameError::TooLarge.to_string(),
        "frame exceeds 65536 bytes"
    );
}

struct Flaky {
    data: Cursor<Vec<u8>>,
    interrupted: bool,
}
impl Read for Flaky {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if !self.interrupted {
            self.interrupted = true;
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        self.data.read(buf)
    }
}

#[test]
fn interrupted_reads_are_retried_and_io_errors_surface() {
    let mut r = BufReader::new(Flaky {
        data: Cursor::new(b"x\n".to_vec()),
        interrupted: false,
    });
    assert_eq!(read_frame(&mut r).unwrap().as_deref(), Some("x"));
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("broken"))
        }
    }
    assert!(matches!(
        read_frame(&mut BufReader::new(Broken)),
        Err(FrameError::Io(_))
    ));
}

// ---- params ----

#[test]
fn draft_params_round_trip() {
    let v = json!({"category":"mental_health","provider":"mock","folder":"/p","scope":["src"],
        "instructions":"do it","mock":{"outcome":"artifact","step_ms":0}});
    let p: TaskCreateParams = parse_params(v.clone()).unwrap();
    assert_eq!(
        p.mock.as_ref().unwrap().outcome.as_deref(),
        Some("artifact")
    );
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
    let mut no_mock = v.clone();
    no_mock.as_object_mut().unwrap().remove("mock");
    let p: TaskCreateParams = parse_params(no_mock).unwrap();
    assert!(p.mock.is_none());
}

#[test]
fn update_params_carry_draft() {
    let v = json!({"task_id":"t","expected_revision":2,"category":"hunger","provider":"mock",
        "folder":"/p","scope":[],"instructions":"x","mock":{"step_ms":2000}});
    let p: TaskUpdateParams = parse_params(v).unwrap();
    assert_eq!(p.expected_revision, 2);
    assert_eq!(p.draft().category, "hunger");
    assert_eq!(p.draft().mock.unwrap().step_ms, Some(2000));
}

#[test]
fn unknown_params_fields_rejected() {
    let cases = [json!({"task_id":"t","extra":1}), json!({"bogus":1})];
    let e = params_from_value::<TaskIdParams>(cases[0].clone()).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidParams);
    assert!(e.message.contains("extra"));
    assert!(params_from_value::<EmptyParams>(cases[1].clone()).is_err());
    let nested = json!({"category":"hunger","provider":"mock","folder":"/p","scope":[],
        "instructions":"x","mock":{"outcome":"artifact","sneaky":true}});
    assert_eq!(
        params_from_value::<DraftParams>(nested).unwrap_err().code,
        ErrorCode::InvalidParams
    );
}

#[test]
fn missing_and_mistyped_params() {
    let e = params_from_value::<TaskStartParams>(json!({"task_id":"t","revision":1})).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidParams);
    assert!(e.message.contains("preflight_token"));
    let e =
        params_from_value::<TaskPreflightParams>(json!({"task_id":"t","revision":-1})).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidParams);
    assert!(params_from_value::<PetPauseParams>(json!({"paused":"yes"})).is_err());
    assert!(
        params_from_value::<SettingsUpdateParams>(json!({"expected_revision":1,
        "decay_enabled":false,"decay_per_hour":[1,2,3],"timeout_seconds":60}))
        .is_err()
    );
}

#[test]
fn params_must_be_object_not_sequence() {
    // serde would otherwise accept a positional array for a struct.
    let e = params_from_value::<TaskPreflightParams>(json!(["t", 1])).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidParams);
    assert!(params_from_value::<EmptyParams>(Value::Null).is_err());
}

#[test]
fn params_errors_do_not_echo_huge_input() {
    let huge = "q".repeat(100_000);
    let e = params_from_value::<TaskPreflightParams>(json!({"task_id":"t","revision":huge}))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidParams);
    assert!(e.message.len() < 300, "{}", e.message.len());
    let e = params_from_value::<EmptyParams>(json!({huge.clone(): 1})).unwrap_err();
    assert!(e.message.len() < 300);
}

#[test]
fn numeric_limits() {
    let ok = |v| parse_params::<EventsSubscribeParams>(v);
    assert_eq!(ok(json!({"after_seq":0})).unwrap().limit_or_default(), 100);
    assert!(ok(json!({"after_seq":0,"limit":100})).is_ok());
    assert!(ok(json!({"after_seq":0,"limit":1})).is_ok());
    for bad in [0, 101] {
        let e = ok(json!({"after_seq":0,"limit":bad})).unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidParams);
    }
    let h = |v| parse_params::<HistoryListParams>(v);
    assert_eq!(h(json!({})).unwrap().limit_or_default(), 50);
    assert!(h(json!({"limit":50,"before_ms":123})).is_ok());
    assert!(h(json!({"limit":51})).is_err());
    assert!(h(json!({"limit":0})).is_err());
    assert!(h(json!({"limit":null})).is_ok());
    assert!(h(json!({"before_ms":5,"before_task_id":"t"})).is_ok());
    assert!(
        h(json!({"before_task_id":"t"})).is_err(),
        "cursor id needs before_ms"
    );
    assert!(h(json!({"before_ms":5,"before_task_id":""})).is_err());
    let m = |v| parse_params::<MockParams>(v);
    assert!(m(json!({"step_ms":2000})).is_ok());
    assert!(m(json!({"step_ms":0})).is_ok());
    assert_eq!(
        m(json!({"step_ms":2001})).unwrap_err().code,
        ErrorCode::InvalidParams
    );
    assert!(m(json!({"step_ms":-1})).is_err());
    assert!(m(json!({})).is_ok());
    // Nested mock limits apply through draft validation.
    let draft = json!({"category":"hunger","provider":"mock","folder":"/p","scope":[],
        "instructions":"x","mock":{"step_ms":2001}});
    assert!(parse_params::<DraftParams>(draft.clone()).is_err());
    let mut update = draft;
    update["task_id"] = json!("t");
    update["expected_revision"] = json!(1);
    assert!(parse_params::<TaskUpdateParams>(update).is_err());
}

#[test]
fn token_and_reason_bounds() {
    let accept = |v| parse_params::<TaskAcceptArtifactParams>(v);
    let ok = accept(json!({"task_id":"t","review_digest":"d","acknowledged":true})).unwrap();
    assert!(ok.override_reason.is_none());
    assert!(accept(
        json!({"task_id":"t","review_digest":"d","acknowledged":true,
        "override_reason":"checks are simulated"})
    )
    .is_ok());
    assert!(accept(
        json!({"task_id":"t","review_digest":"d","acknowledged":true,
        "override_reason":"r".repeat(MAX_OVERRIDE_REASON_BYTES + 1)})
    )
    .is_err());
    // The limit is 512 bytes after trimming, in UTF-8 bytes rather than characters.
    assert_eq!(MAX_OVERRIDE_REASON_BYTES, 512);
    let padded = format!("  {}  ", "r".repeat(MAX_OVERRIDE_REASON_BYTES));
    assert!(accept(
        json!({"task_id":"t","review_digest":"d","acknowledged":true,"override_reason":padded})
    )
    .is_ok());
    let cjk = "\u{4e2d}".repeat(171);
    assert!(accept(
        json!({"task_id":"t","review_digest":"d","acknowledged":true,"override_reason":cjk})
    )
    .is_err());
    assert!(accept(json!({"task_id":"","review_digest":"d","acknowledged":true})).is_err());
    assert!(
        accept(json!({"task_id":"t","review_digest":"d".repeat(129),"acknowledged":true})).is_err()
    );
    assert!(accept(json!({"task_id":"t","review_digest":"d"})).is_err());
    assert!(parse_params::<TaskStartParams>(
        json!({"task_id":"t","revision":1,"preflight_token":"a".repeat(64)})
    )
    .is_ok());
    assert!(parse_params::<TaskStartParams>(
        json!({"task_id":"t","revision":1,"preflight_token":""})
    )
    .is_err());
    assert!(parse_params::<TaskDiscardParams>(json!({"task_id":"t","review_digest":"d"})).is_ok());
    assert!(parse_params::<TaskIdParams>(json!({"task_id":"x".repeat(129)})).is_err());
}

#[test]
fn hello_settings_and_pause_params() {
    let h: HelloParams = parse_params(json!({})).unwrap();
    assert_eq!(h, HelloParams::default());
    let h: HelloParams = parse_params(json!({"client":"qml","versions":[1]})).unwrap();
    assert_eq!(h.versions, Some(vec![1]));
    assert!(parse_params::<HelloParams>(json!({"client":"c".repeat(129)})).is_err());
    assert!(parse_params::<HelloParams>(json!({"versions":vec![1; 17]})).is_err());
    let s: SettingsUpdateParams = parse_params(json!({"expected_revision":1,"decay_enabled":true,
        "decay_per_hour":[100,100,50,50],"timeout_seconds":1800}))
    .unwrap();
    assert_eq!(s.decay_per_hour, [100, 100, 50, 50]);
    let p: PetPauseParams = parse_params(json!({"paused":true})).unwrap();
    assert!(p.paused);
    let _: TaskRecoverParams = parse_params(json!({})).unwrap();
}

#[test]
fn end_to_end_request_to_params() {
    let line = r#"{"v":1,"id":"acc-1","method":"task.accept_artifact","params":{"task_id":"t","review_digest":"abc","acknowledged":true}}"#;
    let mut cur = frames(format!("{line}\n").as_bytes());
    let frame = read_frame(&mut cur).unwrap().unwrap();
    let request = parse_request(&frame).unwrap();
    let p: TaskAcceptArtifactParams = parse_params(request.params).unwrap();
    assert!(p.acknowledged);
    let resp = Response::ok(request.id, json!({"duplicate": false}));
    assert!(resp
        .to_frame()
        .unwrap()
        .starts_with("{\"v\":1,\"id\":\"acc-1\",\"ok\":true"));
}

#[test]
fn care_sleep_and_wake_have_strict_params() {
    for action in ["food", "water", "play", "wash", "bathroom", "routine"] {
        let p: PetCareParams = parse_params(json!({"action": action})).unwrap();
        assert_eq!(p.action, action);
        assert_eq!(serde_json::to_value(p).unwrap(), json!({"action": action}));
    }
    for params in [
        json!({}),
        json!({"action": "sleep"}),
        json!({"action": "Food"}),
        json!({"action": ""}),
        json!({"action": "food", "amount": 100}),
        json!({"action": "food", "request_id": "injected"}),
        json!({"action": null}),
        json!({"action": ["food", "water"]}),
        json!({"action": "f".repeat(MAX_FRAME_BYTES)}),
    ] {
        assert_eq!(
            parse_params::<PetCareParams>(params).unwrap_err().code,
            ErrorCode::InvalidParams
        );
    }
    for method in [Method::PetSleep, Method::PetWake] {
        assert!(method.is_mutating());
        assert!(method.is_implemented());
        let request = req(&format!(
            r#"{{"v":1,"id":"rest-1","method":"{method}","params":{{}}}}"#
        ))
        .unwrap();
        let _: EmptyParams = parse_params(request.params).unwrap();
    }
    assert!(parse_params::<PetSleepParams>(json!({"duration_ms": 1})).is_err());
    assert!(parse_params::<PetWakeParams>(json!({"force": true})).is_err());
    assert!(parse_params::<PetCareParams>(Value::Null).is_err());
}

// ---- v0.2 lifecycle protocol ----

const NEW_METHODS: [(&str, bool); 8] = [
    ("pet.select", true),
    ("pet.adopt", true),
    ("pet.rules", true),
    ("pet.configure_life", true),
    ("pet.retire", true),
    ("pet.events_seen", true),
    ("pet.memorial", false),
    ("lifecycle.rules", false),
];

#[test]
fn protocol_version_stays_one_and_changes_are_additive() {
    assert_eq!(PROTOCOL_VERSION, 1);
    // The 23 v0.1 methods and 28 v0.1 codes keep their positions; new entries are appended.
    assert_eq!(Method::ALL[..23].last(), Some(&Method::PetWake));
    assert_eq!(Method::ALL[0], Method::Hello);
    assert_eq!(ErrorCode::ALL[27], ErrorCode::Internal);
    assert_eq!(ErrorCode::ALL[28], ErrorCode::NoActivePet);
    // v0.3 appends after the v0.2 lists.
    assert_eq!(ErrorCode::ALL[29], ErrorCode::NotImplemented);
    assert_eq!(Method::ALL[31..], Method::V03_ADDED);
    for (i, (name, _)) in NEW_METHODS.iter().enumerate() {
        assert_eq!(Method::ALL[23 + i].as_str(), *name);
    }
    let mut seen = std::collections::HashSet::new();
    assert!(Method::ALL.iter().all(|m| seen.insert(m.as_str())));
    let mut seen = std::collections::HashSet::new();
    assert!(ErrorCode::ALL.iter().all(|c| seen.insert(c.as_str())));
}

#[test]
fn is_mutating_table_for_every_method() {
    // The full classification, written out so any change is a visible test edit.
    let table = [
        ("hello", false),
        ("state.get", true),
        ("events.subscribe", false),
        ("providers.inspect", false),
        ("task.create", true),
        ("task.get", false),
        ("task.update", true),
        ("task.preflight", true),
        ("task.start", true),
        ("task.cancel", true),
        ("task.review", false),
        ("task.logs", false),
        ("task.apply", true),
        ("task.accept_artifact", true),
        ("task.discard", true),
        ("task.rollback", true),
        ("task.recover", true),
        ("history.list", false),
        ("settings.update", true),
        ("pet.pause", true),
        ("pet.care", true),
        ("pet.sleep", true),
        ("pet.wake", true),
        ("pet.select", true),
        ("pet.adopt", true),
        ("pet.rules", true),
        ("pet.configure_life", true),
        ("pet.retire", true),
        ("pet.events_seen", true),
        ("pet.memorial", false),
        ("lifecycle.rules", false),
        ("pet.start", true),
        ("pet.rename", true),
        ("treats.give", true),
        ("app.set_enabled", true),
        ("app.set_helpers", true),
        ("agent.forget_history", true),
        ("clients.status", true),
        ("clients.connect", true),
        ("clients.disconnect", true),
        ("clients.test", true),
    ];
    assert_eq!(table.len(), Method::ALL.len());
    for (m, (name, mutating)) in Method::ALL.into_iter().zip(table) {
        assert_eq!(m.as_str(), name);
        assert_eq!(m.is_mutating(), mutating, "{name}");
    }
}

#[test]
fn new_methods_parse_and_are_implemented() {
    for (name, mutating) in NEW_METHODS {
        let m = Method::parse(name).unwrap();
        assert_eq!(m.is_mutating(), mutating, "{name}");
        assert!(m.is_implemented(), "{name}");
        let r = req(&format!(
            r#"{{"v":1,"id":"life-1","method":"{name}","params":{{}}}}"#
        ))
        .unwrap();
        assert_eq!(r.method, m);
        assert_eq!(
            r.to_frame().unwrap(),
            format!("{{\"v\":1,\"id\":\"life-1\",\"method\":\"{name}\",\"params\":{{}}}}\n")
        );
    }
    for near in [
        "pet.Select",
        "pet.configure-life",
        "pet.memorials",
        "lifecycle.rule",
        "life.rules",
    ] {
        assert_eq!(
            code_of(&format!(r#"{{"v":1,"id":"a","method":"{near}"}}"#)),
            (ErrorCode::UnknownMethod, Some("a".into())),
            "{near}"
        );
    }
}

#[test]
fn no_active_pet_code_shape() {
    let c = ErrorCode::NoActivePet;
    assert_eq!(c.as_str(), "NO_ACTIVE_PET");
    assert!(!c.retryable());
    assert_eq!(ErrorCode::parse("NO_ACTIVE_PET"), Some(c));
    let r = Response::err(Some("care-1".into()), c, "No companion is living.");
    assert_eq!(
        serde_json::to_value(&r).unwrap(),
        json!({"v":1,"id":"care-1","ok":false,"error":{"code":"NO_ACTIVE_PET","message":"No companion is living.","retryable":false}})
    );
    let back: Response = serde_json::from_str(&r.to_frame().unwrap()).unwrap();
    assert_eq!(back, r);
}

fn invalid<T: serde::de::DeserializeOwned + Validate + std::fmt::Debug>(v: Value) -> String {
    let e = parse_params::<T>(v).unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidParams);
    assert!(e.message.len() < 300, "{}", e.message);
    e.message
}

#[test]
fn pet_ids_are_the_fifteen_companions() {
    assert_eq!(PET_IDS.len(), 15);
    assert_eq!(PET_IDS[0], "cat");
    let mut seen = std::collections::HashSet::new();
    for id in PET_IDS {
        assert!(seen.insert(id), "{id}");
        assert!(is_pet_id(id));
        assert!(
            id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}"
        );
    }
    for bad in [
        "",
        "Cat",
        "red-panda",
        "goldfish-in-jar",
        "ginger",
        " cat",
        "cat ",
    ] {
        assert!(!is_pet_id(bad), "{bad}");
    }
}

#[test]
fn pet_select_params() {
    for id in PET_IDS {
        let p: PetSelectParams = parse_params(json!({"life_id":"life-1","pet_id":id})).unwrap();
        assert_eq!(p.pet_id, id);
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            json!({"life_id":"life-1","pet_id":id})
        );
    }
    assert!(
        invalid::<PetSelectParams>(json!({"life_id":"life-1","pet_id":"dragon"}))
            .contains("pet_id")
    );
    assert!(
        invalid::<PetSelectParams>(json!({"life_id":"life-1","pet_id":"red-panda"}))
            .contains("pet_id")
    );
    assert!(invalid::<PetSelectParams>(json!({"life_id":"","pet_id":"cat"})).contains("life_id"));
    invalid::<PetSelectParams>(json!({"life_id":"l".repeat(129),"pet_id":"cat"}));
    invalid::<PetSelectParams>(json!({"pet_id":"cat"}));
    invalid::<PetSelectParams>(json!({"life_id":"life-1"}));
    invalid::<PetSelectParams>(json!({"life_id":"life-1","pet_id":"cat","stage":2}));
    invalid::<PetSelectParams>(json!({"life_id":"life-1","pet_id":null}));
    invalid::<PetSelectParams>(json!({"life_id":7,"pet_id":"cat"}));
    let huge = "q".repeat(100_000);
    invalid::<PetSelectParams>(json!({"life_id":"life-1","pet_id":huge}));
}

#[test]
fn pet_adopt_params() {
    let p: PetAdoptParams = parse_params(json!({"species":"axolotl","mode":"permadeath",
        "acknowledge_rules":true}))
    .unwrap();
    assert_eq!(p.name, None);
    assert_eq!(p.pace, None);
    assert_eq!(p.pace_or_default(), "steady");
    let full = json!({"species":"red_panda","name":"Maple","mode":"gentle","pace":"playtest",
        "acknowledge_rules":true});
    let p: PetAdoptParams = parse_params(full.clone()).unwrap();
    assert_eq!(p.pace_or_default(), "playtest");
    assert_eq!(serde_json::to_value(&p).unwrap(), full);
    // A missing acknowledgement is well-formed: storage answers ACKNOWLEDGEMENT_REQUIRED.
    let p: PetAdoptParams = parse_params(json!({"species":"cat","mode":"permadeath",
        "acknowledge_rules":false}))
    .unwrap();
    assert!(!p.acknowledge_rules);
    // Explicit nulls mean "use the default".
    let p: PetAdoptParams = parse_params(json!({"species":"cat","name":null,"pace":null,
        "mode":"gentle","acknowledge_rules":true}))
    .unwrap();
    assert_eq!((p.name, p.pace), (None, None));
    // A name may carry spaces and non-ASCII text; core normalizes it.
    assert!(
        parse_params::<PetAdoptParams>(json!({"species":"cat","name":"  Señor Ñoño ",
        "mode":"gentle","acknowledge_rules":true}))
        .is_ok()
    );
    let base = json!({"species":"cat","mode":"permadeath","acknowledge_rules":true});
    let with = |k: &str, v: Value| {
        let mut o = base.clone();
        o[k] = v;
        o
    };
    assert!(invalid::<PetAdoptParams>(with("species", json!("dragon"))).contains("species"));
    assert!(invalid::<PetAdoptParams>(with("species", json!("Cat"))).contains("species"));
    assert!(invalid::<PetAdoptParams>(with("mode", json!("hardcore"))).contains("mode"));
    assert!(invalid::<PetAdoptParams>(with("mode", json!("Permadeath"))).contains("mode"));
    assert!(invalid::<PetAdoptParams>(with("pace", json!("fast"))).contains("pace"));
    assert!(invalid::<PetAdoptParams>(with("name", json!(""))).contains("name"));
    assert!(invalid::<PetAdoptParams>(with("name", json!("   "))).contains("name"));
    assert!(invalid::<PetAdoptParams>(with("name", json!("a\u{0}b"))).contains("name"));
    invalid::<PetAdoptParams>(with(
        "name",
        json!("n".repeat(MAX_PET_NAME_INPUT_BYTES + 1)),
    ));
    assert!(parse_params::<PetAdoptParams>(with(
        "name",
        json!("n".repeat(MAX_PET_NAME_INPUT_BYTES))
    ))
    .is_ok());
    invalid::<PetAdoptParams>(with("acknowledge_rules", json!("yes")));
    invalid::<PetAdoptParams>(with("ordinal", json!(2)));
    invalid::<PetAdoptParams>(with("xp", json!(400)));
    invalid::<PetAdoptParams>(json!({"species":"cat","acknowledge_rules":true}));
    invalid::<PetAdoptParams>(json!({"species":"cat","mode":"gentle"}));
    invalid::<PetAdoptParams>(json!({"mode":"gentle","acknowledge_rules":true}));
}

#[test]
fn pet_rules_params() {
    for mode in LIFE_MODES {
        for ack in [true, false] {
            let p: PetRulesParams =
                parse_params(json!({"life_id":"life-1","mode":mode,"acknowledge_rules":ack}))
                    .unwrap();
            assert_eq!((p.mode.as_str(), p.acknowledge_rules), (mode, ack));
        }
    }
    assert!(
        invalid::<PetRulesParams>(json!({"life_id":"life-1","mode":"standard",
        "acknowledge_rules":true}))
        .contains("mode")
    );
    invalid::<PetRulesParams>(json!({"life_id":"","mode":"gentle","acknowledge_rules":true}));
    invalid::<PetRulesParams>(json!({"life_id":"life-1","acknowledge_rules":true}));
    invalid::<PetRulesParams>(json!({"life_id":"life-1","mode":"gentle"}));
    invalid::<PetRulesParams>(
        json!({"life_id":"life-1","mode":"gentle","acknowledge_rules":true,
        "pace":"playtest"}),
    );
}

#[test]
fn pet_configure_life_params() {
    let ok = |v| parse_params::<PetConfigureLifeParams>(v);
    let p = ok(json!({"life_id":"life-1","expected_revision":4,"grace_hours":12})).unwrap();
    assert_eq!(
        (p.grace_hours, p.critical_after_hours, p.decay_percent),
        (Some(12), None, None)
    );
    let p = ok(
        json!({"life_id":"life-1","expected_revision":0,"grace_hours":48,
        "critical_after_hours":24,"decay_percent":25}),
    )
    .unwrap();
    assert_eq!(p.expected_revision, 0);
    for v in [
        json!({"grace_hours":1}),
        json!({"critical_after_hours":1}),
        json!({"decay_percent":100}),
        json!({"decay_percent":25}),
    ] {
        let mut o = json!({"life_id":"life-1","expected_revision":1});
        o.as_object_mut()
            .unwrap()
            .extend(v.as_object().unwrap().clone());
        assert!(ok(o.clone()).is_ok(), "{o}");
    }
    let bad = |extra: Value| {
        let mut o = json!({"life_id":"life-1","expected_revision":1});
        o.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        invalid::<PetConfigureLifeParams>(o)
    };
    assert!(bad(json!({})).contains("at least one"));
    assert!(bad(json!({"grace_hours":null,"decay_percent":null})).contains("at least one"));
    assert!(bad(json!({"grace_hours":0})).contains("grace_hours"));
    assert!(bad(json!({"grace_hours":49})).contains("grace_hours"));
    assert!(bad(json!({"critical_after_hours":0})).contains("critical_after_hours"));
    assert!(bad(json!({"critical_after_hours":25})).contains("critical_after_hours"));
    assert!(bad(json!({"decay_percent":24})).contains("decay_percent"));
    assert!(bad(json!({"decay_percent":101})).contains("decay_percent"));
    assert!(bad(json!({"decay_percent":0})).contains("decay_percent"));
    // Out of the wire types entirely.
    bad(json!({"decay_percent":256}));
    bad(json!({"grace_hours":-1}));
    bad(json!({"grace_hours":70000}));
    bad(json!({"grace_hours":1.5}));
    // Raw millisecond fields and strictness knobs are not part of the request.
    bad(json!({"grace_ms":3_600_000}));
    bad(json!({"decay_per_hour":[1,1,1,1]}));
    invalid::<PetConfigureLifeParams>(json!({"life_id":"life-1","grace_hours":12}));
    invalid::<PetConfigureLifeParams>(json!({"life_id":"life-1","expected_revision":-1,
        "grace_hours":12}));
    invalid::<PetConfigureLifeParams>(json!({"expected_revision":1,"grace_hours":12}));
    assert_eq!(MAX_GRACE_HOURS as i64 * 3_600_000, 172_800_000);
    assert_eq!(MAX_CRITICAL_AFTER_HOURS as i64 * 3_600_000, 86_400_000);
}

#[test]
fn pet_retire_params() {
    let p: PetRetireParams =
        parse_params(json!({"life_id":"life-1","confirm_name":"Ginger"})).unwrap();
    assert_eq!(p.confirm_name, "Ginger");
    // Name matching (case, whitespace) is storage's call; protocol only bounds the input.
    assert!(
        parse_params::<PetRetireParams>(json!({"life_id":"life-1","confirm_name":"ginger "}))
            .is_ok()
    );
    assert!(
        invalid::<PetRetireParams>(json!({"life_id":"life-1","confirm_name":""}))
            .contains("confirm_name")
    );
    invalid::<PetRetireParams>(json!({"life_id":"life-1","confirm_name":" \t "}));
    invalid::<PetRetireParams>(json!({"life_id":"life-1","confirm_name":"G\u{0}"}));
    invalid::<PetRetireParams>(
        json!({"life_id":"life-1","confirm_name":"x".repeat(MAX_PET_NAME_INPUT_BYTES + 1)}),
    );
    invalid::<PetRetireParams>(json!({"life_id":"life-1"}));
    invalid::<PetRetireParams>(json!({"confirm_name":"Ginger"}));
    invalid::<PetRetireParams>(json!({"life_id":"life-1","confirm_name":"Ginger","force":true}));
}

#[test]
fn pet_events_seen_params() {
    for seq in [0, 1, 88, i64::MAX] {
        let p: PetEventsSeenParams =
            parse_params(json!({"life_id":"life-1","through_seq":seq})).unwrap();
        assert_eq!(p.through_seq, seq);
    }
    assert!(
        invalid::<PetEventsSeenParams>(json!({"life_id":"life-1","through_seq":-1}))
            .contains("through_seq")
    );
    invalid::<PetEventsSeenParams>(json!({"life_id":"life-1","through_seq":"88"}));
    invalid::<PetEventsSeenParams>(json!({"life_id":"life-1","through_seq":u64::MAX}));
    invalid::<PetEventsSeenParams>(json!({"life_id":"life-1"}));
    invalid::<PetEventsSeenParams>(json!({"through_seq":1}));
    invalid::<PetEventsSeenParams>(json!({"life_id":"life-1","through_seq":1,"seen_at_ms":0}));
}

#[test]
fn pet_memorial_params() {
    let p: PetMemorialParams = parse_params(json!({})).unwrap();
    assert_eq!(p, PetMemorialParams::default());
    assert_eq!(p.limit_or_default(), MAX_MEMORIAL_LIMIT);
    assert_eq!(MAX_MEMORIAL_LIMIT, 20);
    let p: PetMemorialParams = parse_params(json!({"before_ordinal":7,"limit":1})).unwrap();
    assert_eq!((p.before_ordinal, p.limit_or_default()), (Some(7), 1));
    assert!(parse_params::<PetMemorialParams>(json!({"limit":20})).is_ok());
    assert!(parse_params::<PetMemorialParams>(json!({"limit":null,"before_ordinal":null})).is_ok());
    assert!(invalid::<PetMemorialParams>(json!({"limit":0})).contains("limit"));
    assert!(invalid::<PetMemorialParams>(json!({"limit":21})).contains("limit"));
    invalid::<PetMemorialParams>(json!({"before_ordinal":-1}));
    invalid::<PetMemorialParams>(json!({"before_ordinal":u64::from(u32::MAX) + 1}));
    invalid::<PetMemorialParams>(json!({"life_id":"life-1"}));
}

#[test]
fn lifecycle_rules_params_are_empty() {
    let _: LifecycleRulesParams = parse_params(json!({})).unwrap();
    invalid::<LifecycleRulesParams>(json!({"pace":"playtest"}));
    invalid::<LifecycleRulesParams>(json!({"ruleset":"lifecycle-v1"}));
}

#[test]
fn new_params_reject_non_objects_and_duplicates() {
    for v in [json!([]), json!(null), json!("cat"), json!(1)] {
        invalid::<PetSelectParams>(v.clone());
        invalid::<PetAdoptParams>(v.clone());
        invalid::<PetMemorialParams>(v);
    }
    let e = req(r#"{"v":1,"id":"a","method":"pet.select","params":{"life_id":"l","pet_id":"cat","pet_id":"dog"}}"#)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidRequest);
}

#[test]
fn lifecycle_requests_end_to_end() {
    let cases = [
        (
            r#"{"v":1,"id":"sel-1","method":"pet.select","params":{"life_id":"life-1","pet_id":"owl"}}"#,
            true,
        ),
        (
            r#"{"v":1,"id":"adopt-1","method":"pet.adopt","params":{"species":"frog","mode":"permadeath","acknowledge_rules":true}}"#,
            true,
        ),
        (
            r#"{"v":1,"id":"rules-1","method":"pet.rules","params":{"life_id":"life-1","mode":"gentle","acknowledge_rules":true}}"#,
            true,
        ),
        (
            r#"{"v":1,"id":"cfg-1","method":"pet.configure_life","params":{"life_id":"life-1","expected_revision":3,"decay_percent":50}}"#,
            true,
        ),
        (
            r#"{"v":1,"id":"ret-1","method":"pet.retire","params":{"life_id":"life-1","confirm_name":"Ginger"}}"#,
            true,
        ),
        (
            r#"{"v":1,"id":"seen-1","method":"pet.events_seen","params":{"life_id":"life-1","through_seq":88}}"#,
            true,
        ),
        (
            r#"{"v":1,"id":"mem-1","method":"pet.memorial","params":{"limit":5}}"#,
            false,
        ),
        (
            r#"{"v":1,"id":"rules-2","method":"lifecycle.rules"}"#,
            false,
        ),
    ];
    for (line, mutating) in cases {
        let mut cur = frames(format!("{line}\n").as_bytes());
        let request = parse_request(&read_frame(&mut cur).unwrap().unwrap()).unwrap();
        assert_eq!(request.method.is_mutating(), mutating, "{line}");
        let p = request.params.clone();
        let ok = match request.method {
            Method::PetSelect => parse_params::<PetSelectParams>(p).is_ok(),
            Method::PetAdopt => parse_params::<PetAdoptParams>(p).is_ok(),
            Method::PetRules => parse_params::<PetRulesParams>(p).is_ok(),
            Method::PetConfigureLife => parse_params::<PetConfigureLifeParams>(p).is_ok(),
            Method::PetRetire => parse_params::<PetRetireParams>(p).is_ok(),
            Method::PetEventsSeen => parse_params::<PetEventsSeenParams>(p).is_ok(),
            Method::PetMemorial => parse_params::<PetMemorialParams>(p).is_ok(),
            Method::LifecycleRules => parse_params::<LifecycleRulesParams>(p).is_ok(),
            other => panic!("unexpected {other}"),
        };
        assert!(ok, "{line}");
    }
}

// ---- v0.3 skeleton (spec §6.2) ----

#[test]
fn v03_method_lists_partition_the_spec_tables() {
    let added: Vec<&str> = Method::V03_ADDED.iter().map(|m| m.as_str()).collect();
    assert_eq!(
        added,
        [
            "pet.start",
            "pet.rename",
            "treats.give",
            "app.set_enabled",
            "app.set_helpers",
            "agent.forget_history",
            "clients.status",
            "clients.connect",
            "clients.disconnect",
            "clients.test"
        ]
    );
    let removed: Vec<&str> = Method::V03_REMOVED.iter().map(|m| m.as_str()).collect();
    assert_eq!(
        removed,
        [
            "events.subscribe",
            "providers.inspect",
            "task.create",
            "task.get",
            "task.update",
            "task.preflight",
            "task.start",
            "task.cancel",
            "task.review",
            "task.logs",
            "task.apply",
            "task.accept_artifact",
            "task.discard",
            "task.rollback",
            "task.recover",
            "history.list",
            "settings.update"
        ]
    );
    for m in Method::V03_ADDED {
        assert!(!Method::V03_REMOVED.contains(&m));
        assert!(m.is_mutating(), "{m}");
    }
    // Kept: everything else (14 methods, including hello).
    let kept = Method::ALL
        .iter()
        .filter(|m| !Method::V03_ADDED.contains(m) && !Method::V03_REMOVED.contains(m))
        .count();
    assert_eq!(kept, 14, "hello + state.get + 12 pet/lifecycle methods");
}

#[test]
fn v03_params_accept_the_contract_shapes() {
    parse_params::<PetStartParams>(json!({
        "species": "cat", "mode": "gentle", "acknowledge_rules": true
    }))
    .unwrap();
    parse_params::<PetStartParams>(json!({
        "life_id": "life-1", "species": "goldfish", "name": "Ginger", "mode": "gentle",
        "acknowledge_rules": false
    }))
    .unwrap();
    parse_params::<PetRenameParams>(json!({"life_id": "life-1", "name": "Ginger"})).unwrap();
    parse_params::<TreatsGiveParams>(json!({"life_id": "life-1"})).unwrap();
    parse_params::<AppSetEnabledParams>(json!({"enabled": false})).unwrap();
    parse_params::<AppSetHelpersParams>(json!({"enabled": true})).unwrap();
    parse_params::<AgentForgetHistoryParams>(json!({})).unwrap();
    parse_params::<ClientsStatusParams>(json!({})).unwrap();
    parse_params::<ClientsConnectParams>(json!({"clients": ["claude-code", "grok-cli"]})).unwrap();
    parse_params::<ClientsConnectParams>(json!({"clients": CLIENT_IDS})).unwrap();
    parse_params::<ClientsDisconnectParams>(json!({"client": "codex"})).unwrap();
    parse_params::<ClientsTestParams>(json!({})).unwrap();
    parse_params::<ClientsTestParams>(json!({"client": "hermes"})).unwrap();
}

#[test]
fn v03_params_refuse_bad_shapes() {
    let bad = [
        (
            "pet.start",
            json!({"species": "unicorn", "mode": "gentle", "acknowledge_rules": true}),
        ),
        (
            "pet.start",
            json!({"species": "cat", "mode": "easy", "acknowledge_rules": true}),
        ),
        (
            "pet.start",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true, "pace": "steady"}),
        ),
        (
            "pet.start",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true, "name": "  "}),
        ),
        (
            "pet.start",
            json!({"species": "cat", "mode": "gentle", "acknowledge_rules": true, "life_id": ""}),
        ),
        ("pet.rename", json!({"life_id": "life-1"})),
        (
            "pet.rename",
            json!({"life_id": "life-1", "name": "a\u{0}b"}),
        ),
        ("treats.give", json!({})),
        ("treats.give", json!({"life_id": "life-1", "count": 2})),
        ("app.set_enabled", json!({"enabled": "yes"})),
        ("app.set_helpers", json!({})),
        ("agent.forget_history", json!({"all": true})),
        ("clients.status", json!({"refresh": true})),
        ("clients.connect", json!({"clients": []})),
        ("clients.connect", json!({"clients": ["claude"]})),
        ("clients.connect", json!({"clients": ["codex", "codex"]})),
        ("clients.connect", json!({"clients": ["muse-code"]})),
        (
            "clients.connect",
            json!({"clients": ["claude-code","codex","opencode","hermes","grok-cli","codex"]}),
        ),
        ("clients.disconnect", json!({"client": "crush"})),
        ("clients.disconnect", json!({})),
        ("clients.test", json!({"client": "gemini"})),
    ];
    for (method, params) in bad {
        let r = match method {
            "pet.start" => parse_params::<PetStartParams>(params.clone()).map(|_| ()),
            "pet.rename" => parse_params::<PetRenameParams>(params.clone()).map(|_| ()),
            "treats.give" => parse_params::<TreatsGiveParams>(params.clone()).map(|_| ()),
            "app.set_enabled" | "app.set_helpers" => {
                parse_params::<EnabledParams>(params.clone()).map(|_| ())
            }
            "agent.forget_history" | "clients.status" => {
                parse_params::<EmptyParams>(params.clone()).map(|_| ())
            }
            "clients.connect" => parse_params::<ClientsConnectParams>(params.clone()).map(|_| ()),
            "clients.disconnect" => {
                parse_params::<ClientsDisconnectParams>(params.clone()).map(|_| ())
            }
            "clients.test" => parse_params::<ClientsTestParams>(params.clone()).map(|_| ()),
            other => panic!("{other}"),
        };
        assert_eq!(
            r.unwrap_err().code,
            ErrorCode::InvalidParams,
            "{method} {params}"
        );
    }
}

#[test]
fn v03_codes_are_not_retryable() {
    for code in [
        ErrorCode::NotImplemented,
        ErrorCode::AlreadyArmed,
        ErrorCode::NameInvalid,
        ErrorCode::NoTreats,
        ErrorCode::PetNeedsCare,
        ErrorCode::TestState,
    ] {
        assert!(!code.retryable(), "{code}");
    }
}

/// `docs/v0.3/contracts/protocol.json` is the WP0 contract: its method tables and
/// param examples must agree with this crate.
#[test]
fn v03_protocol_contract_matches_the_crate() {
    let doc: Value =
        serde_json::from_str(include_str!("../../../docs/v0.3/contracts/protocol.json")).unwrap();
    let names = |key: &str| -> Vec<String> {
        doc[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                v.as_str()
                    .map_or_else(|| v["method"].as_str().unwrap().to_owned(), str::to_owned)
            })
            .collect()
    };
    let added = names("added");
    assert_eq!(
        added,
        Method::V03_ADDED.map(|m| m.as_str().to_owned()).to_vec()
    );
    assert_eq!(
        names("removed"),
        Method::V03_REMOVED.map(|m| m.as_str().to_owned()).to_vec()
    );
    let kept = names("kept");
    assert_eq!(
        kept.len() + added.len() + Method::V03_REMOVED.len(),
        Method::ALL.len()
    );
    for name in &kept {
        let m = Method::parse(name).unwrap();
        assert!(!Method::V03_ADDED.contains(&m) && !Method::V03_REMOVED.contains(&m));
    }
    for code in doc["error_codes_added"].as_object().unwrap().keys() {
        assert!(ErrorCode::parse(code).is_some(), "{code}");
    }
    let clients: Vec<&str> = doc["client_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(clients, CLIENT_IDS);
    for entry in doc["added"].as_array().unwrap() {
        let method = entry["method"].as_str().unwrap();
        let m = Method::parse(method).unwrap();
        assert_eq!(entry["mutating"], m.is_mutating(), "{method}");
        let check = |params: &Value| -> Result<(), ProtocolError> {
            let p = params.clone();
            match m {
                Method::PetStart => parse_params::<PetStartParams>(p).map(drop),
                Method::PetRename => parse_params::<PetRenameParams>(p).map(drop),
                Method::TreatsGive => parse_params::<TreatsGiveParams>(p).map(drop),
                Method::AppSetEnabled => parse_params::<AppSetEnabledParams>(p).map(drop),
                Method::AppSetHelpers => parse_params::<AppSetHelpersParams>(p).map(drop),
                Method::AgentForgetHistory => parse_params::<AgentForgetHistoryParams>(p).map(drop),
                Method::ClientsStatus => parse_params::<ClientsStatusParams>(p).map(drop),
                Method::ClientsConnect => parse_params::<ClientsConnectParams>(p).map(drop),
                Method::ClientsDisconnect => parse_params::<ClientsDisconnectParams>(p).map(drop),
                Method::ClientsTest => parse_params::<ClientsTestParams>(p).map(drop),
                other => panic!("{other} is not a v0.3 method"),
            }
        };
        for valid in entry["examples"]["valid"].as_array().unwrap() {
            check(valid).unwrap_or_else(|e| panic!("{method} {valid}: {e}"));
        }
        for invalid in entry["examples"]["invalid"].as_array().unwrap() {
            assert_eq!(
                check(invalid).unwrap_err().code,
                ErrorCode::InvalidParams,
                "{method} {invalid}"
            );
        }
    }
}
