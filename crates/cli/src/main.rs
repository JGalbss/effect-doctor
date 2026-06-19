use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use agent_doctor_core::{
    all_metas, example_for, scan, Diagnostic, FileContext, ScanOptions, ScanResult, ScanScope,
    Severity, SCORE_GOOD_THRESHOLD, SCORE_OK_THRESHOLD,
};
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Clone, Copy, ValueEnum)]
enum ScopeArg {
    /// Whole repository
    Full,
    /// All issues in files changed vs --base (plus untracked files)
    Changed,
    /// Only issues on lines changed vs --base
    Lines,
}

impl From<ScopeArg> for ScanScope {
    fn from(scope: ScopeArg) -> ScanScope {
        match scope {
            ScopeArg::Full => ScanScope::Full,
            ScopeArg::Changed => ScanScope::ChangedFiles,
            ScopeArg::Lines => ScanScope::ChangedLines,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "agent-doctor",
    version,
    about = "Health checks for Effect TS codebases"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Directory to scan
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Show every rule group (default: top 3)
    #[arg(long)]
    verbose: bool,

    /// Emit the report as JSON
    #[arg(long)]
    json: bool,

    /// Max locations printed per rule group
    #[arg(long, default_value_t = 5)]
    max_locations: usize,

    /// Run v4-migration rules even on an effect v3 codebase (migration audit)
    #[arg(long)]
    migrate: bool,

    /// What to scan: the whole repo, changed files, or changed lines only
    #[arg(long, value_enum, default_value_t = ScopeArg::Full)]
    scope: ScopeArg,

    /// Diff base ref for --scope changed/lines (default: merge-base with main)
    #[arg(long)]
    base: Option<String>,

    /// Also run the type-aware tier (@effect/language-service) and merge it
    #[arg(long)]
    deep: bool,

    /// Experimental: recommend vanilla-TS → Effect migrations (async fns,
    /// .then chains, Promise.all, awaits in loops)
    #[arg(long)]
    adopt: bool,

    /// Experimental "agent doctor": flag non-Effect slop LLM agents emit —
    /// if/else chains, ternaries, string-equality guards, raw loops, `let`,
    /// duplicated function bodies (warn)
    #[arg(long)]
    agent: bool,

    /// Escalate --agent findings to errors (hard-fail). Implies --agent.
    #[arg(long)]
    agent_strict: bool,
}

mod lsp;

#[derive(Subcommand)]
enum Command {
    /// Show what a rule means and how to rewrite the code cleanly
    Explain {
        /// Rule id (e.g. require-yield-star)
        rule: String,
    },
    /// List all rules
    Rules {
        /// Emit the full rule catalog (with rewrite examples) as JSON
        #[arg(long)]
        json: bool,
    },
    /// Run as a language server over stdio (editor diagnostics)
    Lsp,
    /// Select the tests impacted by the working diff (impact-based selection)
    Impact {
        /// Diff base ref (default: merge-base with main)
        #[arg(long)]
        base: Option<String>,
        /// Emit the result as JSON
        #[arg(long)]
        json: bool,
        /// Tests to always include regardless of the diff (repeatable)
        #[arg(long = "always-run")]
        always_run: Vec<String>,
    },
    /// Gate the working diff against policy/ACL/leases (deterministic deny)
    Gate {
        /// Diff base ref (default: merge-base with main)
        #[arg(long)]
        base: Option<String>,
        /// Acting agent id (enables ACL + lease checks)
        #[arg(long)]
        actor: Option<String>,
        /// Policy file (default: agent-doctor.policy.toml)
        #[arg(long, default_value = "agent-doctor.policy.toml")]
        policy: PathBuf,
        /// Leases file (default: .agent-doctor/leases.json)
        #[arg(long, default_value = ".agent-doctor/leases.json")]
        leases: PathBuf,
        /// Emit violations as JSON
        #[arg(long)]
        json: bool,
    },
    /// Semantic (AST-level) 3-way merge driver for TypeScript files
    Merge {
        /// Base (common ancestor) file
        base: PathBuf,
        /// Ours (current) file — receives the merged result unless --output
        ours: PathBuf,
        /// Theirs (other) file
        theirs: PathBuf,
        /// Write merged output here instead of overwriting <ours>
        #[arg(long)]
        output: Option<PathBuf>,
        /// Print the merge result as JSON instead of writing a file
        #[arg(long)]
        json: bool,
    },
    /// Run the context server: warm kernel answering line-delimited JSON queries
    Serve {
        /// Policy file (default: agent-doctor.policy.toml)
        #[arg(long, default_value = "agent-doctor.policy.toml")]
        policy: PathBuf,
        /// Leases file (default: .agent-doctor/leases.json)
        #[arg(long, default_value = ".agent-doctor/leases.json")]
        leases: PathBuf,
    },
}

fn run_explain(rule_id: &str) -> ExitCode {
    let p = palette();
    let Some(meta) = all_metas().into_iter().find(|meta| meta.id == rule_id) else {
        eprintln!("unknown rule: {rule_id} — see `agent-doctor rules`");
        return ExitCode::from(2);
    };
    println!();
    println!(
        "  {}{}{}  {}{} · {}{}",
        p.bold,
        meta.id,
        p.reset,
        p.dim,
        severity_name(meta.severity),
        meta.category.label(),
        p.reset
    );
    println!();
    println!("  {}", meta.help);
    if let Some(example) = example_for(rule_id) {
        println!();
        println!("  {}✖ instead of{}", p.red, p.reset);
        for line in example.bad.lines() {
            println!("    {}{}{}", p.dim, line, p.reset);
        }
        println!();
        println!("  {}✓ write{}", p.green, p.reset);
        for line in example.good.lines() {
            println!("    {line}");
        }
    }
    println!();
    ExitCode::SUCCESS
}

fn run_rules(json: bool) -> ExitCode {
    let metas = all_metas();
    if json {
        let catalog: Vec<serde_json::Value> = metas
            .iter()
            .map(|meta| {
                let example = example_for(meta.id);
                serde_json::json!({
                    "id": meta.id,
                    "severity": severity_name(meta.severity),
                    "category": meta.category.label(),
                    "help": meta.help,
                    "bad": example.as_ref().map(|example| example.bad),
                    "good": example.as_ref().map(|example| example.good),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&catalog).expect("serializable catalog")
        );
        return ExitCode::SUCCESS;
    }
    let p = palette();
    println!();
    let mut sorted = metas;
    sorted.sort_by_key(|meta| (meta.severity, meta.id));
    for meta in &sorted {
        println!(
            "  {}{:<42}{} {}{:<5} {}{}",
            p.bold,
            meta.id,
            p.reset,
            severity_color(p, meta.severity),
            severity_name(meta.severity),
            meta.category.label(),
            p.reset
        );
    }
    println!();
    println!(
        "  {}{} rules — `agent-doctor explain <rule>` for rewrite recipes{}",
        p.dim,
        sorted.len(),
        p.reset
    );
    println!();
    ExitCode::SUCCESS
}

/// `agent-doctor impact` — build the index, diff against the base, and report
/// the tests reaching the change.
fn run_impact(
    root: &std::path::Path,
    base: Option<&str>,
    json: bool,
    always_run: Vec<String>,
) -> ExitCode {
    let resolved_base = match agent_doctor_core::resolve_base(root, base) {
        Ok(base) => base,
        Err(error) => {
            eprintln!("agent-doctor impact: {error}");
            return ExitCode::from(2);
        }
    };
    let diff = match agent_doctor_core::collect_diff(root, &resolved_base, false) {
        Ok(diff) => diff,
        Err(error) => {
            eprintln!("agent-doctor impact: {error}");
            return ExitCode::from(2);
        }
    };
    let mut changed: Vec<String> = diff.files.keys().cloned().collect();
    changed.sort();
    let index = agent_doctor_core::Index::build(root);
    let result = agent_doctor_impact::select(
        index.graph(),
        &changed,
        &agent_doctor_impact::ImpactConfig { always_run },
    );

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serializable impact result")
        );
        return ExitCode::SUCCESS;
    }

    let p = palette();
    println!();
    println!(
        "  {}impact{}  {}{} changed file{} → {} test{} to run{}",
        p.bold,
        p.reset,
        p.dim,
        changed.len(),
        plural(changed.len()),
        result.tests.len(),
        plural(result.tests.len()),
        p.reset
    );
    println!();
    for test in &result.tests {
        println!("  {}{}{}", p.cyan, test, p.reset);
    }
    if result.tests.is_empty() {
        println!("  {}no tests reach this change{}", p.dim, p.reset);
    }
    for caveat in &result.caveats {
        println!();
        println!("  {}⚠ {}{}", p.yellow, caveat, p.reset);
    }
    println!();
    ExitCode::SUCCESS
}

/// `agent-doctor gate` — evaluate the working diff against policy + leases.
/// Exits non-zero when any violation is found (CI/agent gate).
fn run_gate(
    root: &std::path::Path,
    base: Option<&str>,
    actor: Option<&str>,
    policy_path: &std::path::Path,
    leases_path: &std::path::Path,
    json: bool,
) -> ExitCode {
    let policy = match agent_doctor_policy::Policy::load(policy_path) {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("agent-doctor gate: policy: {error}");
            return ExitCode::from(2);
        }
    };
    let resolved_base = match agent_doctor_core::resolve_base(root, base) {
        Ok(base) => base,
        Err(error) => {
            eprintln!("agent-doctor gate: {error}");
            return ExitCode::from(2);
        }
    };
    let diff = match agent_doctor_core::collect_diff(root, &resolved_base, false) {
        Ok(diff) => diff,
        Err(error) => {
            eprintln!("agent-doctor gate: {error}");
            return ExitCode::from(2);
        }
    };
    let leases = match agent_doctor_policy::LeaseSet::load(leases_path) {
        Ok(leases) => leases,
        Err(error) => {
            eprintln!("agent-doctor gate: leases: {error}");
            return ExitCode::from(2);
        }
    };
    let mut changed: Vec<String> = diff.files.keys().cloned().collect();
    changed.sort();
    let index = agent_doctor_core::Index::build(root);
    let violations = agent_doctor_policy::evaluate(&agent_doctor_policy::GateInput {
        policy: &policy,
        graph: index.graph(),
        changed: &changed,
        actor,
        leases: Some(&leases),
    });

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&violations).expect("serializable violations")
        );
        return gate_exit(&violations);
    }

    let p = palette();
    println!();
    if violations.is_empty() {
        println!("  {}✓ gate passed{} — no policy violations", p.green, p.reset);
        println!();
        return ExitCode::SUCCESS;
    }
    println!(
        "  {}✖ gate failed{} — {} violation{}",
        p.red,
        p.reset,
        violations.len(),
        plural(violations.len())
    );
    println!();
    for violation in &violations {
        println!(
            "  {}{:?}{} {}{}{}",
            p.red,
            violation.kind,
            p.reset,
            p.cyan,
            violation.file,
            p.reset
        );
        println!("    {}{}{}", p.dim, violation.reason, p.reset);
    }
    println!();
    gate_exit(&violations)
}

