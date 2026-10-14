use std::process::Command;
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_codepet"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn no_arguments_do_not_start_a_task() {
    let output = run(&[]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("No daemon"));
    assert!(!text.contains("seq="));
}
#[test]
fn demo_stops_at_unverified_review_without_reward() {
    let output = run(&["mock-demo"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("state=ReviewReady"));
    assert!(text.contains("Mental health=70/100"));
    assert!(!text.contains("state=Accepted"));
}
#[test]
fn real_provider_command_is_rejected() {
    let output = run(&["codex"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
