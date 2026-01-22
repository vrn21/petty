# VM Layer Deep Dive

> **Layer**: 2 (`bouvet-vm`)  
> **Related Code**: [`crates/bouvet-vm/src/`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/)

---

## Overview

The `bouvet-vm` crate is the MicroVM management layer for the Bouvet agentic sandbox. It provides a high-level abstraction over the [firepilot](https://crates.io/crates/firepilot) SDK, which in turn interfaces with the Firecracker hypervisor.

This crate is responsible for:
- **VM Lifecycle Management**: Create, start, stop, kill, and destroy MicroVMs
- **Drive Configuration**: Root filesystem and additional block devices
- **Network Configuration**: TAP device support for guest networking
- **vsock Support**: Guest-host communication channel for agent RPC
- **Builder Pattern**: Ergonomic configuration via `VmBuilder`

---

## Architecture

```
┌──────────────────────────────────────────────────┐
│                   bouvet-vm                       │
├──────────────────────────────────────────────────┤
│  VmBuilder → MachineConfig → VirtualMachine      │
└──────────────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────────────┐
│              firepilot SDK                        │
│      (Firecracker API client library)            │
└──────────────────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────────────────┐
│              Firecracker                         │
│         (KVM-based hypervisor)                   │
└──────────────────────────────────────────────────┘
```

### Module Structure

```
bouvet-vm/src/
├── lib.rs           # Public API exports
├── builder.rs       # VmBuilder fluent configuration API
├── config.rs        # Configuration types (MachineConfig, VsockConfig, etc.)
├── machine.rs       # VirtualMachine - running VM instance
├── machine_config.rs # Direct Firecracker API for vCPU/memory config
├── vsock.rs         # Direct Firecracker API for vsock config
└── error.rs         # VmError type definitions
```

---

## Key Types

| Type | File | Purpose |
|------|------|---------|
| [`VmBuilder`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/builder.rs) | builder.rs | Fluent configuration API for creating VMs |
| [`MachineConfig`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/config.rs) | config.rs | Validated VM configuration (vCPU, memory, drives, etc.) |
| [`VirtualMachine`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/machine.rs) | machine.rs | Running VM instance with lifecycle methods |
| [`VsockConfig`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/config.rs#L150-L155) | config.rs | vsock socket configuration (CID, UDS path) |
| [`VmState`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/machine.rs#L30-L41) | machine.rs | State enum: Creating, Running, Paused, Stopped |
| [`DriveConfig`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/config.rs#L105-L114) | config.rs | Block device configuration |
| [`NetworkConfig`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/config.rs#L129-L136) | config.rs | Network interface configuration |
| [`VmError`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/error.rs) | error.rs | Error types for VM operations |

---

## VmBuilder API

The `VmBuilder` provides a fluent, ergonomic API for configuring and creating VMs:

```rust
use bouvet_vm::VmBuilder;

let vm = VmBuilder::new()
    .vcpus(4)                           // 1-32 vCPUs
    .memory_mib(512)                    // 128-32768 MiB
    .kernel("/path/to/vmlinux")         // Kernel image
    .rootfs("/path/to/rootfs.ext4")     // Root filesystem
    .with_vsock(5)                      // Guest CID for vsock
    .firecracker_path("/usr/local/bin/firecracker")
    .chroot_path("/tmp/bouvet")
    .build()
    .await?;
```

### Builder Methods

| Method | Description |
|--------|-------------|
| `vcpus(count: u8)` | Set vCPU count (1-32) |
| `memory_mib(mib: u32)` | Set memory in MiB (128-32768) |
| `kernel(path)` | Set path to vmlinux kernel |
| `boot_args(args)` | Set kernel boot arguments |
| `rootfs(path)` | Set path to root filesystem image |
| `rootfs_read_only()` | Make root drive read-only |
| `with_drive(id, path)` | Add an extra block device |
| `with_network(tap_dev)` | Add network interface with TAP device |
| `with_vsock(cid)` | Configure vsock with guest CID |
| `firecracker_path(path)` | Set Firecracker binary location |
| `chroot_path(path)` | Set working directory for VM state |
| `build()` | Create and start the VirtualMachine |
| `build_config()` | Return config without creating VM (for testing) |

---

## MachineConfig

The `MachineConfig` struct holds all configuration needed to create a VM:

```rust
pub struct MachineConfig {
    pub vcpu_count: u8,           // 1-32 (default: 2)
    pub memory_mib: u32,          // 128-32768 MiB (default: 256)
    pub kernel_path: PathBuf,     // Path to vmlinux
    pub boot_args: String,        // Kernel command line
    pub root_drive: DriveConfig,  // Root filesystem drive
    pub extra_drives: Vec<DriveConfig>,
    pub network: Option<NetworkConfig>,
    pub vsock: Option<VsockConfig>,
    pub firecracker_path: PathBuf,
    pub chroot_path: PathBuf,
}
```

### Default Configuration

| Field | Default Value |
|-------|---------------|
| `vcpu_count` | 2 |
| `memory_mib` | 256 |
| `kernel_path` | `/var/lib/bouvet/kernel/vmlinux` |
| `boot_args` | `console=ttyS0 reboot=k panic=1 pci=off` |
| `rootfs` | `/var/lib/bouvet/images/debian.ext4` |
| `firecracker_path` | `/usr/local/bin/firecracker` |
| `chroot_path` | `/tmp/bouvet` |

### Validation Rules

The `validate()` method enforces:
- vCPU count: 1-32 (Firecracker limit)
- Memory: 128 MiB - 32 GiB
- vsock CID: Must be > 2 (0, 1, 2 are reserved)
- Drive IDs: Must be unique across root and extra drives

---

## VM Lifecycle

### State Machine

```
     ┌──────────────────────────────────────────────────────┐
     │                                                      │
     │                     VmBuilder::build()               │
     │                           │                          │
     │                           ▼                          │
     │                  ┌────────────────┐                  │
     │                  │    Creating    │                  │
     │                  └───────┬────────┘                  │
     │                          │                           │
     │                          │ (firepilot create + start)|
     │                          ▼                           │
     │                  ┌────────────────┐                  │
     │          ┌──────▶│    Running     │◀──────┐          │
     │          │       └───────┬────────┘       │          │
     │          │               │                │          │
     │    start()           stop()           start()        │
     │          │               │                │          │
     │          │               ▼                │          │
     │          │       ┌────────────────┐       │          │
     │          └───────│    Stopped     │───────┘          │
     │                  └───────┬────────┘                  │
     │                          │                           │
     │                      destroy()                       │
     │                          │                           │
     │                          ▼                           │
     │                    [Resources cleaned up]            │
     └──────────────────────────────────────────────────────┘
```

### Creation Flow

The `VirtualMachine::create()` method performs the following sequence:

```
1. Validate configuration
       │
       ▼
2. Build firepilot Configuration
   ├── Kernel (KernelBuilder)
   ├── Root drive (DriveBuilder)
   ├── Extra drives (DriveBuilder)
   ├── Network interfaces (NetworkInterfaceBuilder)
   └── Executor (FirecrackerExecutorBuilder)
       │
       ▼
3. Create Machine (machine.create())
   └── Spawns Firecracker process
   └── Creates API socket at: {chroot_path}/{vm_id}/firecracker.socket
       │
       ▼
4. Configure machine resources via direct API
   └── PUT /machine-config (vcpu, memory)
       │
       ▼
5. Configure vsock via direct API (if enabled)
   └── PUT /vsock (cid, uds_path)
       │
       ▼
6. Start VM (machine.start())
   └── PUT /actions { action_type: "InstanceStart" }
       │
       ▼
7. Return VirtualMachine { state: Running }
```

### VirtualMachine Methods

| Method | Description |
|--------|-------------|
| `id()` | Get the unique UUID of this VM |
| `state()` | Get current VmState |
| `config()` | Get the MachineConfig reference |
| `socket_path()` | Get Firecracker API socket path |
| `vsock_uds_path()` | Get vsock UDS path (if configured) |
| `vsock_cid()` | Get vsock guest CID (if configured) |
| `start()` | Start a stopped/paused VM |
| `stop()` | Gracefully stop the VM |
| `kill()` | Force kill the VM immediately |
| `destroy()` | Stop and clean up all resources |

---

## Firecracker API Integration

The `bouvet-vm` crate interfaces with Firecracker via two mechanisms:

### 1. firepilot SDK (High-Level)

Used for:
- Spawning the Firecracker process
- Configuring kernel, drives, and network
- Starting/stopping/killing the VM

### 2. Direct HTTP API (Low-Level)

Since firepilot doesn't expose all Firecracker features, `bouvet-vm` makes direct HTTP calls to the Firecracker API socket for:

- **Machine Configuration** ([`machine_config.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/machine_config.rs)): `PUT /machine-config`
- **vsock Configuration** ([`vsock.rs`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/vsock.rs)): `PUT /vsock`

### Socket Paths

| Path | Purpose |
|------|---------|
| `{chroot_path}/{vm_id}/firecracker.socket` | Firecracker API socket |
| `{chroot_path}/{vm_id}/v.sock` | vsock UDS (host side) |

### Configuration Sequence

```
Firecracker API Timeline:

    │ machine.create()
    ▼
┌─────────────────────────────────────────────────────────────┐
│  Firecracker process spawned                                │
│  API socket ready at: /tmp/bouvet/{vm-id}/firecracker.socket│
└─────────────────────────────────────────────────────────────┘
    │
    │ PUT /machine-config { "vcpu_count": 2, "mem_size_mib": 256 }
    ▼
    │
    │ PUT /vsock { "guest_cid": 5, "uds_path": "/tmp/bouvet/{vm-id}/v.sock" }
    ▼
    │
    │ PUT /actions { "action_type": "InstanceStart" }
    ▼
┌─────────────────────────────────────────────────────────────┐
│  VM is now running                                           │
└─────────────────────────────────────────────────────────────┘
```

---

## vsock Configuration

vsock enables zero-config, low-latency communication between host and guest.

### VsockConfig Structure

```rust
pub struct VsockConfig {
    pub guest_cid: u32,      // Context ID (must be > 2)
    pub uds_path: PathBuf,   // Host-side Unix Domain Socket
}
```

### Helper Method

```rust
impl VsockConfig {
    pub fn for_vm(cid: u32, chroot_path: &Path, vm_id: &str) -> Self {
        Self {
            guest_cid: cid,
            uds_path: chroot_path.join(vm_id).join("v.sock"),
        }
    }
}
```

### CID Reservation

| CID | Reserved For |
|-----|--------------|
| 0 | Hypervisor |
| 1 | Loopback |
| 2 | Host |
| 3+ | Guest VMs (assigned by bouvet) |

---

## Error Types

The [`VmError`](file:///Users/vrn21/Developer/rust/petty/crates/bouvet-vm/src/error.rs) enum covers all failure modes:

| Variant | Description |
|---------|-------------|
| `Create(String)` | Failed to create/spawn the VM |
| `Start(String)` | Failed to start the VM |
| `Stop(String)` | Failed to stop the VM |
| `InvalidState { expected, actual }` | Invalid state transition |
| `Config(String)` | Configuration validation error |
| `Firepilot(String)` | firepilot/Firecracker API error |
| `Io(std::io::Error)` | I/O operation failed |
| `Timeout(Duration)` | Operation timed out |

### Error Propagation

```
┌────────────────────────────────────────────────┐
│ VmError (bouvet-vm)                            │
└────────────────────┬───────────────────────────┘
                     │ wrapped by
┌────────────────────▼───────────────────────────┐
│ CoreError::Vm(VmError) (bouvet-core)           │
└────────────────────┬───────────────────────────┘
                     │ mapped to
┌────────────────────▼───────────────────────────┐
│ MCP error response (bouvet-mcp)                │
└────────────────────────────────────────────────┘
```

---

## Usage Example

### Complete VM Creation

```rust
use bouvet_vm::{VmBuilder, VmState};

async fn create_sandbox_vm() -> bouvet_vm::Result<()> {
    // Create and start VM
    let vm = VmBuilder::new()
        .vcpus(2)
        .memory_mib(256)
        .kernel("/var/lib/bouvet/vmlinux")
        .rootfs("/var/lib/bouvet/debian.ext4")
        .with_vsock(3)  // Guest CID
        .chroot_path("/tmp/bouvet")
        .build()
        .await?;

    // VM is now running
    assert_eq!(vm.state(), VmState::Running);
    
    // Get vsock path for agent communication
    if let Some(vsock_path) = vm.vsock_uds_path() {
        println!("Agent available at: {:?}", vsock_path);
    }

    // Cleanup
    vm.destroy().await?;
    Ok(())
}
```

### Using create_with_id

When creating VMs for sandboxes, use `create_with_id` to match sandbox IDs:

```rust
use bouvet_vm::VirtualMachine;
use uuid::Uuid;

let sandbox_id = Uuid::new_v4();
let vm = VirtualMachine::create_with_id(sandbox_id, config).await?;

assert_eq!(vm.id(), sandbox_id);
```

---

## Relationship to Other Layers

```
Layer 5 (MCP)    │ bouvet-mcp   │ Uses SandboxPool → SandboxManager
                 │              │         │
                 │              │         ▼
Layer 4 (Core)   │ bouvet-core  │ Sandbox wraps VirtualMachine
                 │              │         │
                 │              │         ▼
Layer 2 (VM)     │ bouvet-vm    │ VirtualMachine manages Firecracker
                 │              │         │
                 │              │         ▼
Layer 1 (Hyper)  │ Firecracker  │ Runs the MicroVM
```

The `bouvet-vm` crate is a **pure wrapper** — it has no knowledge of:
- Sandboxes or agents (Layer 4)
- MCP protocol or tools (Layer 5)
- What runs inside the guest

This separation of concerns allows `bouvet-vm` to be used independently for any Firecracker-based workload.

---

## See Also

- [vsock Communication](./VSOCK_COMMUNICATION.md) — Host-guest communication details
- [Sandbox Lifecycle](./SANDBOX_LIFECYCLE.md) — How bouvet-core uses bouvet-vm
- [Agent Protocol](./AGENT_PROTOCOL.md) — RPC protocol over vsock