fn gate_exit(violations: &[agent_doctor_policy::Violation]) -> ExitCode {
    if violations.is_empty() {
        return ExitCode::SUCCESS;
    }
    ExitCode::FAILURE
}

/// `agent-doctor merge` — semantic 3-way merge. Writes the result to `<ours>`
/// (git merge-driver convention) or `--output`, and exits non-zero on conflict.
fn run_merge(
    base: &std::path::Path,
    ours: &std::path::Path,
    theirs: &std::path::Path,
    output: Option<&std::path::Path>,
    json: bool,
) -> ExitCode {
    let read = |path: &std::path::Path| std::fs::read_to_string(path);
    let (base_src, ours_src, theirs_src) = match (read(base), read(ours), read(theirs)) {
        (Ok(b), Ok(o), Ok(t)) => (b, o, t),
        _ => {
            eprintln!("agent-doctor merge: could not read one of the input files");
            return ExitCode::from(2);
        }
    };
    let result = agent_doctor_merge::merge(&base_src, &ours_src, &theirs_src);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serializable merge result")
        );
        return merge_exit(result.is_clean());
    }

    let destination = output.unwrap_or(ours);
    if let Err(error) = std::fs::write(destination, &result.merged) {
        eprintln!("agent-doctor merge: write {}: {error}", destination.display());
        return ExitCode::from(2);
    }
    let p = palette();
    if result.is_clean() {
        eprintln!(
            "  {}✓ merged cleanly{}{}",
            p.green,
            p.reset,
            if result.fell_back { " (line fallback)" } else { "" }
        );
    } else {
        eprintln!(
            "  {}✖ {} conflict{}{} — markers written to {}",
            p.red,
            result.conflicts.len(),
            plural(result.conflicts.len()),
            p.reset,
            destination.display()
        );
        for conflict in &result.conflicts {
            eprintln!("    {}{}{}", p.dim, conflict.description, p.reset);
        }
    }
    merge_exit(result.is_clean())
}

