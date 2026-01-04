# Architecture

Technical deep dive into Bouvet's internals.

---

## Overview

Bouvet is a layered system that creates isolated execution environments for AI agents. Each layer handles a specific concern:

```
┌─────────────────────────────────────────────────────────────────┐
│                         AI Agent                                 │
│                    (Claude, Cursor, etc.)                        │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ MCP Protocol (JSON-RPC)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        bouvet-mcp                                │
│                      (MCP Server Layer)                          │
│   • Exposes MCP tools (create_sandbox, execute_code, etc.)       │
│   • Handles stdio and HTTP transports                           │
│   • Manages warm sandbox pool                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Rust API
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        bouvet-core                               │
│                   (Orchestration Layer)                          │
│   • SandboxManager — lifecycle management                       │
│   • Sandbox — high-level sandbox abstraction                     │
│   • AgentClient — vsock communication                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Rust API
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         bouvet-vm                                │
│                        (VM Layer)                                │
│   • VirtualMachine — Firecracker wrapper                        │
│   • VmBuilder — configuration builder                           │
│   • Vsock configuration                                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ firepilot SDK
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Firecracker                               │
│                     (Hypervisor Layer)                           │
│   • KVM-based microVM                                           │
│   • ~5ms boot time                                              │
│   • Hardware-level isolation                                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ vsock (AF_VSOCK)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       bouvet-agent                               │
│                      (Guest Daemon)                              │
│   • Runs inside the microVM                                     │
│   • Executes commands and code                                  │
│   • Handles file operations                                     │
└─────────────────────────────────────────────────────────────────┘
```

---

## Crate Structure

```
bouvet/
├── crates/
│   ├── bouvet-mcp/      # MCP server (binary)
│   ├── bouvet-core/     # Sandbox orchestration (library)
│   ├── bouvet-vm/       # VM management (library)
│   └── bouvet-agent/    # Guest agent (binary, runs in VM)
├── images/              # Rootfs image build
├── terraform/           # AWS deployment
└── scripts/             # Build and runtime scripts
```

---

## Layer 1: bouvet-vm

The lowest layer — a thin wrapper around Firecracker.

### Purpose

Abstracts Firecracker complexity into a simple `VirtualMachine` type.

### Components

```
┌────────────────────────────────────────────────────┐
│                    bouvet-vm                        │
├────────────────────────────────────────────────────┤
│                                                    │
│  ┌──────────────────┐   ┌──────────────────────┐  │
│  │    VmBuilder     │──▶│   MachineConfig      │  │
│  │ (fluent config)  │   │   (validated config) │  │
│  └──────────────────┘   └──────────────────────┘  │
│                                │                   │
│                                ▼                   │
│                     ┌──────────────────────┐      │
│                     │   VirtualMachine     │      │
│                     │ ──────────────────── │      │
│                     │ • create()           │      │
│                     │ • start() / stop()   │      │
│                     │ • destroy()          │      │
│                     │ • vsock_uds_path()   │      │
│                     └──────────────────────┘      │
│                                                    │
└────────────────────────────────────────────────────┘
```

### Key Types

| Type             | Description                    |
| ---------------- | ------------------------------ |
| `VmBuilder`      | Fluent API for configuring VMs |
| `MachineConfig`  | Validated configuration        |
| `VirtualMachine` | Running VM instance            |
| `VsockConfig`    | vsock socket configuration     |

### Example

```rust
let vm = VmBuilder::new()
    .kernel("/var/lib/bouvet/vmlinux")
    .rootfs("/var/lib/bouvet/rootfs.ext4")
    .vcpus(1)
    .memory_mib(256)
    .with_vsock(3)  // CID for guest communication
    .build()
    .await?;

// VM is now running
let socket = vm.vsock_uds_path();
```

---

## Layer 2: bouvet-agent

The guest-side daemon that runs inside each microVM.

### Purpose

Receives commands from the host, executes them, and returns results.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                       microVM (guest)                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                   bouvet-agent                       │   │
│  │  ─────────────────────────────────────────────────  │   │
│  │                                                     │   │
│  │   ┌──────────────┐    ┌──────────────────────────┐ │   │
│  │   │ VsockServer  │◀──▶│   CommandExecutor        │ │   │
│  │   │ (port 52)    │    │ ──────────────────────── │ │   │
│  │   └──────────────┘    │ • exec(cmd)              │ │   │
│  │                       │ • exec_code(lang, code)  │ │   │
│  │                       │ • read_file(path)        │ │   │
│  │                       │ • write_file(path, data) │ │   │
│  │                       │ • list_dir(path)         │ │   │
│  │                       │ • ping()                 │ │   │
│  │                       └──────────────────────────┘ │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Protocol

