use oxc_span::Span;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileContext {
    Production,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Correctness,
    Idiomatic,
    Architecture,
    Performance,
    V4Migration,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Correctness => "Correctness",
            Category::Idiomatic => "Idiomatic",
            Category::Architecture => "Architecture",
            Category::Performance => "Performance",
            Category::V4Migration => "v4 Migration",
        }
    }
}

/// Static metadata for a rule, shared by every diagnostic it emits.
#[derive(Debug)]
pub struct RuleMeta {
    pub id: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub help: &'static str,
}

/// Span-based diagnostic emitted while a file's AST is in memory.
/// Converted to a [`Diagnostic`] (line/col + snippet) by the engine.
pub struct RawDiagnostic {
    pub meta: &'static RuleMeta,
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub help: &'static str,
    pub file: String,
    pub file_context: FileContext,
    pub line: u32,
    pub column: u32,
    pub snippet: String,
}
