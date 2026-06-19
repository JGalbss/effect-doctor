//! Latency harness for the kernel across many projects.
//!
//! Usage: `agent-doctor-bench <dir>...`
//!
//! For each project directory it measures, over repeated runs:
//!   - cold index build (full walk + parallel parse),
//!   - warm incremental update of a single file (`Index::update_file`),
//!   - impact selection for a one-file change.
//! and prints p50/p95 so regressions and scaling behaviour are visible.

use std::path::Path;
use std::time::{Duration, Instant};

use agent_doctor_core::Index;
use agent_doctor_impact::{DepGraph, ImpactConfig};

/// How many times to repeat each measured operation.
const BUILD_RUNS: usize = 5;
const IMPACT_RUNS: usize = 20;
/// Cap on per-file incremental updates measured (keeps the run bounded).
const MAX_INCREMENTAL_FILES: usize = 100;

fn main() {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    if dirs.is_empty() {
        eprintln!("usage: agent-doctor-bench <dir>...");
        std::process::exit(2);
    }
    println!(
        "{:<28} {:>7} {:>11} {:>11} {:>12} {:>12} {:>11}",
        "project", "files", "build p50", "build p95", "incr p50", "incr p95", "impact p50"
    );
    println!("{}", "-".repeat(96));
    for dir in &dirs {
        bench_project(Path::new(dir));
    }
}

fn bench_project(root: &Path) {
    let name = root
        .file_name()
        .map(|os| os.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    let probe = Index::build(root);
    let file_count = probe.graph().len();
    if file_count == 0 {
        println!("{name:<28} {:>7} {:>11}", 0, "(no TS files)");
        return;
    }

    let build = measure(BUILD_RUNS, || {
        let _ = Index::build(root);
    });
    let incremental = measure_incremental(root, &probe);
    let impact = measure_impact(&probe);

    println!(
        "{name:<28} {file_count:>7} {:>11} {:>11} {:>12} {:>12} {:>11}",
        ms(percentile(&build, 50)),
        ms(percentile(&build, 95)),
        us(percentile(&incremental, 50)),
        us(percentile(&incremental, 95)),
        us(percentile(&impact, 50)),
    );
}

/// Time `op` `runs` times, returning every duration.
fn measure(runs: usize, mut op: impl FnMut()) -> Vec<Duration> {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            op();
            start.elapsed()
        })
        .collect()
}

/// Time a warm single-file re-parse for up to [`MAX_INCREMENTAL_FILES`] files.
fn measure_incremental(root: &Path, index: &Index) -> Vec<Duration> {
    let paths: Vec<String> = index
        .graph()
        .files()
        .take(MAX_INCREMENTAL_FILES)
        .map(|file| file.path.clone())
        .collect();
    // Clone so the original probe index stays intact for the impact pass.
    let mut working = Index::build(root);
    paths
        .iter()
        .map(|path| {
            let start = Instant::now();
            working.update_file(path);
            start.elapsed()
        })
        .collect()
}

/// Time the warm impact hot path: with the dependency graph already built
/// (as a server keeps it), selecting for a single changed file.
fn measure_impact(index: &Index) -> Vec<Duration> {
    let Some(changed) = index.graph().files().next().map(|file| file.path.clone()) else {
        return vec![Duration::ZERO];
    };
    let dep_graph = DepGraph::build(index.graph());
    let changed = [changed];
    let config = ImpactConfig::default();
    measure(IMPACT_RUNS, || {
        let _ = dep_graph.select(&changed, &config);
    })
}

/// The p-th percentile (0..=100) of a duration sample, nearest-rank.
fn percentile(samples: &[Duration], p: usize) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (p * (sorted.len() - 1)) / 100;
    sorted[rank]
}

fn ms(duration: Duration) -> String {
    format!("{:.2}ms", duration.as_secs_f64() * 1000.0)
}

fn us(duration: Duration) -> String {
    format!("{:.1}µs", duration.as_secs_f64() * 1_000_000.0)
}
