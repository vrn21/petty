# Phase 3: petty-core

> Sandbox orchestration layer connecting VMs to guest agents.

---

## Purpose

Host-side library that:

1. Creates and manages sandbox instances
2. Connects to guest agents via vsock
3. Provides high-level API for code execution and file operations
4. Tracks sandbox lifecycle and state

---

## Core Concepts

### Sandbox

A running microVM with an active connection to its guest agent. Represents a complete isolated execution environment.

### AgentClient

A client that communicates with `petty-agent` running inside the VM via Firecracker's vsock Unix socket.

### SandboxManager

Manages the lifecycle of multiple sandboxes, providing create/get/destroy operations.

---

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    petty-core (host)                     │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  ┌─────────────────┐     ┌──────────────────────────┐   │
│  │ SandboxManager  │────▶│  HashMap<SandboxId,      │   │
│  │   - create()    │     │           Sandbox>       │   │
│  │   - get()       │     └──────────────────────────┘   │
│  │   - destroy()   │                                    │
│  └─────────────────┘                                    │
│           │                                              │
│           ▼                                              │
│  ┌─────────────────┐     ┌──────────────────────────┐   │
│  │    Sandbox      │────▶│   VirtualMachine         │   │
│  │  - execute()    │     │   (from petty-vm)        │   │
│  │  - read_file()  │     └──────────────────────────┘   │
│  │  - write_file() │                │                   │
│  └─────────────────┘                │ vsock             │
│           │                         ▼                   │
│  ┌─────────────────┐     ┌──────────────────────────┐   │
│  │  AgentClient    │────▶│  Unix Socket             │   │
│  │  - call()       │     │  (Firecracker vsock)     │   │
│  │  - ping()       │     └──────────────────────────┘   │
│  └─────────────────┘                                    │
│                                                          │
└──────────────────────────────────────────────────────────┘
                          │
                          ▼
┌──────────────────────────────────────────────────────────┐
│                  petty-agent (guest)                     │
│              Listening on vsock port 52                  │
└──────────────────────────────────────────────────────────┘
```

---

## File Structure

```
crates/petty-core/
├── Cargo.toml
└── src/
    ├── lib.rs        # Public exports
    ├── sandbox.rs    # Sandbox type and methods
    ├── manager.rs    # SandboxManager
    ├── client.rs     # AgentClient (vsock communication)
    ├── config.rs     # SandboxConfig
    └── error.rs      # Error types
