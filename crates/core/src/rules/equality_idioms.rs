use oxc_ast::ast::{BinaryExpression, CallExpression, Expression, SwitchStatement};
use oxc_syntax::operator::BinaryOperator;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{static_member, unwrap_parens};
use crate::rules::{FileCtx, Rule};

static OBJECT_LITERAL_COMPARISON: RuleMeta = RuleMeta {
    id: "no-object-literal-comparison",
    severity: Severity::Warn,
    category: Category::Correctness,
    help: "Comparing against a fresh object/array literal uses reference equality and is always false. Use Equal.equals with Data.struct/Data.array for structural equality.",
};

static TAG_STRING_COMPARISON: RuleMeta = RuleMeta {
    id: "no-tag-string-comparison",
    severity: Severity::Warn,
    category: Category::Idiomatic,
    help: "Reaching into `_tag` for built-in types bypasses the typed predicates. Use Option.isSome/isNone, Either.isLeft/isRight, Exit.isSuccess/isFailure.",
};

static TAG_SWITCH: RuleMeta = RuleMeta {
    id: "prefer-match-over-tag-switch",
    severity: Severity::Info,
    category: Category::Idiomatic,
    help: "switch on `_tag` is non-exhaustive by default. Match.valueTags / Effect.catchTags give exhaustive, declarative dispatch.",
};

fn is_equality(operator: BinaryOperator) -> bool {
    matches!(
        operator,
        BinaryOperator::Equality
            | BinaryOperator::Inequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::StrictInequality
    )
}

fn is_literal_compound(expr: &Expression) -> bool {
    matches!(
        unwrap_parens(expr),
        Expression::ObjectExpression(_) | Expression::ArrayExpression(_)
    )
}

fn is_tag_member(expr: &Expression) -> bool {
    matches!(
        unwrap_parens(expr),
        Expression::StaticMemberExpression(member) if member.property.name == "_tag"
    )
}

fn builtin_tag_predicate(tag: &str) -> Option<&'static str> {
    match tag {
        "Some" => Some("Option.isSome"),
        "None" => Some("Option.isNone"),
        "Left" => Some("Either.isLeft"),
        "Right" => Some("Either.isRight"),
        "Success" => Some("Exit.isSuccess"),
        "Failure" => Some("Exit.isFailure"),
        _ => None,
    }
}

fn string_value<'a>(expr: &'a Expression) -> Option<&'a str> {
    match unwrap_parens(expr) {
        Expression::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

pub struct EqualityIdioms;

impl Rule for EqualityIdioms {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&OBJECT_LITERAL_COMPARISON, &TAG_STRING_COMPARISON, &TAG_SWITCH];
        METAS
    }

    fn on_binary(&self, binary: &BinaryExpression<'_>, ctx: &mut FileCtx) {
        if !is_equality(binary.operator) {
            return;
        }
        if is_literal_compound(&binary.left) || is_literal_compound(&binary.right) {
            ctx.report(
                &OBJECT_LITERAL_COMPARISON,
                binary.span,
                "comparison against a fresh object/array literal is always false — use Equal.equals".to_string(),
            );
            return;
        }
        let tag_side_string = if is_tag_member(&binary.left) {
            string_value(&binary.right)
        } else if is_tag_member(&binary.right) {
            string_value(&binary.left)
        } else {
            None
        };
        let Some(tag) = tag_side_string else {
            return;
        };
        let Some(predicate) = builtin_tag_predicate(tag) else {
            return;
        };
        ctx.report(
            &TAG_STRING_COMPARISON,
            binary.span,
            format!("_tag === \"{tag}\" — use {predicate}"),
        );
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((_, prop)) = static_member(&call.callee) else {
            return;
        };
        if prop != "includes" && prop != "indexOf" {
            return;
        }
        let has_literal_arg = call
            .arguments
            .first()
            .and_then(|argument| argument.as_expression())
            .is_some_and(is_literal_compound);
        if !has_literal_arg {
            return;
        }
        ctx.report(
            &OBJECT_LITERAL_COMPARISON,
            call.span,
            format!(".{prop}(objectLiteral) uses reference equality and never matches — use Equal.equals"),
        );
    }

    fn on_switch(&self, switch_stmt: &SwitchStatement<'_>, ctx: &mut FileCtx) {
        if !is_tag_member(&switch_stmt.discriminant) {
            return;
        }
        ctx.report(
            &TAG_SWITCH,
            oxc_span::Span::new(switch_stmt.span.start, switch_stmt.span.start + 6),
            "switch on _tag — consider Match.valueTags / Effect.catchTags".to_string(),
        );
    }
}
