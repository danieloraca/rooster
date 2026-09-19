//! Export synthetic test data through the real desktop service for browser-only QA.
use rooster_desktop::session::{ScanRequest, Session};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
fn main() {
    assert!(
        std::env::var_os("ROOSTER_CONFIG").is_some(),
        "Use an explicit disposable ROOSTER_CONFIG"
    );
    let out = std::env::args_os().nth(1).expect("output JSON path");
    let session = Arc::new(Session::from_environment().unwrap());
    let settings = session.settings().unwrap();
    let ticket = session
        .start(ScanRequest {
            provider: std::env::args()
                .nth(2)
                .map(|s| s.parse().expect("codex or claude"))
                .unwrap_or_default(),
            workspace_id: None,
            all_markdown: true,
            include_personal: settings.include_personal,
        })
        .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    let generation = loop {
        let status = session.status().unwrap();
        if status.completed == ticket {
            break status.generation;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(30));
    };
    let inventory = session.inventory(generation).unwrap();
    let inspections: std::collections::BTreeMap<_, _> = inventory
        .artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.id.clone(),
                session.inspect(generation, &artifact.id).unwrap(),
            )
        })
        .collect();
    let scopes: std::collections::BTreeMap<_, _> = inventory
        .contexts
        .iter()
        .map(|context| {
            (
                context.id.clone(),
                session.assess(generation, &context.id).unwrap(),
            )
        })
        .collect();
    std::fs::write(out,serde_json::to_vec_pretty(&serde_json::json!({"settings":settings,"inventory":inventory,"inspections":inspections,"scopes":scopes})).unwrap()).unwrap();
}
