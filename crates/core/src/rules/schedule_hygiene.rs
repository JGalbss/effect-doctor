use oxc_ast::ast::StaticMemberExpression;

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::member_module_prop;
use crate::rules::{FileCtx, Rule};

static ADD_JITTER: RuleMeta = RuleMeta {
    id: "add-jitter-to-backoff",
    severity: Severity::Info,
    category: Category::Performance,
    help: "Exponential backoff without jitter synchronizes retries across clients (thundering herd). Add `Schedule.jittered`.",
};

static CAP_BACKOFF: RuleMeta = RuleMeta {
    id: "cap-exponential-backoff",
    severity: Severity::Info,
    category: Category::Performance,
    help: "Uncapped exponential backoff grows without bound. Union with a spaced schedule (`Schedule.either(Schedule.spaced(...))`) to cap the delay.",
};

fn is_cap_combinator(prop: &str) -> bool {
    matches!(prop, "either" | "union" | "upTo" | "intersect" | "both")
}

pub struct ScheduleHygiene;

impl Rule for ScheduleHygiene {
    fn on_member(&self, member: &StaticMemberExpression<'_>, ctx: &mut FileCtx) {
        let Some(("Schedule", prop)) = member_module_prop(member, &ctx.imports) else {
            return;
        };
        if prop == "exponential" {
            ctx.scratch.exponential_spans.push(member.span);
            return;
        }
        if prop == "jittered" {
            ctx.scratch.has_schedule_jitter = true;
            return;
        }
        if is_cap_combinator(prop) {
            ctx.scratch.has_schedule_cap = true;
        }
    }

    fn on_file_end(&self, ctx: &mut FileCtx) {
        let spans = std::mem::take(&mut ctx.scratch.exponential_spans);
        for span in spans {
            if !ctx.scratch.has_schedule_jitter {
                ctx.report(
                    &ADD_JITTER,
                    span,
                    "Schedule.exponential without Schedule.jittered in this file".to_string(),
                );
            }
            if !ctx.scratch.has_schedule_cap {
                ctx.report(
                    &CAP_BACKOFF,
                    span,
                    "Schedule.exponential without a delay cap in this file".to_string(),
                );
            }
        }
    }
}
