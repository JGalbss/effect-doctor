//! Repo-wide symbol graph: per-file top-level definitions and the import edges
//! between files. This is the kernel's "ground truth" about what exists and what
//! depends on what — the substrate the impact (test-selection), policy
//! (layering), and orchestration (footprint) layers all read from.
//!
//! It is deliberately *not* Effect-specific: every top-level definition is
//! indexed regardless of imports, unlike the lint pipeline which short-circuits
//! on files that don't use Effect.

use std::collections::BTreeMap;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Class, Declaration, Function, ImportDeclarationSpecifier, Statement,
    VariableDeclaration,
};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::content_addr::ContentHash;
use crate::text::LineIndex;

/// What kind of binding a definition introduces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Const,
    Variable,
}

/// A top-level definition in a file.
#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub name: String,
    pub kind: SymbolKind,
    pub exported: bool,
    pub line: u32,
    pub column: u32,
}

/// A raw `import ... from "<specifier>"` as written, before resolution.
#[derive(Debug, Clone)]
pub struct ImportEdge {
    /// The module specifier exactly as written (e.g. `"./util"`, `"effect"`).
    pub specifier: String,
    /// Imported binding names (`import { a, b }` → `["a", "b"]`); empty for a
    /// bare/side-effect import or a namespace import.
    pub names: Vec<String>,
}

/// Everything the graph knows about one file.
#[derive(Debug, Clone)]
pub struct FileSymbols {
    pub path: String,
    pub content_hash: ContentHash,
    pub defs: Vec<SymbolDef>,
    pub imports: Vec<ImportEdge>,
}

/// An import edge resolved to the file it points at within the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEdge {
    pub from: String,
    pub to: String,
    pub names: Vec<String>,
}

/// Module-specifier extensions tried during relative-import resolution, in
/// priority order.
const RESOLVE_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"];

/// A repo-wide index of files, their definitions, and their imports.
#[derive(Default)]
pub struct SymbolGraph {
    files: BTreeMap<String, FileSymbols>,
}

impl SymbolGraph {
    pub fn new() -> SymbolGraph {
        SymbolGraph::default()
    }

    /// Parse `source` and record the file's definitions and imports. Replaces
    /// any prior entry for the same path (incremental update).
    pub fn add_file(&mut self, path: &str, source: &str) {
        self.insert(SymbolGraph::analyze(path, source));
    }

    /// Parse one file into [`FileSymbols`] without inserting it — lets callers
    /// (e.g. the [`crate::index::Index`] builder) parse files in parallel and
    /// insert the results afterward.
    pub fn analyze(path: &str, source: &str) -> FileSymbols {
        analyze_file(path, source)
    }

    /// Insert pre-parsed file symbols, replacing any prior entry for its path.
    pub fn insert(&mut self, symbols: FileSymbols) {
        self.files.insert(symbols.path.clone(), symbols);
    }

    /// Drop a file from the graph (e.g. on delete).
    pub fn remove_file(&mut self, path: &str) {
        self.files.remove(path);
    }

    pub fn file(&self, path: &str) -> Option<&FileSymbols> {
        self.files.get(path)
    }

    pub fn files(&self) -> impl Iterator<Item = &FileSymbols> {
        self.files.values()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Resolve a relative module specifier from `importer` to a file present in
    /// the graph. Bare specifiers (npm packages like `"effect"`) return `None`.
    pub fn resolve_import(&self, importer: &str, specifier: &str) -> Option<&str> {
        if !specifier.starts_with('.') {
            return None;
        }
        let base = parent_dir(importer);
        let joined = normalize_join(base, specifier);
        candidate_paths(&joined)
            .into_iter()
            .find(|candidate| self.files.contains_key(candidate))
            .and_then(|candidate| self.files.get_key_value(&candidate).map(|(key, _)| key.as_str()))
    }

    /// All import edges that resolve to a file within the graph.
    pub fn import_edges(&self) -> Vec<ResolvedEdge> {
        let mut edges = Vec::new();
        for file in self.files.values() {
            for import in &file.imports {
                if let Some(target) = self.resolve_import(&file.path, &import.specifier) {
                    edges.push(ResolvedEdge {
                        from: file.path.clone(),
                        to: target.to_string(),
                        names: import.names.clone(),
                    });
                }
            }
        }
        edges
    }

    /// Files that define a symbol with the given name, with the definition.
    pub fn definitions_named<'a>(&'a self, name: &str) -> Vec<(&'a str, &'a SymbolDef)> {
        let mut found = Vec::new();
        for file in self.files.values() {
            for def in &file.defs {
                if def.name == name {
                    found.push((file.path.as_str(), def));
                }
            }
        }
        found
    }
}

