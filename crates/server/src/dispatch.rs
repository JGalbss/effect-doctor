//! Line-delimited JSON dispatch over the [`Kernel`]: one request per line in,
//! one response per line out. The transport is deliberately trivial so it is
//! testable as pure string-in/string-out and depends on nothing networked.
//!
//! Request:  `{"id": <any>, "method": "<name>", "params": { … }}`
//! Response: `{"id": <any>, "result": { … }}` or `{"id": <any>, "error": "…"}`

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::Kernel;

/// Dispatch one method call against the kernel, returning the `result` value.
pub fn handle(kernel: &mut Kernel, method: &str, params: &Value) -> Result<Value, String> {
    match method {
        "symbol_exists" => {
            let name = str_param(params, "name")?;
            to_value(kernel.symbol_exists(name))
        }
        "impact" => {
            let changed = strings_param(params, "changed")?;
            let always_run = optional_strings(params, "always_run");
            to_value(kernel.impact(&changed, always_run))
        }
        "gate" => {
            let changed = strings_param(params, "changed")?;
            let actor = params.get("actor").and_then(Value::as_str);
            to_value(kernel.gate(&changed, actor))
        }
        "context_pack" => {
            let changed = strings_param(params, "changed")?;
            let actor = params.get("actor").and_then(Value::as_str);
            to_value(kernel.context_pack(&changed, actor))
        }
        "update_file" => {
            let path = str_param(params, "path")?;
            kernel.update_file(path);
            Ok(json!({ "ok": true }))
        }
        other => Err(format!("unknown method: {other}")),
    }
}

/// Run the stdio request/response loop until EOF.
pub fn serve(kernel: &mut Kernel) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = respond(kernel, &line);
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

/// Parse one request line and produce the response line.
fn respond(kernel: &mut Kernel, line: &str) -> String {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => return json!({ "error": format!("invalid JSON: {error}") }).to_string(),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(json!({}));
    match handle(kernel, method, &params) {
        Ok(result) => json!({ "id": id, "result": result }).to_string(),
        Err(error) => json!({ "id": id, "error": error }).to_string(),
    }
}

fn str_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string param '{key}'"))
}

fn strings_param(params: &Value, key: &str) -> Result<Vec<String>, String> {
    params
        .get(key)
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .ok_or_else(|| format!("missing array param '{key}'"))
}

fn optional_strings(params: &Value, key: &str) -> Vec<String> {
    params
        .get(key)
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn kernel_with(files: &[(&str, &str)]) -> (Kernel, std::path::PathBuf) {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("ad-dispatch-{}-{}", std::process::id(), unique));
        for (name, source) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        (Kernel::build_bare(&dir), dir)
    }

    #[test]
    fn dispatch_symbol_exists() {
        let (mut kernel, dir) = kernel_with(&[("a.ts", "export function foo() {}")]);
        let result = handle(&mut kernel, "symbol_exists", &json!({"name": "foo"})).unwrap();
        assert_eq!(result.as_array().unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dispatch_impact_and_unknown() {
        let (mut kernel, dir) = kernel_with(&[
            ("src/a.ts", "export const x = 1"),
            ("test/a.test.ts", "import { x } from '../src/a'"),
        ]);
        let result = handle(&mut kernel, "impact", &json!({"changed": ["src/a.ts"]})).unwrap();
        let tests = result.get("tests").unwrap().as_array().unwrap();
        assert_eq!(tests.len(), 1);
        assert!(handle(&mut kernel, "bogus", &json!({})).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respond_wraps_id_and_result() {
        let (mut kernel, dir) = kernel_with(&[("a.ts", "export function foo() {}")]);
        let line = r#"{"id": 7, "method": "symbol_exists", "params": {"name": "foo"}}"#;
        let response = respond(&mut kernel, line);
        let value: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(value["id"], 7);
        assert!(value["result"].is_array());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respond_reports_invalid_json() {
        let (mut kernel, dir) = kernel_with(&[("a.ts", "export const x = 1")]);
        let response = respond(&mut kernel, "{not json");
        assert!(response.contains("error"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
