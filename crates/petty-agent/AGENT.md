# petty-agent

Guest agent for microVMs. Runs inside VM, listens on vsock/Unix socket, executes JSON-RPC commands.

## Build

```
cargo build -p petty-agent --release
```

## Run

```
cargo run -p petty-agent
# Listens: /tmp/petty-agent.sock
```

## Protocol

Transport: Unix socket (vsock port 52 in VM)
Format: JSON-RPC 2.0, newline-delimited

## Methods

### ping

Health check.

```json
{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}
→ {"result":{"pong":true}}
```

### exec

Run shell command.

```json
{"method":"exec","params":{"cmd":"echo hello"}}
→ {"result":{"exit_code":0,"stdout":"hello\n","stderr":""}}
```

### exec_code

Run code. Languages: python|python3, node|javascript|js, bash, sh

```json
{"method":"exec_code","params":{"lang":"python3","code":"print(2+2)"}}
→ {"result":{"exit_code":0,"stdout":"4\n","stderr":""}}
```

### read_file

Read file contents. Max 10MB.

```json
{"method":"read_file","params":{"path":"/etc/hostname"}}
→ {"result":{"content":"myhost\n"}}
```

### write_file

Write to file. Creates parent dirs.

```json
{"method":"write_file","params":{"path":"/tmp/x.txt","content":"data"}}
→ {"result":{"success":true}}
```

### list_dir

List directory.

```json
{"method":"list_dir","params":{"path":"/tmp"}}
→ {"result":{"entries":[{"name":"x.txt","is_dir":false,"size":4}]}}
```

## Error Codes

- -32700: Parse error
- -32601: Method not found
- -32602: Invalid params
- -32603: Internal error

## Limits

- Output: 1MB max (truncated)
- File read: 10MB max

## Files

```
src/
├── main.rs      # Socket listener
├── protocol.rs  # JSON-RPC types
├── handler.rs   # Method routing
├── exec.rs      # Command execution
└── fs.rs        # File operations
```

## Test

```
echo '{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}' | nc -U /tmp/petty-agent.sock
```