fn merge_exit(clean: bool) -> ExitCode {
    if clean {
        return ExitCode::SUCCESS;
    }
    ExitCode::FAILURE
}

/// `agent-doctor serve` — build the warm kernel and answer JSON queries on stdio.
fn run_serve(
    root: &std::path::Path,
    policy: &std::path::Path,
    leases: &std::path::Path,
) -> ExitCode {
    let mut kernel = match agent_doctor_server::Kernel::build(root, policy, leases) {
        Ok(kernel) => kernel,
        Err(error) => {
            eprintln!("agent-doctor serve: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = agent_doctor_server::serve(&mut kernel) {
        eprintln!("agent-doctor serve: {error}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match &cli.command {
        Some(Command::Explain { rule }) => return run_explain(rule),
        Some(Command::Rules { json }) => return run_rules(*json),
        Some(Command::Lsp) => {
            if let Err(error) = lsp::run() {
                eprintln!("agent-doctor lsp: {error}");
                return ExitCode::from(2);
            }
            return ExitCode::SUCCESS;
        }
        Some(Command::Impact {
            base,
            json,
            always_run,
        }) => return run_impact(&cli.path, base.as_deref(), *json, always_run.clone()),
        Some(Command::Gate {
            base,
            actor,
            policy,
            leases,
            json,
        }) => {
            return run_gate(
                &cli.path,
                base.as_deref(),
                actor.as_deref(),
                policy,
                leases,
                *json,
            )
        }
        Some(Command::Merge {
            base,
            ours,
            theirs,
            output,
            json,
        }) => return run_merge(base, ours, theirs, output.as_deref(), *json),
        Some(Command::Serve { policy, leases }) => return run_serve(&cli.path, policy, leases),
        None => {}
    }
    let result = match scan(&ScanOptions {
        root: cli.path.clone(),
        migrate: cli.migrate,
        scope: cli.scope.into(),
        base: cli.base.clone(),
        deep: cli.deep,
        adopt: cli.adopt,
        agent: cli.agent || cli.agent_strict,
        agent_strict: cli.agent_strict,
    }) {
        Ok(result) => result,
        Err(message) => {
            eprintln!("agent-doctor: {message}");
            return ExitCode::from(2);
        }
    };

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).expect("serializable report")
        );
        return exit_code(&result, cli.agent_strict);
    }

    render(&result, cli.verbose, cli.max_locations);
    exit_code(&result, cli.agent_strict)
}

