//! Request handler for bouvet-agent.
//!
//! Routes JSON-RPC requests to the appropriate handlers.

use crate::exec::{exec_code, exec_command};
use crate::fs::{list_dir, read_file, write_file};
use crate::protocol::{
    error_codes, ExecCodeParams, ExecParams, ListDirParams, ReadFileParams, Request, Response,
    WriteFileParams,
};
use serde_json::{json, Value};
use tracing::{debug, trace, warn};

/// Handle a JSON-RPC request and return a response.
///
/// Supported methods:
/// - `ping` - Health check, returns `{pong: true}`.
/// - `exec` - Execute a shell command.
/// - `exec_code` - Execute code in a specified language.
/// - `read_file` - Read a file's contents.
/// - `write_file` - Write content to a file.
/// - `list_dir` - List directory contents.
pub fn handle_request(req: Request) -> Response {
    debug!(method = %req.method, id = req.id, "handling request");
    trace!(params = ?req.params, "request params");

    let response = match req.method.as_str() {
        "ping" => {
            debug!(id = req.id, "ping request");
            Response::success(req.id, json!({"pong": true}))
        }

        "exec" => handle_exec(req.id, req.params),

        "exec_code" => handle_exec_code(req.id, req.params),

        "read_file" => handle_read_file(req.id, req.params),

        "write_file" => handle_write_file(req.id, req.params),

        "list_dir" => handle_list_dir(req.id, req.params),

        _ => {
            warn!(method = %req.method, "unknown method");
            Response::error(
                req.id,
                error_codes::METHOD_NOT_FOUND,
                format!("method not found: {}", req.method),
            )
        }
    };

    if response.error.is_some() {
        debug!(id = req.id, error = ?response.error, "request failed");
    } else {
        debug!(id = req.id, "request succeeded");
        trace!(result = ?response.result, "response result");
    }

    response
}

/// Handle the `exec` method.
fn handle_exec(id: u64, params: Value) -> Response {
    match serde_json::from_value::<ExecParams>(params) {
        Ok(p) => {
            debug!(id = id, cmd = %p.cmd, "handling exec");
            let result = exec_command(&p.cmd);
            match serde_json::to_value(&result) {
                Ok(v) => Response::success(id, v),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e.to_string()),
            }
        }
        Err(e) => {
            warn!(id = id, error = %e, "invalid exec params");
            Response::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("invalid params: {}", e),
            )
        }
    }
}

/// Handle the `exec_code` method.
fn handle_exec_code(id: u64, params: Value) -> Response {
    match serde_json::from_value::<ExecCodeParams>(params) {
        Ok(p) => {
            debug!(id = id, lang = %p.lang, code_len = p.code.len(), "handling exec_code");
            let result = exec_code(&p.lang, &p.code);
            match serde_json::to_value(&result) {
                Ok(v) => Response::success(id, v),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e.to_string()),
            }
        }
        Err(e) => {
            warn!(id = id, error = %e, "invalid exec_code params");
            Response::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("invalid params: {}", e),
            )
        }
    }
}

/// Handle the `read_file` method.
fn handle_read_file(id: u64, params: Value) -> Response {
    match serde_json::from_value::<ReadFileParams>(params) {
        Ok(p) => {
            debug!(id = id, path = %p.path, "handling read_file");
            match read_file(&p.path) {
                Ok(content) => Response::success(id, json!({"content": content})),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e),
            }
        }
        Err(e) => {
            warn!(id = id, error = %e, "invalid read_file params");
            Response::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("invalid params: {}", e),
            )
        }
    }
}

/// Handle the `write_file` method.
fn handle_write_file(id: u64, params: Value) -> Response {
    match serde_json::from_value::<WriteFileParams>(params) {
        Ok(p) => {
            debug!(id = id, path = %p.path, content_len = p.content.len(), "handling write_file");
            match write_file(&p.path, &p.content) {
                Ok(success) => Response::success(id, json!({"success": success})),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e),
            }
        }
        Err(e) => {
            warn!(id = id, error = %e, "invalid write_file params");
            Response::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("invalid params: {}", e),
            )
        }
    }
}

/// Handle the `list_dir` method.
fn handle_list_dir(id: u64, params: Value) -> Response {
    match serde_json::from_value::<ListDirParams>(params) {
        Ok(p) => {
            debug!(id = id, path = %p.path, "handling list_dir");
            match list_dir(&p.path) {
                Ok(entries) => Response::success(id, json!({"entries": entries})),
                Err(e) => Response::error(id, error_codes::INTERNAL_ERROR, e),
            }
        }
        Err(e) => {
            warn!(id = id, error = %e, "invalid list_dir params");
            Response::error(
                id,
                error_codes::INVALID_PARAMS,
                format!("invalid params: {}", e),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_request(method: &str, params: Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: method.to_string(),
            params,
        }
    }

    #[test]
    fn test_ping() {
        let req = make_request("ping", json!({}));
        let resp = handle_request(req);
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), json!({"pong": true}));
    }

    #[test]
    fn test_exec() {
        let req = make_request("exec", json!({"cmd": "echo test"}));
        let resp = handle_request(req);
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["exit_code"], 0);
        assert_eq!(result["stdout"].as_str().unwrap().trim(), "test");
    }

    #[test]
    fn test_method_not_found() {
        let req = make_request("unknown_method", json!({}));
        let resp = handle_request(req);
        assert!(resp.result.is_none());
        assert_eq!(resp.error.unwrap().code, error_codes::METHOD_NOT_FOUND);
    }

    #[test]
    fn test_invalid_params() {
        let req = make_request("exec", json!({"wrong_param": "value"}));
        let resp = handle_request(req);
        assert!(resp.result.is_none());
        assert_eq!(resp.error.unwrap().code, error_codes::INVALID_PARAMS);
    }
}
