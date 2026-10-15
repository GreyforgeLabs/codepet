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
    ];
    assert_eq!(ErrorCode::ALL.len(), expected.len());
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
    ];
    assert_eq!(Method::ALL.len(), names.len());
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
    let reads = [
        Method::Hello,
        Method::StateGet,
        Method::EventsSubscribe,
        Method::ProvidersInspect,
        Method::TaskGet,
        Method::TaskReview,
        Method::TaskLogs,
        Method::HistoryList,
    ];
    for m in Method::ALL {
        assert_eq!(m.is_mutating(), !reads.contains(&m), "{m}");
    }
    for m in [Method::TaskLogs, Method::TaskApply, Method::TaskRollback] {
        assert!(!m.is_implemented());
        assert!(req(&format!(
            r#"{{"v":1,"id":"a","method":"{m}","params":{{}}}}"#
        ))
        .is_ok());
    }
    assert!(Method::TaskRecover.is_implemented());
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
