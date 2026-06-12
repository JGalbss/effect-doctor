use oxc_ast::ast::{ImportDeclarationSpecifier, ModuleExportName, Program, Statement};
use std::collections::HashMap;

/// Which Effect bindings are in scope in a file, resolved from import
/// declarations so aliasing (`import { Effect as E } from "effect"`) and
/// module-path imports (`import * as Effect from "effect/Effect"`) both work.
#[derive(Default)]
pub struct EffectImports {
    /// local binding name -> canonical effect module name ("Effect", "Layer", "Schema", ...)
    locals: HashMap<String, String>,
    has_effect_import: bool,
}

impl EffectImports {
    pub fn from_program(program: &Program) -> Self {
        let mut imports = EffectImports::default();
        for statement in &program.body {
            let Statement::ImportDeclaration(decl) = statement else {
                continue;
            };
            let source = decl.source.value.as_str();
            if !is_effect_package(source) {
                continue;
            }
            imports.has_effect_import = true;
            let Some(specifiers) = &decl.specifiers else {
                continue;
            };
            for specifier in specifiers {
                match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(named) => {
                        if source == "effect" {
                            imports.locals.insert(
                                named.local.name.to_string(),
                                export_name(&named.imported).to_string(),
                            );
                        }
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(namespace) => {
                        if let Some(module) = effect_submodule(source) {
                            imports
                                .locals
                                .insert(namespace.local.name.to_string(), module.to_string());
                        }
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => {}
                }
            }
        }
        imports
    }

    pub fn has_any(&self) -> bool {
        self.has_effect_import
    }

    pub fn module_of(&self, local: &str) -> Option<&str> {
        self.locals.get(local).map(String::as_str)
    }

    pub fn is_module(&self, local: &str, module: &str) -> bool {
        self.module_of(local) == Some(module)
    }
}

fn is_effect_package(source: &str) -> bool {
    source == "effect" || source.starts_with("effect/") || source.starts_with("@effect/")
}

/// `effect/Effect` -> `Effect`, `effect/unstable/http/HttpClient` -> `HttpClient`
fn effect_submodule(source: &str) -> Option<&str> {
    let path = source.strip_prefix("effect/")?;
    Some(path.rsplit('/').next().unwrap_or(path))
}

fn export_name<'b>(name: &'b ModuleExportName) -> &'b str {
    match name {
        ModuleExportName::IdentifierName(identifier) => identifier.name.as_str(),
        ModuleExportName::IdentifierReference(reference) => reference.name.as_str(),
        ModuleExportName::StringLiteral(literal) => literal.value.as_str(),
    }
}
