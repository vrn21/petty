//! Command and code execution for bouvet-agent.
//!
//! Provides functions to execute shell commands and code in various languages.

use crate::protocol::ExecResult;
use std::process::Command;
use tracing::{debug, trace, warn};

/// Maximum output size in bytes (1 MB).
/// Prevents memory exhaustion from commands with huge output.
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

/// Truncate a string to max bytes, preserving UTF-8 boundaries.
fn truncate_output(s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }
    // Find a valid UTF-8 boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = s[..end].to_string();
    truncated.push_str("\n... [output truncated]");
    truncated
}

/// Execute a shell command via `sh -c`.
///
/// # Arguments
/// * `cmd` - The shell command to execute.
///
/// # Returns
/// An `ExecResult` containing exit code, stdout, and stderr.
/// Output is truncated to 1MB to prevent memory exhaustion.
pub fn exec_command(cmd: &str) -> ExecResult {
    debug!(cmd = %cmd, "executing shell command");
    let output = Command::new("sh").args(["-c", cmd]).output();

    match output {
        Ok(out) => {
            let exit_code = out.status.code().unwrap_or(-1);
            let stdout = truncate_output(
                String::from_utf8_lossy(&out.stdout).into_owned(),
                MAX_OUTPUT_SIZE,
            );
            let stderr = truncate_output(
                String::from_utf8_lossy(&out.stderr).into_owned(),
                MAX_OUTPUT_SIZE,
            );
            debug!(
                exit_code = exit_code,
                stdout_len = stdout.len(),
                stderr_len = stderr.len(),
                "command completed"
            );
            trace!(stdout = %stdout, stderr = %stderr, "command output");
            ExecResult {
                exit_code,
                stdout,
                stderr,
            }
        }
        Err(e) => {
            warn!(error = %e, cmd = %cmd, "command execution failed");
            ExecResult::error(&e.to_string())
        }
    }
}

/// Execute code in a specified programming language.
///
/// Supported languages:
/// - `python`, `python3` - Python 3
/// - `node`, `javascript` - Node.js
/// - `bash`, `sh` - Shell script
///
/// # Arguments
/// * `lang` - The programming language.
/// * `code` - The code to execute.
///
/// # Returns
/// An `ExecResult` containing exit code, stdout, and stderr.
pub fn exec_code(lang: &str, code: &str) -> ExecResult {
    debug!(lang = %lang, code_len = code.len(), "executing code");
    trace!(code = %code, "code to execute");

    let (program, args): (&str, Vec<&str>) = match lang.to_lowercase().as_str() {
        "python" | "python3" => ("python3", vec!["-c", code]),
        "node" | "javascript" | "js" => ("node", vec!["-e", code]),
        "bash" => ("bash", vec!["-c", code]),
        "sh" => ("sh", vec!["-c", code]),
        _ => {
            warn!(lang = %lang, "unsupported language requested");
            return ExecResult::error(&format!("unsupported language: {}", lang));
        }
    };

    debug!(program = %program, "using interpreter");
    let output = Command::new(program).args(&args).output();

    match output {
        Ok(out) => {
            let exit_code = out.status.code().unwrap_or(-1);
            let stdout = truncate_output(
                String::from_utf8_lossy(&out.stdout).into_owned(),
                MAX_OUTPUT_SIZE,
            );
            let stderr = truncate_output(
                String::from_utf8_lossy(&out.stderr).into_owned(),
                MAX_OUTPUT_SIZE,
            );
            debug!(
                exit_code = exit_code,
                stdout_len = stdout.len(),
                stderr_len = stderr.len(),
                "code execution completed"
            );
            trace!(stdout = %stdout, stderr = %stderr, "code output");
            ExecResult {
                exit_code,
                stdout,
                stderr,
            }
        }
        Err(e) => {
            warn!(error = %e, program = %program, "code execution failed");
            ExecResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("failed to execute {}: {}", program, e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_command_echo() {
        let result = exec_command("echo hello");
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_exec_command_exit_code() {
        let result = exec_command("exit 42");
        assert_eq!(result.exit_code, 42);
    }

    #[test]
    fn test_exec_command_stderr() {
        let result = exec_command("echo error >&2");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_empty());
        assert_eq!(result.stderr.trim(), "error");
    }

    #[test]
    fn test_exec_code_unsupported() {
        let result = exec_code("cobol", "DISPLAY 'HELLO'");
        assert_eq!(result.exit_code, -1);
        assert!(result.stderr.contains("unsupported language"));
    }
}
