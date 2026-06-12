use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use effect_doctor_core::{
    all_metas, example_for, scan, Diagnostic, FileContext, ScanOptions, ScanResult, ScanScope,
    Severity, SCORE_GOOD_THRESHOLD, SCORE_OK_THRESHOLD,
};

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
#[command(name = "effect-doctor", version, about = "Health checks for Effect TS codebases")]
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
}

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
}

fn run_explain(rule_id: &str) -> ExitCode {
    let p = palette();
    let Some(meta) = all_metas().into_iter().find(|meta| meta.id == rule_id) else {
        eprintln!("unknown rule: {rule_id} — see `effect-doctor rules`");
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
        "  {}{} rules — `effect-doctor explain <rule>` for rewrite recipes{}",
        p.dim,
        sorted.len(),
        p.reset
    );
    println!();
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match &cli.command {
        Some(Command::Explain { rule }) => return run_explain(rule),
        Some(Command::Rules { json }) => return run_rules(*json),
        None => {}
    }
    let result = match scan(&ScanOptions {
        root: cli.path.clone(),
        migrate: cli.migrate,
        scope: cli.scope.into(),
        base: cli.base.clone(),
    }) {
        Ok(result) => result,
        Err(message) => {
            eprintln!("effect-doctor: {message}");
            return ExitCode::from(2);
        }
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&result).expect("serializable report"));
        return ExitCode::SUCCESS;
    }

    render(&result, cli.verbose, cli.max_locations);
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
        "  {}effect doctor{}  {}v{}{}",
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

    let shown_groups = if verbose { ordered.len() } else { ordered.len().min(3) };
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
