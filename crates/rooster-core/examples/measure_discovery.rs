//! Read-only measurement of an explicitly supplied disposable configuration.
use rooster_core::{
    CancellationToken, ConfigStore,
    artifacts::{self, InventoryOptions},
    providers::Provider,
};
use std::time::{Duration, Instant};
fn main() {
    let path = std::env::args_os()
        .nth(1)
        .expect("disposable config.json path");
    let provider: Provider = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "codex".into())
        .parse()
        .unwrap();
    let c = ConfigStore::new(path).unwrap().load().unwrap();
    let roots = c.roots(None).unwrap();
    let options = InventoryOptions {
        provider,
        codex: c.codex,
        claude: c.claude,
        all_markdown: true,
        ..Default::default()
    };
    assert!(
        !options.codex.include_default_roots && !options.claude.include_default_roots,
        "disable ambient provider roots in benchmark settings"
    );
    let mut results = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let mut first = None;
        let mut previous = start;
        let mut longest = Duration::ZERO;
        let mut progress = 0;
        let inv = artifacts::inventory(&roots, &options, &CancellationToken::default(), |_| {
            let now = Instant::now();
            first.get_or_insert(now.duration_since(start));
            longest = longest.max(now.duration_since(previous));
            previous = now;
            progress += 1;
        });
        longest = longest.max(previous.elapsed());
        results.push(serde_json::json!({"seconds":start.elapsed().as_secs_f64(),"first_progress_ms":first.map(|t| t.as_secs_f64()*1000.0),"longest_progress_gap_ms":longest.as_secs_f64()*1000.0,"progress_callbacks":progress,"status":inv.status,"checkouts":inv.repositories.checkouts.len(),"artifacts":inv.artifacts.len(),"diagnostics":inv.diagnostics.len()}));
    }
    let cancel = CancellationToken::default();
    let token = cancel.clone();
    let start = Instant::now();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        let requested = Instant::now();
        token.cancel();
        requested
    });
    let inv = artifacts::inventory(&roots, &options, &cancel, |_| {});
    let done = Instant::now();
    let requested = worker.join().unwrap();
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({"provider":provider,"runs":results,"cancellation":{"status":inv.status,"total_ms":done.duration_since(start).as_secs_f64()*1000.0,"response_ms":done.checked_duration_since(requested).map(|d|d.as_secs_f64()*1000.0)}})).unwrap());
}