/// `--agent-strict` turns the scan into a gate: any error-severity finding
/// (its own escalated rules included) fails the process. Without it the scan
/// stays report-only and always exits 0.
fn exit_code(result: &ScanResult, agent_strict: bool) -> ExitCode {
    if !agent_strict {
        return ExitCode::SUCCESS;
    }
    let has_error = result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error);
    if has_error {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

struct Palette {
    red: &'static str,
    yellow: &'static str,
    green: &'static str,
    cyan: &'static str,
    dim: &'static str,
    bold: &'static str,
    reset: &'static str,
}

const COLOR: Palette = Palette {
    red: "\x1b[31m",
    yellow: "\x1b[33m",
    green: "\x1b[32m",
    cyan: "\x1b[36m",
    dim: "\x1b[2m",
    bold: "\x1b[1m",
    reset: "\x1b[0m",
};

const PLAIN: Palette = Palette {
    red: "",
    yellow: "",
    green: "",
    cyan: "",
    dim: "",
    bold: "",
    reset: "",
};

fn palette() -> &'static Palette {
    if std::io::stdout().is_terminal() {
        return &COLOR;
    }
    &PLAIN
}

fn severity_color(palette: &Palette, severity: Severity) -> &'static str {
    match severity {
        Severity::Error => palette.red,
        Severity::Warn => palette.yellow,
        Severity::Info => palette.cyan,
    }
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warn => "warn",
        Severity::Info => "info",
    }
}

fn score_color(palette: &Palette, score: u32) -> &'static str {
    if score >= SCORE_GOOD_THRESHOLD {
        return palette.green;
    }
    if score >= SCORE_OK_THRESHOLD {
        return palette.yellow;
    }
    palette.red
}

fn render_score_bar(score: u32) -> String {
    let filled = (score as usize * 30) / 100;
    format!("{}{}", "█".repeat(filled), "░".repeat(30 - filled))
}

