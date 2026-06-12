use oxc_ast::ast::{
    Class, ClassElement, Expression, MethodDefinitionKind, TSType, TSTypeName,
};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::call_module_prop;
use crate::rules::{FileCtx, Rule};

static SELF_MISMATCH: RuleMeta = RuleMeta {
    id: "schema-class-self-mismatch",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "The Self type parameter of Schema.Class/TaggedClass/TaggedError must be the declaring class itself, or every static helper is typed against the wrong class.",
};

static CONSTRUCTOR_OVERRIDE: RuleMeta = RuleMeta {
    id: "no-constructor-override-in-schema-class",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "Schema classes control construction for decoding — overriding the constructor breaks decode. Use a static make/factory or field transformations instead.",
};

fn is_schema_class_factory(prop: &str) -> bool {
    matches!(
        prop,
        "Class" | "TaggedClass" | "TaggedError" | "TaggedErrorClass" | "TaggedRequest"
    )
}

/// For `class X extends Schema.Class<Self>("X")({...})`, find the inner
/// `Schema.<factory><Self>(...)` call and return the Self type name.
fn schema_self_type_name<'a, 'b>(
    superclass: &'b Expression<'a>,
    ctx: &FileCtx,
) -> Option<(&'b str, oxc_span::Span)> {
    let mut current = superclass;
    loop {
        let Expression::CallExpression(call) = current else {
            return None;
        };
        if let Some(("Schema", prop)) = call_module_prop(call, &ctx.imports) {
            if !is_schema_class_factory(prop) {
                return None;
            }
            let type_arguments = call.type_arguments.as_ref()?;
            let TSType::TSTypeReference(reference) = type_arguments.params.first()? else {
                return None;
            };
            let TSTypeName::IdentifierReference(identifier) = &reference.type_name else {
                return None;
            };
            return Some((identifier.name.as_str(), reference.span));
        }
        current = &call.callee;
    }
}

fn extends_schema_class(superclass: &Expression, ctx: &FileCtx) -> bool {
    let mut current = superclass;
    loop {
        let Expression::CallExpression(call) = current else {
            return false;
        };
        if let Some(("Schema", prop)) = call_module_prop(call, &ctx.imports) {
            return is_schema_class_factory(prop);
        }
        current = &call.callee;
    }
}

pub struct SchemaClassHygiene;

impl Rule for SchemaClassHygiene {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&SELF_MISMATCH, &CONSTRUCTOR_OVERRIDE];
        METAS
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        let Some(superclass) = &class.super_class else {
            return;
        };
        let Some(class_name) = class.id.as_ref().map(|id| id.name.as_str()) else {
            return;
        };
        if let Some((self_name, span)) = schema_self_type_name(superclass, ctx) {
            if self_name != class_name {
                ctx.report(
                    &SELF_MISMATCH,
                    span,
                    format!(
                        "Self type parameter is `{self_name}` but the declaring class is `{class_name}`"
                    ),
                );
            }
        }
        if !extends_schema_class(superclass, ctx) {
            return;
        }
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            if method.kind != MethodDefinitionKind::Constructor {
                continue;
            }
            ctx.report(
                &CONSTRUCTOR_OVERRIDE,
                method.span,
                format!("constructor override in Schema class `{class_name}` breaks decoding"),
            );
        }
    }
}