/// Parse one file and extract its definitions + imports.
fn analyze_file(path: &str, source: &str) -> FileSymbols {
    let mut symbols = FileSymbols {
        path: path.to_string(),
        content_hash: ContentHash::of(source),
        defs: Vec::new(),
        imports: Vec::new(),
    };
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::ts());
    let parsed = Parser::new(&allocator, source, source_type).parse();
    if parsed.panicked {
        return symbols;
    }
    let lines = LineIndex::new(source);
    for statement in &parsed.program.body {
        collect_statement(statement, &mut symbols, &lines);
    }
    symbols
}

/// A top-level statement → definitions and/or an import edge.
fn collect_statement(statement: &Statement, out: &mut FileSymbols, lines: &LineIndex) {
    match statement {
        Statement::ImportDeclaration(decl) => {
            out.imports.push(ImportEdge {
                specifier: decl.source.value.to_string(),
                names: import_names(decl),
            });
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(declaration) = &export.declaration {
                collect_declaration(declaration, true, out, lines);
            }
            if let Some(source) = &export.source {
                // `export { x } from "./y"` is also a dependency edge.
                out.imports.push(ImportEdge {
                    specifier: source.value.to_string(),
                    names: Vec::new(),
                });
            }
        }
        Statement::ExportAllDeclaration(export) => {
            out.imports.push(ImportEdge {
                specifier: export.source.value.to_string(),
                names: Vec::new(),
            });
        }
        Statement::FunctionDeclaration(func) => push_function(func, false, out, lines),
        Statement::ClassDeclaration(class) => push_class(class, false, out, lines),
        Statement::VariableDeclaration(decl) => push_variables(decl, false, out, lines),
        _ => {}
    }
}

/// An exported `Declaration` (the `declaration` of an `export` statement).
fn collect_declaration(
    declaration: &Declaration,
    exported: bool,
    out: &mut FileSymbols,
    lines: &LineIndex,
) {
    match declaration {
        Declaration::FunctionDeclaration(func) => push_function(func, exported, out, lines),
        Declaration::ClassDeclaration(class) => push_class(class, exported, out, lines),
        Declaration::VariableDeclaration(decl) => push_variables(decl, exported, out, lines),
        _ => {}
    }
}

fn push_function(func: &Function, exported: bool, out: &mut FileSymbols, lines: &LineIndex) {
    if let Some(id) = func.id.as_ref() {
        let (line, column) = lines.line_col(func.span.start as usize);
        out.defs.push(SymbolDef {
            name: id.name.to_string(),
            kind: SymbolKind::Function,
            exported,
            line,
            column,
        });
    }
}

fn push_class(class: &Class, exported: bool, out: &mut FileSymbols, lines: &LineIndex) {
    if let Some(id) = class.id.as_ref() {
        let (line, column) = lines.line_col(class.span.start as usize);
        out.defs.push(SymbolDef {
            name: id.name.to_string(),
            kind: SymbolKind::Class,
            exported,
            line,
            column,
        });
    }
}

