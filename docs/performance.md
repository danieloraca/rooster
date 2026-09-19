# Discovery measurements

Rooster performs fresh filesystem discovery and bounded Git inspection. The desktop runs this work on a Rust background worker and exposes count-based progress and cancellation. There is no persistent index. These are local fixture measurements, not a promise for network drives, cold disks, or arbitrary repositories.

## Reproduce

From the repository root, generate a NEW disposable directory outside any real checkout, then run the read-only core measurement harness:

~~~sh
python3 scripts/generate-benchmark.py /absolute/path/to/new-benchmark --repositories 10 --documents 500
cargo run --release --locked -p rooster-core --example measure_discovery -- /absolute/path/to/new-benchmark/config.json codex
cargo run --release --locked -p rooster-core --example measure_discovery -- /absolute/path/to/new-benchmark/config.json claude
~~~

The default workload contains ten unborn repositories, 5,000 ordinary Markdown files (~938 bytes each), one instruction for each provider in every repository, 500 excluded dependency directories, and 500 deliberately excluded instruction files. Each selected provider should see ten checkouts and 5,010 artifacts. The measurement excludes compilation/generation and prints three elapsed times, first progress, longest callback gap (including final processing), artifact counts, and cancellation response after a request at 100 ms. Run on an otherwise quiet machine; report debug/release and warm-cache conditions. Do not infer desktop render time or peak memory from core timing.

## Tested limits

Measured on 18 September 2026, macOS 26.6.2 on Apple Silicon, Git 2.39.2, Rust 1.95.0 release builds, local storage with warm filesystem caches. Generation and compilation finished before measurement. All six full scans completed with 10 checkouts, 5,010 artifacts, and zero diagnostics.

| Provider | Three runs (seconds) | Mean | First progress | Largest callback gap | Cancellation response |
| --- | --- | --- | --- | --- | --- |
| Codex | 4.511 / 4.519 / 3.858 | 4.296 s | 0.077–0.269 ms | 1.120 s | 5.485 ms |
| Claude | 4.267 / 3.929 / 3.803 | 4.000 s | 0.071–0.669 ms | 0.896 s | 7.954 ms |

Each complete scan emitted 1,041 progress callbacks. Cancellation was requested after 100 ms, during repository discovery; it returned an explicitly cancelled inventory. These figures do not measure cancellation at every later parsing phase. The core remained suitable for the background-scanning pilot at this scale; no measured regression justified adding a cache, parallel Git subprocesses, or an indexing service. The roughly one-second gaps include Git visibility checks and final parsing between callbacks. The app's native pilot used three small checkouts; rendering 5,010 rows, peak memory, cold-cache timing, network disks, and larger repository sets were not measured.

 The default inventory caps remain 20,000 candidate files, 1 MiB per source file, and 64 MiB total source bytes. Reaching a cap produces a partial/diagnostic result; these are safety limits, not verified capacity promises. Package mutations have their own recovery limits, described in [mutation behavior](mutations.md).

Git subprocesses have a five-second timeout and a 4 MiB output cap. Many repositories, slow Git configuration/filesystems, or CPU contention can dominate discovery even when Markdown content is small. Register narrower roots and exclude irrelevant directories when appropriate. Background progress/manual cancellation are retained; no indexing daemon or unmeasured caching optimization is introduced.

During parallel compile/test work, an early targeted run observed incomplete inventories in two fixtures; both cases passed with normal native process access and bounded test concurrency. Keep benchmarking separate from those loaded runs. Filesystem-event tests likewise need normal native event delivery. CI failure diagnostics should retain partial scan issues instead of treating missing artifacts as parser failures.
