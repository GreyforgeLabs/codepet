//! `codepet mock-demo`: an explicit in-memory simulation. Touches no state or files.
use codepet_core::{Category, Draft, Engine, Provider, Verification};

pub fn run() {
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
    let digest = engine
        .mock_review(
            id,
            "SIMULATED artifact: draft -> explicit Start -> running -> review -> explicit acceptance."
                .into(),
            Verification::NotVerified,
        )
        .unwrap();
    for event in engine.events_after(0).unwrap() {
        println!(
            "seq={} job={} state={:?}",
            event.seq, event.job_id, event.state
        );
    }
    println!(
        "Review digest={digest}; NOT VERIFIED; no acceptance or reward occurs in this demo. \
         Mental health={}/100. No files touched and no provider launched.",
        engine.meters().hundredths(Category::MentalHealth) / 100
    );
}
