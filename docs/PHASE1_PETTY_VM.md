# Phase 1: petty-vm Module Design

> Implementation guide for the MicroVM management layer using firepilot.

---

## Objective

Implement a complete and functional `petty-vm` crate that wraps firepilot to provide:

- MicroVM lifecycle management (create, start, stop, destroy)
- Drive and network configuration
- vsock communication channel for guest agent

---

## Current State

The crate structure exists with stub implementations:

```
crates/petty-vm/
├── Cargo.toml
└── src/
    ├── lib.rs      # Module exports
    ├── config.rs   # Configuration types (complete)
    ├── error.rs    # Error types (complete)
    └── machine.rs  # VirtualMachine (stub - needs implementation)
```

---

## Implementation Tasks

### Task 1: Implement VirtualMachine::create()

**File**: [machine.rs](file:///Users/vrn21/Developer/rust/petty/crates/petty-vm/src/machine.rs)

Replace the stub `create()` method with actual firepilot integration:

```rust
use firepilot::{Machine, MachineConfig as FpConfig};
use firepilot_models::{BootSource, Drive, NetworkInterface};

pub struct VirtualMachine {
    id: Uuid,
    config: MachineConfig,
    state: VmState,
    machine: Machine,  // ADD THIS FIELD
}

impl VirtualMachine {
    pub async fn create(config: MachineConfig) -> Result<Self> {
        let id = Uuid::new_v4();

        // 1. Build firepilot config
        let fp_config = FpConfig::builder()
            .vcpu_count(config.vcpu_count as i64)
            .mem_size_mib(config.memory_mib as i64)
            .boot_source(BootSource {
                kernel_image_path: config.kernel_path.to_string_lossy().to_string(),
                boot_args: Some(config.boot_args.clone()),
                ..Default::default()
            })
            .drives(vec![Drive {
                drive_id: config.root_drive.drive_id.clone(),
                path_on_host: config.root_drive.path_on_host.to_string_lossy().to_string(),
                is_root_device: config.root_drive.is_root_device,
                is_read_only: config.root_drive.is_read_only,
                ..Default::default()
            }])
            .build();

        // 2. Create firepilot Machine
        let machine = Machine::new(fp_config)
            .await
            .map_err(|e| VmError::Create(e.to_string()))?;

        // 3. Start the VM
        machine.start()
            .await
            .map_err(|e| VmError::Start(e.to_string()))?;

        Ok(Self {
            id,
            config,
            state: VmState::Running,
            machine,
        })
    }
}
```

### Task 2: Implement Remaining Lifecycle Methods

Update `start()`, `stop()`, `kill()`, `destroy()` to call firepilot:

```rust
pub async fn stop(&mut self) -> Result<()> {
    self.machine.shutdown()
        .await
        .map_err(|e| VmError::Stop(e.to_string()))?;
    self.state = VmState::Stopped;
    Ok(())
}

pub async fn destroy(self) -> Result<()> {
    // Machine dropped automatically cleans up
    drop(self.machine);
    Ok(())
}
```

### Task 3: Add Network Configuration

**File**: [machine.rs](file:///Users/vrn21/Developer/rust/petty/crates/petty-vm/src/machine.rs)

Add network interface when `config.network` is Some:

```rust
// In create(), before .build():
let mut builder = FpConfig::builder()
    // ... existing config ...

if let Some(net) = &config.network {
    builder = builder.network_interfaces(vec![NetworkInterface {
        iface_id: net.iface_id.clone(),
        host_dev_name: net.host_dev_name.clone(),
        guest_mac: net.guest_mac.clone(),
        ..Default::default()
    }]);
}
```

### Task 4: Add vsock Support

**New file**: `src/vsock.rs`

Add vsock configuration for guest communication:

```rust
use firepilot_models::Vsock;

pub struct VsockConfig {
    /// Guest CID (Context ID), must be > 2
    pub guest_cid: u32,
    /// Path to vsock UDS on host
    pub uds_path: PathBuf,
}

impl Default for VsockConfig {
    fn default() -> Self {
        Self {
            guest_cid: 3,
            uds_path: PathBuf::from("/tmp/petty-vsock.sock"),
        }
    }
}
```

Update `MachineConfig` in `config.rs`:

```rust
pub struct MachineConfig {
    // ... existing fields ...
    /// vsock configuration for guest communication
    pub vsock: Option<VsockConfig>,
}
```

Add to Machine creation:

```rust
if let Some(vsock) = &config.vsock {
    builder = builder.vsock(Vsock {
        guest_cid: vsock.guest_cid as i64,
        uds_path: vsock.uds_path.to_string_lossy().to_string(),
        ..Default::default()
    });
}
```

### Task 5: Add Builder Pattern

**New file**: `src/builder.rs`

Create fluent builder for easier configuration:

```rust
pub struct VmBuilder {
    config: MachineConfig,
}

impl VmBuilder {
    pub fn new() -> Self {
        Self { config: MachineConfig::default() }
    }

    pub fn vcpus(mut self, count: u8) -> Self {
        self.config.vcpu_count = count;
        self
    }

    pub fn memory_mib(mut self, mib: u32) -> Self {
        self.config.memory_mib = mib;
        self
    }

    pub fn kernel(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.kernel_path = path.into();
        self
    }

    pub fn rootfs(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.root_drive.path_on_host = path.into();
        self
    }

    pub fn with_network(mut self, host_dev: &str) -> Self {
        self.config.network = Some(NetworkConfig {
            host_dev_name: host_dev.into(),
            ..Default::default()
        });
        self
    }

    pub fn with_vsock(mut self, cid: u32) -> Self {
        self.config.vsock = Some(VsockConfig {
            guest_cid: cid,
            ..Default::default()
        });
        self
    }

    pub async fn build(self) -> Result<VirtualMachine> {
        VirtualMachine::create(self.config).await
    }
}
```

---

## Testing Requirements

### Unit Tests (no Firecracker needed)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MachineConfig::default();
        assert_eq!(config.vcpu_count, 2);
        assert_eq!(config.memory_mib, 256);
    }

    #[test]
    fn test_builder() {
        let config = VmBuilder::new()
            .vcpus(4)
            .memory_mib(512)
            .build_config();
        assert_eq!(config.vcpu_count, 4);
    }
}
```

### Integration Tests (requires Linux + KVM)

Create `tests/integration.rs`:

```rust
//! Integration tests - require Firecracker binary and /dev/kvm

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored
async fn test_vm_lifecycle() {
    let vm = VmBuilder::new()
        .kernel("/path/to/vmlinux")
        .rootfs("/path/to/rootfs.ext4")
        .build()
        .await
        .expect("Failed to create VM");

    assert_eq!(vm.state(), VmState::Running);

    vm.destroy().await.expect("Failed to destroy VM");
}
```

---

## Dependencies

Ensure these are in `Cargo.toml`:

```toml
[dependencies]
firepilot = "1.2"
tokio = { version = "1.40", features = ["full"] }
uuid = { version = "1.10", features = ["v4"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "2.0"
tracing = "0.1"
```

---

## Acceptance Criteria

- [ ] `VirtualMachine::create()` boots a real Firecracker microVM
- [ ] `VirtualMachine::stop()` gracefully shuts down the VM
- [ ] `VirtualMachine::destroy()` cleans up all resources
- [ ] Network interface configurable but optional
- [ ] vsock channel configurable for guest communication
- [ ] Builder pattern for ergonomic configuration
- [ ] All public types documented with doc comments
- [ ] Unit tests pass without Firecracker
- [ ] Integration tests pass on Linux with KVM

---

## Environment Setup (for testing)

```bash
# Download Firecracker (Linux only)
FC_VERSION=1.5.1
curl -L -o firecracker \
  "https://github.com/firecracker-microvm/firecracker/releases/download/v${FC_VERSION}/firecracker-v${FC_VERSION}-x86_64"
chmod +x firecracker
sudo mv firecracker /usr/local/bin/

# Check KVM access
ls -la /dev/kvm
# If permission denied: sudo chmod 666 /dev/kvm

# Download test kernel and rootfs
curl -L -o vmlinux.bin \
  "https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/x86_64/kernels/vmlinux.bin"
curl -L -o rootfs.ext4 \
  "https://s3.amazonaws.com/spec.ccfc.min/ci-artifacts/disks/x86_64/ubuntu-22.04.ext4"
```

---

## Reference Links

- [firepilot docs](https://docs.rs/firepilot)
- [firepilot examples](https://github.com/nicholasguan/firepilot/tree/main/examples)
- [Firecracker getting started](https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md)
