# Agent Protocol Specification

> **Layer 3 (bouvet-agent)** — JSON-RPC 2.0 protocol for guest-host communication.

This document specifies the wire protocol used between the host (`AgentClient` in `bouvet-core`) and the guest (`bouvet-agent` daemon).

---

## Wire Format

All messages are **newline-delimited JSON-RPC 2.0**. Each message is a single JSON object followed by `\n`:

```
{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}\n
```

---

## Request Schema

```json
{
  "jsonrpc": "2.0",        // Required, must be "2.0"
  "id": 1,                 // Required, u64 identifier
  "method": "exec",        // Required, method name
  "params": {...}          // Optional, object or array
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `jsonrpc` | string | Yes | Must be `"2.0"` |
| `id` | u64 | Yes | Request identifier, echoed in response |
| `method` | string | Yes | Method name to invoke |
| `params` | object/array | No | Method parameters (defaults to `{}`) |

---

## Response Schema

### Success Response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {...}
}
```

### Error Response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32601,
    "message": "method not found: unknown",
    "data": null
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `jsonrpc` | string | Always `"2.0"` |
| `id` | u64 | Matches request `id` |
| `result` | object | Present on success (mutually exclusive with `error`) |
| `error` | object | Present on failure |

---

## Methods Reference

| Method | Params | Result | Description |
|--------|--------|--------|-------------|
| `ping` | `{}` | `{pong: true}` | Health check |
| `exec` | `{cmd: string}` | `ExecResult` | Shell command execution |
| `exec_code` | `{lang: string, code: string}` | `ExecResult` | Code execution |
| `read_file` | `{path: string}` | `{content: string}` | Read file contents |
| `write_file` | `{path: string, content: string}` | `{success: bool}` | Write file contents |
| `list_dir` | `{path: string}` | `{entries: FileEntry[]}` | List directory |

---

## Type Definitions

### ExecResult

Returned by `exec` and `exec_code` methods:

```json
{
  "exit_code": 0,      // i32, -1 if spawn failed
  "stdout": "...",     // string, max 1MB
  "stderr": "..."      // string, max 1MB
}
```

> [!NOTE]
> Output is truncated to 1MB per stream to prevent memory exhaustion. If truncated, the message `\n... [output truncated]` is appended.

### FileEntry

Returned in `list_dir` response:

```json
{
  "name": "file.txt",  // string, filename only
  "is_dir": false,     // boolean
  "size": 1024         // u64 bytes (0 for directories)
}
```

---

## Error Codes

Standard JSON-RPC 2.0 error codes:

| Code | Name | Description |
|------|------|-------------|
| `-32700` | Parse error | Invalid JSON received |
| `-32600` | Invalid request | Not a valid request object |
| `-32601` | Method not found | Unknown method name |
| `-32602` | Invalid params | Wrong or missing parameters |
| `-32603` | Internal error | Server-side error |

---

## Language Mapping

The `exec_code` method maps language names to interpreters:

| Input | Interpreter | Notes |
|-------|-------------|-------|
| `python`, `python3` | `python3 -c` | Python 3.11 |
| `node`, `javascript`, `js` | `node -e` | Node.js 20 |
| `bash` | `bash -c` | Bash 5.x |
| `sh` | `sh -c` | POSIX shell |

Unsupported languages return an error:

```json
{
  "exit_code": -1,
  "stdout": "",
  "stderr": "unsupported language: cobol"
}
```

---

## Examples

### Ping (Health Check)

**Request:**
```json
{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}
```

**Response:**
```json
{"jsonrpc":"2.0","id":1,"result":{"pong":true}}
```

---

### Execute Shell Command

**Request:**
```json
{"jsonrpc":"2.0","id":2,"method":"exec","params":{"cmd":"echo hello && ls -la"}}
```

**Response:**
```json
{
  "jsonrpc":"2.0",
  "id":2,
  "result":{
    "exit_code":0,
    "stdout":"hello\ntotal 8\ndrwxr-xr-x 2 root root 4096 ...",
    "stderr":""
  }
}
```

---

### Execute Python Code

**Request:**
```json
{"jsonrpc":"2.0","id":3,"method":"exec_code","params":{"lang":"python","code":"print('Hello from Python!')"}}
```

**Response:**
```json
{
  "jsonrpc":"2.0",
  "id":3,
  "result":{
    "exit_code":0,
    "stdout":"Hello from Python!\n",
    "stderr":""
  }
}
```

---

### Read File

**Request:**
```json
{"jsonrpc":"2.0","id":4,"method":"read_file","params":{"path":"/etc/hostname"}}
```

**Response:**
```json
{
  "jsonrpc":"2.0",
  "id":4,
  "result":{"content":"microvm\n"}
}
```

---

### Write File

**Request:**
```json
{"jsonrpc":"2.0","id":5,"method":"write_file","params":{"path":"/tmp/test.txt","content":"Hello, World!"}}
```

**Response:**
```json
{"jsonrpc":"2.0","id":5,"result":{"success":true}}
```

---

### List Directory

**Request:**
```json
{"jsonrpc":"2.0","id":6,"method":"list_dir","params":{"path":"/tmp"}}
```

**Response:**
```json
{
  "jsonrpc":"2.0",
  "id":6,
  "result":{
    "entries":[
      {"name":"test.txt","is_dir":false,"size":13},
      {"name":"scripts","is_dir":true,"size":0}
    ]
  }
}
```

---

### Error Response Example

**Request (unknown method):**
```json
{"jsonrpc":"2.0","id":7,"method":"unknown","params":{}}
```

**Response:**
```json
{
  "jsonrpc":"2.0",
  "id":7,
  "error":{"code":-32601,"message":"method not found: unknown"}
}
```

---

## Related Files

- [protocol.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/protocol.rs) — Type definitions
- [handler.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/handler.rs) — Request routing
- [exec.rs](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-agent/src/exec.rs) — Command/code execution
