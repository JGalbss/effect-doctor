//! A pluggable [`Executor`] that shells out to an external command — the seam
//! where a real agent (e.g. a wrapper around `claude -p`) plugs into the
//! deterministic loop. The contract is JSON over stdio:
//!   - stdin:  the [`TaskSpec`] (task + context pack + retry feedback),
//!   - stdout: a [`TaskOutcome`] (the proposed file changes + a summary).
//!
//! The agent is the only nondeterministic step; everything around it (gate,
//! tests, lease) stays deterministic and is enforced regardless of its output.

use std::io::Write;
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::{Executor, TaskOutcome, TaskSpec};

/// Runs a subprocess per task, piping the spec in and parsing the outcome out.
pub struct CommandExecutor {
    program: String,
    args: Vec<String>,
}

impl CommandExecutor {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> CommandExecutor {
        CommandExecutor {
            program: program.into(),
            args,
        }
    }
}

/// The wire form of a task spec sent to the executor command.
#[derive(Serialize)]
struct WireSpec<'a> {
    task: &'a crate::Task,
    context: &'a agent_doctor_server::ContextPack,
    feedback: &'a [String],
}

impl Executor for CommandExecutor {
    fn run(&mut self, spec: &TaskSpec) -> Result<TaskOutcome, String> {
        let wire = WireSpec {
            task: &spec.task,
            context: &spec.context,
            feedback: &spec.feedback,
        };
        let input = serde_json::to_string(&wire).map_err(|error| error.to_string())?;

        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("spawn {}: {error}", self.program))?;
        child
            .stdin
            .take()
            .ok_or("no stdin handle")?
            .write_all(input.as_bytes())
            .map_err(|error| error.to_string())?;
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("executor exited with {}", output.status));
        }
        serde_json::from_slice::<TaskOutcome>(&output.stdout)
            .map_err(|error| format!("invalid executor output: {error}"))
    }
}
