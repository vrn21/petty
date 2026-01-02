use petty_agent_comms::protocol::{ExecuteParams, ExecuteResult};
use anyhow::{Result, Context};
use tokio::process::Command;
use std::process::Stdio;
use std::time::Duration;

pub struct Executor;

impl Executor {
    pub async fn execute(params: serde_json::Value) -> Result<serde_json::Value> {
        let params: ExecuteParams = serde_json::from_value(params)?;
        
        if params.command.is_empty() {
            return Err(anyhow::anyhow!("Command cannot be empty"));
        }

        let mut cmd = Command::new(&params.command[0]);
        if params.command.len() > 1 {
            cmd.args(&params.command[1..]);
        }

        if let Some(cwd) = params.cwd {
            cmd.current_dir(cwd);
        }

        if let Some(env) = params.env {
            cmd.envs(env);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let timeout = params.timeout_secs.unwrap_or(30);
        
        let child = cmd.spawn().context("Failed to spawn command")?;
        
        let output = match tokio::time::timeout(
            Duration::from_secs(timeout),
            child.wait_with_output()
        ).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => return Err(anyhow::anyhow!("Command execution failed: {}", e)),
            Err(_) => return Err(anyhow::anyhow!("Command timed out after {} seconds", timeout)),
        };

        let result = ExecuteResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        };

        Ok(serde_json::to_value(result)?)
    }
}