fn render(result: &ScanResult, verbose: bool, max_locations: usize) {
    let p = palette();
    let score = &result.score;
    let color = score_color(p, score.score);

    println!();
    println!(
        "  {}agent doctor{}  {}v{}{}",
        p.bold,
        p.reset,
        p.dim,
        env!("CARGO_PKG_VERSION"),
        p.reset
    );
    println!();
    println!(
        "  {}{}{}{}  {}{}/100 — {}{}",
        color,
        render_score_bar(score.score),
        p.reset,
        p.bold,
        color,
        score.score,
        score.label,
        p.reset
    );
    println!();
    render_category_breakdown(p, result);

    // Group by rule, order groups by (severity, count desc).
    let mut groups: BTreeMap<&str, Vec<&Diagnostic>> = BTreeMap::new();
    for diagnostic in &result.diagnostics {
        groups.entry(diagnostic.rule).or_default().push(diagnostic);
    }
    let mut ordered: Vec<(&str, Vec<&Diagnostic>)> = groups.into_iter().collect();
    ordered.sort_by(|a, b| {
        let severity_a = a.1[0].severity;
        let severity_b = b.1[0].severity;
        (severity_a, std::cmp::Reverse(a.1.len())).cmp(&(severity_b, std::cmp::Reverse(b.1.len())))
    });

    let shown_groups = if verbose {
        ordered.len()
    } else {
        ordered.len().min(3)
    };
    for (rule, diagnostics) in ordered.iter().take(shown_groups) {
        let severity = diagnostics[0].severity;
        let severity_paint = severity_color(p, severity);
        println!(
            "  {}✖ {}{}{}  {}{} · {} · {} issue{}{}",
            severity_paint,
            p.reset,
            p.bold,
            rule,
            p.reset,
            severity_name(severity),
            diagnostics[0].category.label(),
            diagnostics.len(),
            plural(diagnostics.len()),
            p.reset
        );
        println!("    {}{}{}", p.dim, diagnostics[0].help, p.reset);
        for diagnostic in diagnostics.iter().take(max_locations) {
            println!(
                "    {}{}:{}:{}{}  {}{}",
                p.cyan,
                diagnostic.file,
                diagnostic.line,
                diagnostic.column,
                p.reset,
                diagnostic.snippet.trim(),
                test_marker(p, diagnostic)
            );
        }
        let hidden = diagnostics.len().saturating_sub(max_locations);
        if hidden > 0 {
            println!("    {}… and {} more{}", p.dim, hidden, p.reset);
        }
        println!();
    }

    let hidden_groups = ordered.len().saturating_sub(shown_groups);
    if hidden_groups > 0 {
        println!(
            "  {}{} more rule group{} hidden — rerun with --verbose{}",
            p.dim,
            hidden_groups,
            plural(hidden_groups),
            p.reset
        );
        println!();
    }

    println!(
        "  {}Scanned {} files ({} using effect{}) in {}ms — {} issue{}{}",
        p.dim,
        result.files_scanned,
        result.effect_files,
        effect_profile_label(result),
        result.duration_ms,
        result.diagnostics.len(),
        plural(result.diagnostics.len()),
        p.reset
    );
    println!();
}

fn effect_profile_label(result: &ScanResult) -> String {
    let Some(major) = result.effect_major else {
        return String::new();
    };
    if result.v4_rules_active {
        return format!(", effect v{major}, v4 rules on");
    }
    format!(", effect v{major}")
}

fn render_category_breakdown(p: &Palette, result: &ScanResult) {
    if result.diagnostics.is_empty() {
        return;
    }
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for diagnostic in &result.diagnostics {
        let label = diagnostic.category.label();
        match counts.iter_mut().find(|(name, _)| *name == label) {
            Some((_, count)) => *count += 1,
            None => counts.push((label, 1)),
        }
    }
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let parts: Vec<String> = counts
        .iter()
        .map(|(label, count)| format!("{label} {count}"))
        .collect();
    println!("  {}{}{}", p.dim, parts.join(" · "), p.reset);
    println!();
}

fn test_marker(p: &Palette, diagnostic: &Diagnostic) -> String {
    if diagnostic.file_context != FileContext::Test {
        return String::new();
    }
    format!("  {}(test — not scored){}", p.dim, p.reset)
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        return "";
    }
    "s"
}