The agent communicates over vsock using a simple JSON protocol:

```
Request:
┌──────────┬────────────────────────────────────────┐
│ 4 bytes  │           JSON payload                 │
│ (length) │                                        │
└──────────┴────────────────────────────────────────┘

Response:
┌──────────┬────────────────────────────────────────┐
│ 4 bytes  │           JSON payload                 │
│ (length) │                                        │
└──────────┴────────────────────────────────────────┘
```

### Command Types

| Command      | Request                                    | Response                                          |
| ------------ | ------------------------------------------ | ------------------------------------------------- |
| `ping`       | `{}`                                       | `{"ok": true}`                                    |
| `exec`       | `{"cmd": "ls -la"}`                        | `{"exit_code": 0, "stdout": "...", "stderr": ""}` |
| `exec_code`  | `{"lang": "python", "code": "print(1+1)"}` | `{"exit_code": 0, "stdout": "2\n", "stderr": ""}` |
| `read_file`  | `{"path": "/etc/os-release"}`              | `{"content": "..."}`                              |
| `write_file` | `{"path": "/tmp/foo", "content": "bar"}`   | `{"ok": true}`                                    |
| `list_dir`   | `{"path": "/home"}`                        | `{"entries": [...]}`                              |

---

## Layer 3: bouvet-core

The orchestration layer that brings VMs and agents together.

### Purpose

Provides high-level `Sandbox` abstraction that encapsulates VM + agent communication.

### Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                        bouvet-core                              │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │                    SandboxManager                         │ │
│  │  ────────────────────────────────────────────────────────│ │
│  │  • create_sandbox() → SandboxId                          │ │
│  │  • get_sandbox(id) → Option<&Sandbox>                    │ │
│  │  • destroy_sandbox(id) → Result<()>                      │ │
│  │  • destroy_all() → Result<()>                            │ │
│  │                                                          │ │
│  │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐        │ │
│  │  │  Sandbox A  │ │  Sandbox B  │ │  Sandbox C  │        │ │
│  │  └─────────────┘ └─────────────┘ └─────────────┘        │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                                │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │                     SandboxPool                           │ │
│  │  ────────────────────────────────────────────────────────│ │
│  │  • Maintains N warm sandboxes                            │ │
│  │  • Background filler task                                │ │
│  │  • acquire() returns sandbox in ~150ms                   │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                                │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │                      Sandbox                              │ │
│  │  ────────────────────────────────────────────────────────│ │
│  │  • execute(cmd) → ExecResult                             │ │
│  │  • execute_code(lang, code) → ExecResult                 │ │
│  │  • read_file(path) → String                              │ │
│  │  • write_file(path, content) → ()                        │ │
│  │  • list_dir(path) → Vec<FileEntry>                       │ │
│  │  • is_healthy() → bool                                   │ │
│  │  • destroy() → ()                                        │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

### Sandbox Lifecycle

```
┌─────────┐     create()      ┌─────────┐
│ (none)  │──────────────────▶│Creating │
└─────────┘                   └────┬────┘
                                   │
                                   │ VM boots + agent connects
                                   ▼
                              ┌─────────┐
                              │  Ready  │◀───────────────┐
                              └────┬────┘                │
                                   │                     │
                     execute(), read_file(), etc.        │ commands
                                   │                     │
                                   └─────────────────────┘
                                   │
                                   │ destroy()
                                   ▼
                              ┌───────────┐
                              │ Destroyed │
                              └───────────┘
```

### Warm Pool

The pool pre-boots sandboxes for instant allocation:

