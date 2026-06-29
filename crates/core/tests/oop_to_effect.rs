//! OOP → Effect rules (`--agent`): hand-rolled design patterns Effect replaces.
//! The linter only analyzes files importing `effect`, so every fixture includes
//! the import prelude.

mod common;

use common::{assert_fires_agent, lint_agent_strict};

const PRELUDE: &str = "import { Effect } from \"effect\"\n";

fn src(body: &str) -> String {
    format!("{PRELUDE}{body}")
}

#[test]
fn flags_singleton() {
    let source = src(
        "export class Db {\n  private static instance: Db\n  private constructor() {}\n  static getInstance() {\n    if (!Db.instance) Db.instance = new Db()\n    return Db.instance\n  }\n}\n",
    );
    assert_fires_agent(&source, "oop-singleton-to-layer", 1);
}

#[test]
fn static_helper_class_is_not_a_singleton() {
    // static method but no static `instance` field → not a singleton.
    let source = src("export class MathUtil {\n  static add(a: number, b: number) { return a + b }\n}\n");
    assert_fires_agent(&source, "oop-singleton-to-layer", 0);
}

#[test]
fn flags_observer() {
    let source = src(
        "export class Emitter {\n  private listeners: Array<(e: number) => void> = []\n  subscribe(fn: (e: number) => void) { this.listeners.push(fn) }\n  notify(e: number) { for (const fn of this.listeners) fn(e) }\n}\n",
    );
    assert_fires_agent(&source, "oop-observer-to-pubsub", 1);
}

#[test]
fn listeners_field_without_pubsub_methods_is_not_observer() {
    let source = src(
        "export class Registry {\n  private listeners: number[] = []\n  count() { return this.listeners.length }\n}\n",
    );
    assert_fires_agent(&source, "oop-observer-to-pubsub", 0);
}

#[test]
fn flags_strategy_with_two_impls() {
    let source = src(
        "interface Discount { apply(total: number): number }\nexport class NoDiscount implements Discount { apply(t: number) { return t } }\nexport class HalfOff implements Discount { apply(t: number) { return t / 2 } }\n",
    );
    assert_fires_agent(&source, "oop-strategy-to-function", 1);
}

#[test]
fn single_impl_interface_is_not_strategy() {
    let source = src(
        "interface Logger { log(s: string): void }\nexport class ConsoleLogger implements Logger { log(s: string) {} }\n",
    );
    assert_fires_agent(&source, "oop-strategy-to-function", 0);
}

#[test]
fn multi_method_interface_is_not_strategy() {
    let source = src(
        "interface Repo { find(id: string): number; save(x: number): void }\nexport class A implements Repo { find(id: string) { return 1 } save(x: number) {} }\nexport class B implements Repo { find(id: string) { return 2 } save(x: number) {} }\n",
    );
    assert_fires_agent(&source, "oop-strategy-to-function", 0);
}

#[test]
fn flags_visitor() {
    let source = src(
        "export class AreaVisitor {\n  visitCircle(c: { r: number }) { return c.r }\n  visitSquare(s: { side: number }) { return s.side }\n}\n",
    );
    assert_fires_agent(&source, "oop-visitor-to-match", 1);
}

#[test]
fn single_visit_method_is_not_visitor() {
    let source = src("export class Inspector {\n  visitNode(n: number) { return n }\n}\n");
    assert_fires_agent(&source, "oop-visitor-to-match", 0);
}

#[test]
fn flags_chain_of_responsibility() {
    let source = src(
        "export class AuthHandler {\n  next?: AuthHandler\n  setNext(h: AuthHandler) { this.next = h }\n  handle(req: number): number { return this.next ? this.next.handle(req) : req }\n}\n",
    );
    assert_fires_agent(&source, "oop-chain-to-catchtag", 1);
}

#[test]
fn next_field_without_handler_methods_is_not_chain() {
    // A linked-list node has `next` but no handle/setNext.
    let source = src("export class Node {\n  next?: Node\n  value = 0\n}\n");
    assert_fires_agent(&source, "oop-chain-to-catchtag", 0);
}

#[test]
fn oop_rules_are_silent_without_agent() {
    use common::lint;
    let source = src(
        "export class Db {\n  private static instance: Db\n  private constructor() {}\n  static getInstance() { return Db.instance }\n}\n",
    );
    let oop = lint(&source)
        .into_iter()
        .filter(|d| d.rule.starts_with("oop-"))
        .count();
    assert_eq!(oop, 0, "OOP rules must be opt-in (--agent)");
}

#[test]
fn agent_strict_escalates_to_error() {
    use agent_doctor_core::Severity;
    let source = src(
        "export class Db {\n  private static instance: Db\n  private constructor() {}\n  static getInstance() { return Db.instance }\n}\n",
    );
    let escalated = lint_agent_strict(&source)
        .into_iter()
        .any(|d| d.rule == "oop-singleton-to-layer" && d.severity == Severity::Error);
    assert!(escalated, "--agent-strict should escalate OOP findings to error");
}
