use codepet_core::{Category, Draft, Engine, Provider, Verification};
fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("providers") => println!("Mock: available (simulation only)\nCodex: unsupported\nOpenCode: unsupported\nHermes: unsupported\nClaude Code: unsupported"),
        Some("mock-demo") => demo(),
        None | Some("--help") | Some("help") => println!("CodePet prototype\n  codepet providers   Show mock-only support\n  codepet mock-demo   Explicitly run an in-memory simulation\nNo daemon, real provider, project IO, persistence, or background jobs."),
        Some(other) => { eprintln!("Unsupported command: {other}. Use --help."); std::process::exit(2); }
    }
}
fn demo() {
    let mut engine = Engine::default();
    let id = engine
        .create(Draft {
            category: Category::MentalHealth,
            provider: Provider::Mock,
            folder: "/mock/project".into(),
            scope: vec!["src".into()],
            instructions: "Explain a mock state machine; do not read or edit files".into(),
        })
        .expect("valid mock draft");
    let token = engine.preflight(id).expect("mock validation");
    engine
        .start(id, token)
        .expect("explicit mock-demo invocation");
    engine.mock_prepared(id).unwrap();
    engine.mock_exited(id, true).unwrap();
    let digest = engine.mock_review(id,"SIMULATED artifact: draft -> explicit Start -> running -> review -> explicit acceptance.".into(),Verification::NotVerified).unwrap();
    for event in engine.events_after(0).unwrap() {
        println!(
            "seq={} job={} state={:?}",
            event.seq, event.job_id, event.state
        );
    }
    println!("Review digest={digest}; NOT VERIFIED; no acceptance or reward occurs in this demo. Mental health={}/100. No files touched and no provider launched.",engine.meters().hundredths(Category::MentalHealth)/100);
}