```
┌─────────────────────────────────────────────────────────────┐
│                       SandboxPool                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   Pool Queue (min_size = 3)                                 │
│   ┌─────────┐ ┌─────────┐ ┌─────────┐                      │
│   │ Sandbox │ │ Sandbox │ │ Sandbox │ ◀── warm, ready      │
│   │  (warm) │ │  (warm) │ │  (warm) │                      │
│   └─────────┘ └─────────┘ └─────────┘                      │
│        ▲                                                    │
│        │                                                    │
│   ┌────┴─────────────────────────────────────────────────┐ │
│   │              Background Filler Task                   │ │
│   │  • Monitors pool size                                 │ │
│   │  • Boots new sandboxes when size < min_size           │ │
│   │  • Limits concurrent boots to max_boots               │ │
│   └──────────────────────────────────────────────────────┘ │
│                                                             │
│   acquire() ─────▶ Returns warm sandbox instantly (~150ms) │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## Layer 4: bouvet-mcp

The MCP server that exposes sandboxes to AI agents.

### Purpose

Translates MCP tool calls into sandbox operations.

### Architecture

```
┌────────────────────────────────────────────────────────────────┐
│                         bouvet-mcp                              │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │                    BouvetServer                           │ │
│  │  ────────────────────────────────────────────────────────│ │
│  │  • Implements rmcp::ServerHandler                        │ │
│  │  • Owns SandboxManager                                   │ │
│  │  • Routes tool calls to sandbox operations               │ │
│  └──────────────────────────────────────────────────────────┘ │
│                         │                                      │
│         ┌───────────────┴───────────────┐                     │
│         ▼                               ▼                     │
│  ┌──────────────┐               ┌──────────────┐              │
│  │    Stdio     │               │     HTTP     │              │
│  │  Transport   │               │  Transport   │              │
│  │ ──────────── │               │ ──────────── │              │
│  │ stdin/stdout │               │ :8080/mcp    │              │
│  │ (local AI)   │               │ (remote AI)  │              │
│  └──────────────┘               └──────────────┘              │
│                                                                │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │                      MCP Tools                            │ │
│  │  ────────────────────────────────────────────────────────│ │
│  │  create_sandbox    destroy_sandbox    list_sandboxes     │ │
│  │  execute_code      run_command                           │ │
│  │  read_file         write_file         list_directory     │ │
│  └──────────────────────────────────────────────────────────┘ │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

### Request Flow

```
┌─────────┐  JSON-RPC   ┌─────────────┐  tool call  ┌─────────────┐
│   AI    │────────────▶│ BouvetServer │────────────▶│   Sandbox   │
│  Agent  │             │             │              │  Manager    │
└─────────┘             └─────────────┘              └──────┬──────┘
                                                           │
                                                           ▼
                                                    ┌─────────────┐
                                                    │   Sandbox   │
                                                    │ (VM+Agent)  │
                                                    └─────────────┘
```

---

## Communication: Host ↔ Guest

### Why vsock?

| Transport | Pros                            | Cons                    |
| --------- | ------------------------------- | ----------------------- |
| **vsock** | Fast, no network config, secure | Linux-only              |
| TCP/IP    | Universal                       | Requires network setup  |
| Serial    | Simple                          | Slow, limited bandwidth |

Bouvet uses **vsock** (AF_VSOCK) — a socket family for VM ↔ hypervisor communication.

### vsock Flow

```
┌──────────────────────────────────────────────────────────────────┐
│                            Host                                   │
│                                                                  │
│   ┌──────────────────┐                                          │
│   │   AgentClient    │                                          │
│   │  (bouvet-core)   │                                          │
│   └────────┬─────────┘                                          │
│            │                                                     │
│            │ connect(/tmp/bouvet/vm-xxx/vsock.sock)             │
│            ▼                                                     │
│   ┌──────────────────┐                                          │
│   │   UDS Socket     │◀────────────┐                            │
│   │ (Unix Domain)    │             │                            │
│   └──────────────────┘             │                            │
│                                    │                            │
└────────────────────────────────────┼────────────────────────────┘
                                     │ Firecracker proxy
                                     │
┌────────────────────────────────────┼────────────────────────────┐
│                           microVM  │                             │
│                                    │                            │
│   ┌──────────────────┐             │                            │
│   │   VsockServer    │◀────────────┘                            │
│   │ (bouvet-agent)   │                                          │
│   │   CID=3, port=52 │                                          │
│   └──────────────────┘                                          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

---

## Security Model

### Isolation Layers

```
┌────────────────────────────────────────────────────────────────┐
│                         Host OS                                 │
├────────────────────────────────────────────────────────────────┤
│                                                                │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                     Firecracker                          │  │
│  │  • seccomp filters (minimal syscalls)                   │  │
│  │  • jailer (chroot, cgroups, namespaces)                 │  │
│  └─────────────────────────────────────────────────────────┘  │
│                              │                                 │
│                              │ KVM                             │
│                              ▼                                 │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                       microVM                            │  │
│  │  ┌───────────────────────────────────────────────────┐  │  │
│  │  │                  Guest Kernel                      │  │  │
│  │  │  • Separate kernel (not shared with host)         │  │  │
│  │  │  • No access to host filesystem                   │  │  │
│  │  │  • No access to host network                      │  │  │
│  │  │  • Memory isolated by hypervisor                  │  │  │
│  │  └───────────────────────────────────────────────────┘  │  │
│  │                                                          │  │
│  │  ┌───────────────────────────────────────────────────┐  │  │
│  │  │                  bouvet-agent                      │  │  │
│  │  │  • Runs as regular process                        │  │  │
│  │  │  • Only vsock access to host                      │  │  │
│  │  │  • Executes user code in subprocess               │  │  │
│  │  └───────────────────────────────────────────────────┘  │  │
│  │                                                          │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

