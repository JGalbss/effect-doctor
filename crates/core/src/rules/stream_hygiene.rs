use oxc_ast::ast::{Argument, CallExpression, Expression, ObjectPropertyKind, PropertyKey};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::matchers::{call_module_prop, member_module_prop, static_member};
use crate::rules::{FileCtx, Rule};

static INFINITE_RUNCOLLECT: RuleMeta = RuleMeta {
    id: "no-runcollect-on-infinite-stream",
    severity: Severity::Error,
    category: Category::Correctness,
    help: "Stream.runCollect buffers the whole stream — on an infinite source it runs until the process dies. Add Stream.take/takeUntil/timeout, or use Stream.runDrain.",
};

static EAGER_CHUNK: RuleMeta = RuleMeta {
    id: "no-eager-chunk-stream",
    severity: Severity::Info,
    category: Category::Performance,
    help: "Stream.fromChunk(Chunk.fromIterable(x)) materializes the entire iterable up front. Stream.fromIterable(x) streams it lazily.",
};

static MAPEFFECT_CONCURRENCY: RuleMeta = RuleMeta {
    id: "stream-mapeffect-missing-concurrency",
    severity: Severity::Info,
    category: Category::Performance,
    help: "Stream.mapEffect runs items one at a time unless you pass { concurrency }. For I/O per element that's usually a large slowdown — make the choice explicit.",
};

static QUEUE_BOUNDED: RuleMeta = RuleMeta {
    id: "prefer-queue-bounded",
    severity: Severity::Info,
    category: Category::Performance,
    help: "Unbounded queues have no backpressure — a fast producer grows the queue until OOM. Prefer Queue.bounded (or sliding/dropping) sized to the workload.",
};

fn is_infinite_source(call: &CallExpression<'_>, ctx: &FileCtx) -> bool {
    let Some(("Stream", prop)) = call_module_prop(call, &ctx.imports) else {
        return false;
    };
    if matches!(
        prop,
        "tick" | "forever" | "repeatValue" | "repeatEffect" | "repeatEffectOption" | "fromSchedule"
            | "iterate"
    ) {
        return true;
    }
    if prop != "range" {
        return false;
    }
    matches!(
        call.arguments.get(1).and_then(Argument::as_expression),
        Some(Expression::Identifier(identifier)) if identifier.name == "Infinity"
    )
}

fn pipe_arg_stream_prop<'a, 'b>(
    expr: &'b Expression<'a>,
    ctx: &'b FileCtx,
) -> Option<&'b str> {
    let (module, prop) = match expr {
        Expression::StaticMemberExpression(member) => member_module_prop(member, &ctx.imports)?,
        Expression::CallExpression(call) => call_module_prop(call, &ctx.imports)?,
        _ => return None,
    };
    if module != "Stream" {
        return None;
    }
    Some(prop)
}

fn is_bounding_op(prop: &str) -> bool {
    matches!(
        prop,
        "take" | "takeUntil" | "takeUntilEffect" | "takeWhile" | "timeout" | "timeoutTo"
            | "interruptAfter" | "interruptWhen" | "runDrain"
    )
}

fn options_have_concurrency(call: &CallExpression<'_>) -> bool {
    call.arguments
        .iter()
        .filter_map(Argument::as_expression)
        .any(|expr| {
            let Expression::ObjectExpression(object) = expr else {
                return false;
            };
            object.properties.iter().any(|property| {
                let ObjectPropertyKind::ObjectProperty(entry) = property else {
                    return false;
                };
                matches!(&entry.key, PropertyKey::StaticIdentifier(key) if key.name == "concurrency")
            })
        })
}

pub struct StreamHygiene;

impl StreamHygiene {
    fn check_infinite_collect(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        // `<infinite source>.pipe(..., Stream.runCollect)` with no bounding op.
        let Some((object, "pipe")) = static_member(&call.callee) else {
            return;
        };
        let Expression::CallExpression(root) = object else {
            return;
        };
        if !is_infinite_source(root, ctx) {
            return;
        }
        let props: Vec<&str> = call
            .arguments
            .iter()
            .filter_map(Argument::as_expression)
            .filter_map(|expr| pipe_arg_stream_prop(expr, ctx))
            .collect();
        if !props.iter().any(|prop| *prop == "runCollect") {
            return;
        }
        if props.iter().any(|prop| is_bounding_op(prop)) {
            return;
        }
        ctx.report(
            &INFINITE_RUNCOLLECT,
            call.span,
            "Stream.runCollect on an infinite stream — this buffers forever; add Stream.take or use Stream.runDrain".to_string(),
        );
    }

    fn check_eager_chunk(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if call_module_prop(call, &ctx.imports) != Some(("Stream", "fromChunk")) {
            return;
        }
        let Some(Expression::CallExpression(inner)) =
            call.arguments.first().and_then(Argument::as_expression)
        else {
            return;
        };
        if call_module_prop(inner, &ctx.imports) != Some(("Chunk", "fromIterable")) {
            return;
        }
        ctx.report(
            &EAGER_CHUNK,
            call.span,
            "Stream.fromChunk(Chunk.fromIterable(x)) — use Stream.fromIterable(x)".to_string(),
        );
    }

    fn check_mapeffect_concurrency(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        if call_module_prop(call, &ctx.imports) != Some(("Stream", "mapEffect")) {
            return;
        }
        if options_have_concurrency(call) {
            return;
        }
        ctx.report(
            &MAPEFFECT_CONCURRENCY,
            call.span,
            "Stream.mapEffect without { concurrency } runs sequentially — make it explicit".to_string(),
        );
    }

    fn check_unbounded_queue(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        let Some((module, "unbounded")) = call_module_prop(call, &ctx.imports) else {
            return;
        };
        if module != "Queue" && module != "PubSub" {
            return;
        }
        ctx.report(
            &QUEUE_BOUNDED,
            call.span,
            format!("{module}.unbounded has no backpressure — prefer {module}.bounded"),
        );
    }
}

impl Rule for StreamHygiene {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&INFINITE_RUNCOLLECT, &EAGER_CHUNK, &MAPEFFECT_CONCURRENCY, &QUEUE_BOUNDED];
        METAS
    }

    fn on_call(&self, call: &CallExpression<'_>, ctx: &mut FileCtx) {
        self.check_infinite_collect(call, ctx);
        self.check_eager_chunk(call, ctx);
        self.check_mapeffect_concurrency(call, ctx);
        self.check_unbounded_queue(call, ctx);
    }
}