fn push_variables(
    decl: &VariableDeclaration,
    exported: bool,
    out: &mut FileSymbols,
    lines: &LineIndex,
) {
    let kind = match decl.kind.is_const() {
        true => SymbolKind::Const,
        false => SymbolKind::Variable,
    };
    for declarator in &decl.declarations {
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            continue;
        };
        let (line, column) = lines.line_col(declarator.span.start as usize);
        out.defs.push(SymbolDef {
            name: id.name.to_string(),
            kind,
            exported,
            line,
            column,
        });
    }
}

fn import_names(decl: &oxc_ast::ast::ImportDeclaration) -> Vec<String> {
    let Some(specifiers) = &decl.specifiers else {
        return Vec::new();
    };
    specifiers
        .iter()
        .filter_map(|specifier| match specifier {
            ImportDeclarationSpecifier::ImportSpecifier(named) => Some(named.local.name.to_string()),
            ImportDeclarationSpecifier::ImportDefaultSpecifier(default) => {
                Some(default.local.name.to_string())
            }
            ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => None,
        })
        .collect()
}

/// Directory portion of a forward-slash path (`"src/a/b.ts"` → `"src/a"`).
fn parent_dir(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

/// Join a base directory with a relative specifier and normalise `.`/`..`.
fn normalize_join(base: &str, specifier: &str) -> String {
    let mut segments: Vec<&str> = if base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// Candidate file paths for a resolved, extensionless module path: the path with
/// each known extension, then `<path>/index.<ext>`.
fn candidate_paths(joined: &str) -> Vec<String> {
    let mut candidates = vec![joined.to_string()];
    for extension in RESOLVE_EXTENSIONS {
        candidates.push(format!("{joined}{extension}"));
    }
    for extension in RESOLVE_EXTENSIONS {
        candidates.push(format!("{joined}/index{extension}"));
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_top_level_defs_with_export_flags() {
        let mut graph = SymbolGraph::new();
        graph.add_file(
            "src/a.ts",
            "export function foo() { return 1 }\nconst bar = 2\nexport const baz = 3\nclass Q {}",
        );
        let file = graph.file("src/a.ts").expect("file recorded");
        let foo = file.defs.iter().find(|d| d.name == "foo").unwrap();
        assert_eq!(foo.kind, SymbolKind::Function);
        assert!(foo.exported);
        let bar = file.defs.iter().find(|d| d.name == "bar").unwrap();
        assert_eq!(bar.kind, SymbolKind::Const);
        assert!(!bar.exported);
        assert!(file.defs.iter().find(|d| d.name == "baz").unwrap().exported);
        assert_eq!(
            file.defs.iter().find(|d| d.name == "Q").unwrap().kind,
            SymbolKind::Class
        );
    }

    #[test]
    fn resolves_cross_file_import_edge() {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/a.ts", "export function foo() { return 1 }");
        graph.add_file("src/b.ts", "import { foo } from './a'\nfoo()");
        let edges = graph.import_edges();
        assert_eq!(
            edges,
            vec![ResolvedEdge {
                from: "src/b.ts".to_string(),
                to: "src/a.ts".to_string(),
                names: vec!["foo".to_string()],
            }]
        );
    }

    #[test]
    fn resolves_parent_and_index_imports() {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/util/index.ts", "export const x = 1");
        graph.add_file("src/feature/b.ts", "import { x } from '../util'");
        let edges = graph.import_edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to, "src/util/index.ts");
    }

    #[test]
    fn bare_specifiers_are_not_edges() {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/a.ts", "import { Effect } from 'effect'");
        assert!(graph.import_edges().is_empty());
    }

    #[test]
    fn incremental_update_replaces_file() {
        let mut graph = SymbolGraph::new();
        graph.add_file("src/a.ts", "export const x = 1");
        let first = graph.file("src/a.ts").unwrap().content_hash;
        graph.add_file("src/a.ts", "export const x = 1\nexport const y = 2");
        let file = graph.file("src/a.ts").unwrap();
        assert_ne!(first, file.content_hash);
        assert_eq!(file.defs.len(), 2);
    }
}
