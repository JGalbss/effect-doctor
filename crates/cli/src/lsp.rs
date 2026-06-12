//! `effect-doctor lsp` — a stdio language server publishing the syntactic
//! rule set as editor diagnostics on open/change. Diagnostic messages carry
//! the rule's help text; the code field is the rule id (`effect-doctor explain
//! <id>` for the rewrite recipe).

use std::error::Error;
use std::path::PathBuf;

use effect_doctor_core::{detect_effect_major, lint_source, Severity};
use lsp_server::{Connection, Message, Notification as ServerNotification};
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, Notification, PublishDiagnostics,
};
use lsp_types::{
    Diagnostic, DiagnosticSeverity, InitializeParams, NumberOrString, Position,
    PublishDiagnosticsParams, Range, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
};

type LspResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn map_severity(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warn => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    }
}

fn to_lsp_diagnostics(path: &str, text: &str, v4_active: bool) -> Vec<Diagnostic> {
    lint_source(path, text, v4_active)
        .into_iter()
        .map(|finding| {
            let line = finding.line.saturating_sub(1);
            let start_column = finding.column.saturating_sub(1);
            let end_column = (finding.snippet.len() as u32).max(start_column + 1);
            Diagnostic {
                range: Range {
                    start: Position::new(line, start_column),
                    end: Position::new(line, end_column),
                },
                severity: Some(map_severity(finding.severity)),
                code: Some(NumberOrString::String(finding.rule.to_string())),
                source: Some("effect-doctor".to_string()),
                message: format!("{}\n\n{}", finding.message, finding.help),
                ..Diagnostic::default()
            }
        })
        .collect()
}

fn publish(connection: &Connection, uri: Url, diagnostics: Vec<Diagnostic>) -> LspResult<()> {
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    connection
        .sender
        .send(Message::Notification(ServerNotification::new(
            PublishDiagnostics::METHOD.to_string(),
            params,
        )))?;
    Ok(())
}

fn lint_and_publish(
    connection: &Connection,
    uri: Url,
    text: &str,
    v4_active: bool,
) -> LspResult<()> {
    let path = uri
        .to_file_path()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| uri.to_string());
    let diagnostics = to_lsp_diagnostics(&path, text, v4_active);
    publish(connection, uri, diagnostics)
}

pub fn run() -> LspResult<()> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..ServerCapabilities::default()
    };
    let initialize_params = connection.initialize(serde_json::to_value(&capabilities)?)?;
    let initialize: InitializeParams = serde_json::from_value(initialize_params)?;
    let root: PathBuf = initialize
        .root_uri
        .and_then(|uri| uri.to_file_path().ok())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let v4_active = detect_effect_major(&root) == Some(4);

    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
            }
            Message::Notification(notification) => match notification.method.as_str() {
                DidOpenTextDocument::METHOD => {
                    let params: lsp_types::DidOpenTextDocumentParams =
                        serde_json::from_value(notification.params)?;
                    lint_and_publish(
                        &connection,
                        params.text_document.uri,
                        &params.text_document.text,
                        v4_active,
                    )?;
                }
                DidChangeTextDocument::METHOD => {
                    let params: lsp_types::DidChangeTextDocumentParams =
                        serde_json::from_value(notification.params)?;
                    // Full sync: the last change carries the whole document.
                    let Some(change) = params.content_changes.into_iter().last() else {
                        continue;
                    };
                    lint_and_publish(&connection, params.text_document.uri, &change.text, v4_active)?;
                }
                _ => {}
            },
            Message::Response(_) => {}
        }
    }
    io_threads.join()?;
    Ok(())
}
