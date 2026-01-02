# petty-core

Host-side sandbox orchestration. Creates VMs, connects to agents, manages lifecycle.

## Build

```
cargo build -p petty-core --release
```

## Quick Start

```rust
use petty_core::{SandboxManager, SandboxConfig, ManagerConfig};

let manager = SandboxManager::new(ManagerConfig::new(
    "/path/to/vmlinux",
    "/path/to/rootfs.ext4",
    "/usr/bin/firecracker",
    "/tmp/petty",
));

let config = SandboxConfig::builder()
    .kernel("/path/to/vmlinux")
    .rootfs("/path/to/rootfs.ext4")
    .build()?;

let id = manager.create(config).await?;
let result = manager.with_sandbox_async(id, |sb| async move {
    sb.execute_code("python", "print(1+1)").await
}).await?;
manager.destroy(id).await?;
```

## Types

### SandboxManager

Manages multiple sandboxes. Thread-safe.

Methods:

- `new(config)` - Create manager
- `create(config)` - Create sandbox, returns ID
- `create_default()` - Create with manager's default paths
- `with_sandbox_async(id, f)` - Run async closure on sandbox
- `with_sandbox(id, f)` - Run sync closure on sandbox
- `destroy(id)` - Destroy sandbox
- `destroy_all()` - Destroy all sandboxes
- `list()` - Get all sandbox IDs
- `count()` - Get sandbox count
- `exists(id)` - Check if sandbox exists

### ManagerConfig

Fields:

- `kernel_path` - Default kernel for new sandboxes
- `rootfs_path` - Default rootfs for new sandboxes
- `firecracker_path` - Firecracker binary path
- `chroot_path` - Working directory for sockets
- `max_sandboxes` - Max concurrent sandboxes (default: 100, 0=unlimited)

### Sandbox

A running VM with agent connection.

Methods:

- `id()` - Get sandbox ID
- `state()` - Get state (Creating|Ready|Destroyed)
- `execute(cmd)` - Run shell command, returns ExecResult
- `execute_code(lang, code)` - Run code, returns ExecResult
- `read_file(path)` - Read file contents
- `write_file(path, content)` - Write file
- `list_dir(path)` - List directory
- `is_healthy()` - Ping agent, returns bool (non-blocking)
- `destroy()` - Stop VM and cleanup

### SandboxConfig

Builder pattern:

```rust
SandboxConfig::builder()
    .kernel(path)      // Required
    .rootfs(path)      // Required
    .memory_mib(256)   // Default: 256
    .vcpu_count(2)     // Default: 2
    .timeout(dur)      // Optional
    .vsock_cid(3)      // Default: 3, must be >= 3
    .build()?
```

### ExecResult

Command result:

- `exit_code: i32` - Process exit code
- `stdout: String` - Standard output
- `stderr: String` - Standard error
- `success()` - Returns true if exit_code == 0

### FileEntry

Directory entry:

- `name: String` - File/dir name
- `is_dir: bool` - True if directory
- `size: u64` - Size in bytes

### SandboxId

UUID wrapper. Display/Hash/Eq.

## Error Handling

CoreError variants:

- `Vm(VmError)` - VM creation/operation failed
- `Connection(String)` - vsock connect failed
- `AgentTimeout(Duration)` - Agent not responsive
- `Rpc{code, message}` - Agent returned error
- `NotFound(SandboxId)` - Sandbox not found
- `InvalidState{expected, actual}` - Wrong sandbox state
- `Json(serde_json::Error)` - Serialization error
- `Io(io::Error)` - I/O error

## Connection

Connects via Firecracker vsock:

1. UnixStream to {chroot}/v.sock
2. Sends "CONNECT 52\n"
3. Reads "OK <port>\n"
4. Exchanges JSON-RPC

Retry: 100ms interval, 10s timeout

## Files

```
src/
├── lib.rs      # Public exports
├── config.rs   # SandboxConfig, builder
├── error.rs    # CoreError
├── client.rs   # AgentClient, vsock/RPC
├── sandbox.rs  # Sandbox, SandboxId, SandboxState
└── manager.rs  # SandboxManager, ManagerConfig
```

## Limits

- max_sandboxes: 100 default
- connect_timeout: 10s
- rpc_timeout: 30s
- vsock_cid: must be >= 3

## Dependencies

- petty-vm: VM creation
- petty-agent: Must be running in guest
