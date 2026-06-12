use oxc_ast::ast::TaggedTemplateExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::ident_name;
use crate::rules::{FileCtx, Rule};

static META: RuleMeta = RuleMeta {
    id: "no-manual-sql-transactions",
    severity: Severity::Warn,
    category: Category::Architecture,
    help: "Hand-written BEGIN/COMMIT/ROLLBACK loses automatic rollback on failure and interruption. Use sql.withTransaction.",
};

fn is_transaction_keyword(text: &str) -> bool {
    let upper = text.trim().to_ascii_uppercase();
    upper.starts_with("BEGIN") || upper.starts_with("COMMIT") || upper.starts_with("ROLLBACK")
}

pub struct NoManualSqlTransactions;

impl Rule for NoManualSqlTransactions {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&META];
        METAS
    }

    fn on_tagged_template(&self, template: &TaggedTemplateExpression<'_>, ctx: &mut FileCtx) {
        if ident_name(&template.tag) != Some("sql") {
            return;
        }
        let Some(first_quasi) = template.quasi.quasis.first() else {
            return;
        };
        if !is_transaction_keyword(&first_quasi.value.raw) {
            return;
        }
        ctx.report(
            &META,
            template.span,
            "manual transaction statement — use sql.withTransaction".to_string(),
        );
    }
}