### What's Isolated

| Resource   | Isolation Level                          |
| ---------- | ---------------------------------------- |
| Memory     | Hardware (KVM) — separate physical pages |
| CPU        | Hardware (KVM) — VMCS isolation          |
| Filesystem | Separate ext4 image (copy-on-write)      |
| Network    | No network by default                    |
| Processes  | Separate PID namespace                   |
| Users      | Separate user namespace                  |

### Attack Surface

| Vector              | Mitigation                                   |
| ------------------- | -------------------------------------------- |
| Guest → Host escape | KVM + Firecracker seccomp                    |
| Agent vulnerability | Minimal attack surface, no network           |
| Resource exhaustion | Memory/CPU limits, timeout enforcement       |
| Persistent access   | Sandboxes are ephemeral, destroyed after use |

---

## Data Flow: execute_code

Complete flow for running Python code:

```
┌─────────┐
│   AI    │ 1. {"method": "tools/call", "params": {"name": "execute_code", ...}}
│  Agent  │─────────────────────────────────────────────────────────────────────┐
└─────────┘                                                                     │
                                                                                │
┌───────────────────────────────────────────────────────────────────────────────┼───┐
│                              bouvet-mcp                                       │   │
│                                                                               ▼   │
│  2. Parse tool call                                                               │
│  3. Look up sandbox by ID                                                         │
│  4. Call sandbox.execute_code("python", code)                                     │
└───────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────────┐
│                              bouvet-core                                          │
│                                                                                   │
│  5. Acquire lock on AgentClient                                                   │
│  6. Send request over vsock                                                       │
└───────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        │ vsock
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────────┐
│                              bouvet-agent (guest)                                 │
│                                                                                   │
│  7. Receive request                                                               │
│  8. Write code to /tmp/script.py                                                  │
│  9. Execute: python3 /tmp/script.py                                               │
│  10. Capture stdout, stderr, exit code                                            │
│  11. Send response                                                                │
└───────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        │ vsock
                                        ▼
┌───────────────────────────────────────────────────────────────────────────────────┐
│                              bouvet-mcp                                           │
│                                                                                   │
│  12. Format as MCP CallToolResult                                                 │
│  13. Return JSON-RPC response                                                     │
└───────────────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
┌─────────┐
│   AI    │ {"result": {"content": [{"type": "text", "text": "..."}]}}
│  Agent  │◀────────────────────────────────────────────────────────────────────────
└─────────┘
```

---

## Performance Characteristics

| Metric              | Cold Start | Warm Pool |
| ------------------- | ---------- | --------- |
| Sandbox creation    | ~500ms     | ~150ms    |
| Command execution   | ~10ms      | ~10ms     |
| File read/write     | ~5ms       | ~5ms      |
| Sandbox destruction | ~50ms      | ~50ms     |

### Bottlenecks

| Operation           | Bottleneck             | Mitigation         |
| ------------------- | ---------------------- | ------------------ |
| VM boot             | Kernel init            | Warm pool          |
| Agent connect       | vsock handshake        | Pre-connected pool |
| Code execution      | Process spawn in guest | —                  |
| Large file transfer | vsock bandwidth        | Chunked transfer   |

---

## Dependencies

### Core

| Crate       | Purpose                     |
| ----------- | --------------------------- |
| `tokio`     | Async runtime               |
| `firepilot` | Firecracker SDK             |
| `rmcp`      | MCP protocol implementation |

### Serialization

| Crate        | Purpose                        |
| ------------ | ------------------------------ |
| `serde`      | Serialization framework        |
| `serde_json` | JSON encoding                  |
| `schemars`   | JSON Schema generation for MCP |

### Networking

| Crate        | Purpose                         |
| ------------ | ------------------------------- |
| `axum`       | HTTP server for MCP             |
| `tower-http` | HTTP middleware (CORS, tracing) |

---

## Future Considerations

### Planned

- **Snapshots**: Faster restore from memory snapshots
- **ARM support**: a1.metal instances for cost reduction
- **Network access**: Optional controlled internet access

### Not Planned

- **Container fallback**: Containers don't provide sufficient isolation
- **Windows VMs**: Focus on Linux-based development environments
- **Persistent storage**: Sandboxes are intentionally ephemeral
