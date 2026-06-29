//! OOP → Effect: hand-rolled Gang-of-Four design patterns that Effect (or plain
//! functional TypeScript) replaces with a first-class primitive. Each rule
//! matches a distinctive structural signature with low false-positive risk and
//! points at the idiomatic rewrite. All are opt-in under `--agent` and escalate
//! to `error` under `--agent-strict`.

use oxc_ast::ast::{Class, ClassElement, MethodDefinitionKind, TSAccessibility, TSInterfaceDeclaration, TSSignature, TSTypeName};

use crate::diagnostics::{Category, RuleMeta, Severity};
use crate::rules::{FileCtx, Rule};

static SINGLETON: RuleMeta = RuleMeta {
    id: "oop-singleton-to-layer",
    severity: Severity::Warn,
    category: Category::OopToEffect,
    help: "A hand-rolled Singleton (private static instance + getInstance) is global mutable state with no lifecycle. Model it as a service: Context.Tag (or Effect.Service) + a Layer, so construction, dependencies, and teardown are explicit and testable.",
};

static OBSERVER: RuleMeta = RuleMeta {
    id: "oop-observer-to-pubsub",
    severity: Severity::Warn,
    category: Category::OopToEffect,
    help: "A hand-rolled Observer (listener array + subscribe/notify) leaks subscriptions and has no backpressure. Use PubSub for broadcast, Stream for a consumable feed, or SubscriptionRef for observable state.",
};

static STRATEGY: RuleMeta = RuleMeta {
    id: "oop-strategy-to-function",
    severity: Severity::Warn,
    category: Category::OopToEffect,
    help: "A single-method interface with several class implementations is the Strategy pattern — in TypeScript that's just a function type. Replace the interface + classes with a function (or `Effect`) value passed where the strategy is needed.",
};

static VISITOR: RuleMeta = RuleMeta {
    id: "oop-visitor-to-match",
    severity: Severity::Warn,
    category: Category::OopToEffect,
    help: "The Visitor pattern (visitX double-dispatch methods) works around the lack of pattern matching. Model the data as a tagged union (Data.TaggedClass / a discriminated union) and branch with Match.exhaustive.",
};

static CHAIN: RuleMeta = RuleMeta {
    id: "oop-chain-to-catchtag",
    severity: Severity::Warn,
    category: Category::OopToEffect,
    help: "A Chain-of-Responsibility handler (a `next` link + handle/setNext) is sequential fallback. Compose it directly: Effect.orElse / Effect.catchTag chains for error fallback, or a list of handlers folded with Effect.firstSuccessOf / a middleware pipeline.",
};

/// The names a class declares, split by member kind. Recomputed per rule per
/// class — classes are few, so the duplication is cheaper than shared state.
struct Members {
    static_fields: Vec<String>,
    fields: Vec<String>,
    methods: Vec<String>,
    static_methods: Vec<String>,
    has_private_constructor: bool,
}

fn member_name(key: &oxc_ast::ast::PropertyKey<'_>) -> Option<String> {
    key.static_name().map(|name| name.into_owned())
}

fn class_members(class: &Class<'_>) -> Members {
    let mut members = Members {
        static_fields: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        static_methods: Vec::new(),
        has_private_constructor: false,
    };
    for element in &class.body.body {
        match element {
            ClassElement::MethodDefinition(method) => {
                if method.kind == MethodDefinitionKind::Constructor {
                    if method.accessibility == Some(TSAccessibility::Private) {
                        members.has_private_constructor = true;
                    }
                    continue;
                }
                let Some(name) = member_name(&method.key) else {
                    continue;
                };
                if method.r#static {
                    members.static_methods.push(name.clone());
                }
                members.methods.push(name);
            }
            ClassElement::PropertyDefinition(property) => {
                let Some(name) = member_name(&property.key) else {
                    continue;
                };
                if property.r#static {
                    members.static_fields.push(name.clone());
                }
                members.fields.push(name);
            }
            _ => {}
        }
    }
    members
}

fn class_name<'a>(class: &'a Class<'a>) -> &'a str {
    class
        .id
        .as_ref()
        .map(|id| id.name.as_str())
        .unwrap_or("anonymous")
}

fn any(names: &[String], matches: impl Fn(&str) -> bool) -> bool {
    names.iter().any(|name| matches(name.as_str()))
}

fn is_singleton_field(name: &str) -> bool {
    matches!(name, "instance" | "_instance" | "instance_" | "_self")
}

fn is_get_instance(name: &str) -> bool {
    matches!(name, "getInstance" | "instance" | "get_instance" | "getDefault")
}

fn is_listeners_field(name: &str) -> bool {
    matches!(
        name,
        "listeners"
            | "_listeners"
            | "observers"
            | "_observers"
            | "subscribers"
            | "_subscribers"
            | "subscriptions"
            | "handlers"
            | "callbacks"
    )
}

