//! Latency harness for the kernel across many projects.
//!
//! Usage: `agent-doctor-bench [--max-us-per-file <N>] <dir>...`
//!
//! For each project directory it measures, over repeated runs:
//!   - cold index build (full walk + parallel parse),
//!   - warm incremental update of a single file (`Index::update_file`),
//!   - warm impact selection (Layer 2),
//!   - gate evaluation with a layering rule (Layer 1),
//!   - semantic merge of one file (Layer 3),
//! and prints p50/p95. With `--max-us-per-file` it exits non-zero if any
//! project's cold-build cost per file exceeds the threshold (a CI latency gate).

use std::path::Path;
use std::time::{Duration, Instant};

use agent_doctor_core::Index;
use agent_doctor_impact::{DepGraph, ImpactConfig};
use agent_doctor_merge::merge;
use agent_doctor_policy::{evaluate, GateInput, Policy};

const BUILD_RUNS: usize = 5;
const HOT_RUNS: usize = 20;
const MAX_INCREMENTAL_FILES: usize = 100;

/// Policy with one layering rule, to exercise the import-edge scan in `gate`.
const GATE_POLICY: &str = "[[layer]]\nname = \"bench\"\npath = \"**\"\nforbid_imports_from = [\"__nonexistent__/**\"]\n";

fn main() {
    let mut dirs: Vec<String> = Vec::new();
    let mut max_us_per_file: Option<f64> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-us-per-file" => {
                max_us_per_file = args.next().and_then(|value| value.parse().ok());
            }
            _ => dirs.push(arg),
        }
    }
    if dirs.is_empty() {
        eprintln!("usage: agent-doctor-bench [--max-us-per-file <N>] <dir>...");
        std::process::exit(2);
    }

    println!(
        "{:<24} {:>6} {:>10} {:>10} {:>9} {:>10} {:>9} {:>10}",
        "project", "files", "build p50", "build p95", "incr p50", "impact p50", "gate p50", "merge p50"
    );
    println!("{}", "-".repeat(96));

    let mut regressions = 0;
    for dir in &dirs {
        if let Some(per_file) = bench_project(Path::new(dir)) {
            if let Some(limit) = max_us_per_file {
                if per_file > limit {
                    println!("  ↑ {dir}: {per_file:.1}µs/file exceeds limit {limit:.1}µs/file");
                    regressions += 1;
                }
            }
        }
    }
    if regressions > 0 {
        std::process::exit(1);
    }
}

/// Bench one project; returns cold-build µs per file (for threshold checks).
fn bench_project(root: &Path) -> Option<f64> {
    let name = root
        .file_name()
        .map(|os| os.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    let probe = Index::build(root);
    let file_count = probe.graph().len();
    if file_count == 0 {
        println!("{name:<24} {:>6} {:>10}", 0, "(no TS files)");
        return None;
    }

    let build = measure(BUILD_RUNS, || {
        let _ = Index::build(root);
    });
    let incremental = measure_incremental(root, &probe);
    let impact = measure_impact(&probe);
    let gate = measure_gate(&probe);
    let merge_times = measure_merge(root, &probe);

    let build_p50 = percentile(&build, 50);
    println!(
        "{name:<24} {file_count:>6} {:>10} {:>10} {:>9} {:>10} {:>9} {:>10}",
        ms(build_p50),
        ms(percentile(&build, 95)),
        us(percentile(&incremental, 50)),
        us(percentile(&impact, 50)),
        us(percentile(&gate, 50)),
        us(percentile(&merge_times, 50)),
    );
    Some(build_p50.as_secs_f64() * 1_000_000.0 / file_count as f64)
}

fn measure(runs: usize, mut op: impl FnMut()) -> Vec<Duration> {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            op();
            start.elapsed()
        })
        .collect()
}

fn measure_incremental(root: &Path, index: &Index) -> Vec<Duration> {
    let paths: Vec<String> = index
        .graph()
        .files()
        .take(MAX_INCREMENTAL_FILES)
        .map(|file| file.path.clone())
        .collect();
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

fn measure_impact(index: &Index) -> Vec<Duration> {
    let Some(changed) = first_file(index) else {
        return vec![Duration::ZERO];
    };
    let dep_graph = DepGraph::build(index.graph());
    let changed = [changed];
    let config = ImpactConfig::default();
    measure(HOT_RUNS, || {
        let _ = dep_graph.select(&changed, &config);
    })
}

fn measure_gate(index: &Index) -> Vec<Duration> {
    let Some(changed) = first_file(index) else {
        return vec![Duration::ZERO];
    };
    let policy = Policy::parse(GATE_POLICY).expect("valid bench policy");
    let changed = [changed];
    measure(HOT_RUNS, || {
        let _ = evaluate(&GateInput {
            policy: &policy,
            graph: index.graph(),
            changed: &changed,
            actor: None,
            leases: None,
        });
    })
}

fn measure_merge(root: &Path, index: &Index) -> Vec<Duration> {
    let Some(path) = first_file(index) else {
        return vec![Duration::ZERO];
    };
    let Ok(base) = std::fs::read_to_string(root.join(&path)) else {
        return vec![Duration::ZERO];
    };
    let theirs = format!("{base}\nexport function __bench_added() {{ return 1 }}\n");
    measure(HOT_RUNS, || {
        let _ = merge(&base, &base, &theirs);
    })
}

fn first_file(index: &Index) -> Option<String> {
    index.graph().files().next().map(|file| file.path.clone())
}

fn percentile(samples: &[Duration], p: usize) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted[(p * (sorted.len() - 1)) / 100]
}

fn ms(duration: Duration) -> String {
    format!("{:.2}ms", duration.as_secs_f64() * 1000.0)
}

fn us(duration: Duration) -> String {
    format!("{:.1}µs", duration.as_secs_f64() * 1_000_000.0)
}