```

---

## Components

### 1. SandboxConfig

Configuration for creating a new sandbox.

**Fields:**

- `language` - Primary language (python, node, bash, etc.)
- `memory_mib` - Memory allocation (default: 256)
- `vcpu_count` - CPU count (default: 2)
- `timeout` - Maximum execution time (optional)
- `kernel_path` - Path to kernel image
- `rootfs_path` - Path to rootfs image

**Behavior:**

- Provide sensible defaults for all fields
- Validate configuration before VM creation
- Support builder pattern for ergonomic configuration

---

### 2. AgentClient

Communicates with petty-agent inside the VM.

**Connection Protocol (Firecracker vsock):**

1. Connect to Firecracker's vsock Unix socket at `{chroot_path}/{vm_id}/v.sock`
2. Send `CONNECT 52\n` to establish connection to guest port 52
3. Read response (OK or error)
4. Exchange newline-delimited JSON-RPC messages

**Methods:**

- `connect(vsock_path)` - Establish connection to guest
- `ping()` - Health check
- `exec(cmd)` - Execute shell command
- `exec_code(lang, code)` - Execute code in language
- `read_file(path)` - Read file from guest
- `write_file(path, content)` - Write file to guest
- `list_dir(path)` - List directory contents
- `call<T>(method, params)` - Generic JSON-RPC call

**Error Handling:**

- Connection timeout (guest not ready)
- Connection refused (agent not running)
- RPC errors (from agent)
- JSON parse errors

**Retry Logic:**

- On connect: retry with backoff until timeout
- Guest agent may take 1-2 seconds to start after VM boot

---

### 3. Sandbox

A running sandbox with VM and agent connection.

**Fields:**

- `id` - Unique identifier (UUID)
- `vm` - VirtualMachine from petty-vm
- `client` - AgentClient for communication
- `config` - SandboxConfig used to create it
- `created_at` - Creation timestamp

**Lifecycle:**

1. **Create**: Boot VM → Wait for agent → Connect
2. **Execute**: Send commands via AgentClient
3. **Destroy**: Kill VM → Cleanup resources

**Methods:**

- `id()` - Get sandbox ID
- `execute(cmd)` - Run shell command, return ExecResult
- `execute_code(lang, code)` - Run code, return ExecResult
- `read_file(path)` - Read file contents
- `write_file(path, content)` - Write file
- `list_dir(path)` - List directory
- `destroy()` - Cleanup and destroy

**State Flow:**

```
Creating → Ready → [Executing ↔ Ready] → Destroyed
```

---

### 4. SandboxManager

Manages multiple sandbox instances.

**Fields:**

- `sandboxes` - Map of active sandboxes
- `config` - Manager configuration (paths, defaults)

**Methods:**

- `new(config)` - Create manager with config
- `create(config)` - Create new sandbox, return ID
- `get(id)` - Get sandbox by ID (immutable)
- `get_mut(id)` - Get sandbox by ID (mutable)
- `destroy(id)` - Destroy specific sandbox
- `destroy_all()` - Cleanup all sandboxes
- `list()` - List all sandbox IDs

**Configuration:**

- `kernel_path` - Default kernel image
- `images_dir` - Directory containing rootfs images
- `chroot_path` - Working directory for VMs
- `firecracker_path` - Path to Firecracker binary

---

### 5. Error Types

**CoreError enum:**

- `Vm(VmError)` - From petty-vm
- `Connection(String)` - vsock connection failed
- `AgentTimeout` - Agent not responding
- `Rpc { code, message }` - JSON-RPC error from agent
- `NotFound(SandboxId)` - Sandbox not found
- `InvalidState { expected, actual }` - Wrong state for operation

---

## Implementation Tasks

### Task 1: Create Crate Structure

- Set up Cargo.toml with dependencies on petty-vm
- Create module files
- Define public exports in lib.rs

### Task 2: Define Types

- `SandboxId` (newtype around UUID)
- `SandboxConfig` with defaults and builder
- `SandboxState` enum
- `CoreError` with proper error handling

### Task 3: Implement AgentClient

- Connect to vsock Unix socket
- Send CONNECT handshake
- Implement JSON-RPC message exchange
- Add retry logic for initial connection
- Add timeout handling

### Task 4: Implement Sandbox

- Wrap VirtualMachine + AgentClient
- Implement execute/file operation methods
- Handle connection lifecycle
- Implement destroy with cleanup

### Task 5: Implement SandboxManager

- Manage sandbox HashMap
- Implement create with VM boot + agent connect
- Track sandbox state
- Implement destroy and list operations

### Task 6: Add Integration Tests

- Test sandbox creation and destruction
- Test command execution
- Test file operations
- Test error handling

---

## Dependencies

**Required crates:**

- `petty-vm` - VM management
- `tokio` - Async runtime, UnixStream, timeouts
- `serde` / `serde_json` - JSON-RPC serialization
- `uuid` - Sandbox IDs
- `thiserror` - Error types
- `tracing` - Logging

---

## Key Behaviors

### Sandbox Creation Flow

1. Validate SandboxConfig
2. Create VirtualMachine via petty-vm (with vsock enabled)
3. Wait for VM to boot (vsock socket appears)
4. Connect AgentClient to vsock
5. Wait for agent ready (ping succeeds)
6. Return Sandbox instance

### Agent Connection Retry

- Retry every 100ms for up to 10 seconds
- Agent needs time to start after kernel boot
- Fail with AgentTimeout if not ready

### Graceful Cleanup

- On destroy: attempt to stop VM gracefully
- On drop: force kill if still running
- Remove socket files and temp directories

---

## Acceptance Criteria

- [ ] SandboxConfig with sensible defaults
- [ ] AgentClient connects via vsock and sends JSON-RPC
- [ ] Sandbox wraps VM + AgentClient
- [ ] execute() runs commands and returns result
- [ ] File operations work (read/write/list)
- [ ] SandboxManager tracks multiple sandboxes
- [ ] Proper error handling for all failure modes
- [ ] Connection retry logic works
- [ ] Graceful cleanup on destroy

---

## Testing Notes

- Unit tests for config validation and error types
- Integration tests require petty-vm and petty-agent
- Mock AgentClient for unit testing Sandbox logic
- End-to-end tests with real VMs (Linux + KVM only)