fn is_subscribe_method(name: &str) -> bool {
    matches!(
        name,
        "subscribe" | "addListener" | "addObserver" | "attach" | "addEventListener" | "register"
    )
}

fn is_notify_method(name: &str) -> bool {
    matches!(
        name,
        "notify" | "notifyAll" | "emit" | "publish" | "fire" | "broadcast" | "dispatch"
    )
}

fn is_chain_next_field(name: &str) -> bool {
    matches!(
        name,
        "next" | "_next" | "nextHandler" | "_nextHandler" | "successor" | "nextMiddleware"
    )
}

fn is_handle_method(name: &str) -> bool {
    matches!(name, "handle" | "handleRequest")
}

fn is_set_next_method(name: &str) -> bool {
    matches!(name, "setNext" | "setSuccessor" | "setNextHandler")
}

fn is_visit_method(name: &str) -> bool {
    name.len() > "visit".len() && name.starts_with("visit")
}

pub struct Singleton;

impl Rule for Singleton {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&SINGLETON];
        METAS
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let members = class_members(class);
        let has_instance_field = any(&members.static_fields, is_singleton_field);
        let has_get_instance = any(&members.static_methods, is_get_instance);
        if has_instance_field && has_get_instance {
            ctx.report_agent(
                &SINGLETON,
                class.span,
                format!(
                    "class {} is a hand-rolled Singleton — model it as a Context.Tag/Layer service",
                    class_name(class)
                ),
            );
        }
    }
}

pub struct Observer;

impl Rule for Observer {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&OBSERVER];
        METAS
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let members = class_members(class);
        let has_listeners = any(&members.fields, is_listeners_field);
        let has_pubsub_method =
            any(&members.methods, is_subscribe_method) || any(&members.methods, is_notify_method);
        if has_listeners && has_pubsub_method {
            ctx.report_agent(
                &OBSERVER,
                class.span,
                format!(
                    "class {} hand-rolls the Observer pattern — use PubSub / Stream / SubscriptionRef",
                    class_name(class)
                ),
            );
        }
    }
}

pub struct Visitor;

impl Rule for Visitor {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&VISITOR];
        METAS
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let members = class_members(class);
        let visit_methods = members
            .methods
            .iter()
            .filter(|name| is_visit_method(name))
            .count();
        if visit_methods >= 2 {
            ctx.report_agent(
                &VISITOR,
                class.span,
                format!(
                    "class {} implements the Visitor pattern — use a tagged union + Match.exhaustive",
                    class_name(class)
                ),
            );
        }
    }
}

pub struct ChainOfResponsibility;

impl Rule for ChainOfResponsibility {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&CHAIN];
        METAS
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let members = class_members(class);
        let has_next = any(&members.fields, is_chain_next_field);
        let has_chain_method =
            any(&members.methods, is_handle_method) || any(&members.methods, is_set_next_method);
        if has_next && has_chain_method {
            ctx.report_agent(
                &CHAIN,
                class.span,
                format!(
                    "class {} is a Chain-of-Responsibility handler — compose with Effect.orElse / catchTag",
                    class_name(class)
                ),
            );
        }
    }
}

pub struct Strategy;

impl Strategy {
    /// True when the interface body is exactly one method signature — the
    /// structural tell of a Strategy interface (vs. a data/shape interface).
    fn is_single_method(interface: &TSInterfaceDeclaration<'_>) -> bool {
        let body = &interface.body.body;
        body.len() == 1 && matches!(body.first(), Some(TSSignature::TSMethodSignature(_)))
    }
}

impl Rule for Strategy {
    fn metas(&self) -> &'static [&'static RuleMeta] {
        static METAS: &[&RuleMeta] = &[&STRATEGY];
        METAS
    }

    fn on_interface(&self, interface: &TSInterfaceDeclaration<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() || !Strategy::is_single_method(interface) {
            return;
        }
        ctx.scratch
            .single_method_interfaces
            .push((interface.id.name.to_string(), interface.span));
    }

    fn on_class(&self, class: &Class<'_>, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        for implemented in &class.implements {
            if let TSTypeName::IdentifierReference(identifier) = &implemented.expression {
                ctx.scratch
                    .implemented_interfaces
                    .push(identifier.name.to_string());
            }
        }
    }

    fn on_file_end(&self, ctx: &mut FileCtx) {
        if !ctx.agent_active() {
            return;
        }
        let interfaces = std::mem::take(&mut ctx.scratch.single_method_interfaces);
        for (name, span) in &interfaces {
            let implementers = ctx
                .scratch
                .implemented_interfaces
                .iter()
                .filter(|implemented| *implemented == name)
                .count();
            if implementers >= 2 {
                ctx.report_agent(
                    &STRATEGY,
                    *span,
                    format!(
                        "interface {name} is a single-method Strategy with {implementers} implementations — use a function type"
                    ),
                );
            }
        }
    }
}
