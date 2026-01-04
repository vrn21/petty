# bouvet-vm

MicroVM management for Bouvet agentic sandbox using Firecracker.

## Types

- VmBuilder: fluent config builder
- VirtualMachine: running VM with lifecycle methods
- MachineConfig: full config struct
- VsockConfig: guest-host communication

## VmBuilder methods

vcpus(n), memory_mib(n), kernel(path), rootfs(path), with_vsock(cid), firecracker_path(path), chroot_path(path), build().await

## VirtualMachine methods

id(), state(), vsock_uds_path(), stop().await, kill().await, destroy().await

## Communication

vsock: host connects via Unix socket at vsock_uds_path(), guest listens on CID

## Requirements

Linux + /dev/kvm + Firecracker binary + kernel + rootfs image

---

## Implementation Guide

### 1. Add dependency

In Cargo.toml: bouvet-vm = { path = "../crates/bouvet-vm" }

### 2. Create VM

```
let vm = VmBuilder::new()
    .kernel("/var/lib/bouvet/kernel/vmlinux")
    .rootfs("/var/lib/bouvet/images/debian.ext4")
    .with_vsock(3)
    .build()
    .await?;
```

### 3. Get vsock path for agent communication

```
let uds = vm.vsock_uds_path().unwrap();
// Connect to this Unix socket to talk to guest agent
```

### 4. Cleanup

```
vm.destroy().await?;
```

### Common configurations

Minimal VM:

```
VmBuilder::new()
    .kernel(path)
    .rootfs(path)
    .build().await
```

With resources:

```
VmBuilder::new()
    .vcpus(4)
    .memory_mib(512)
    .kernel(path)
    .rootfs(path)
    .with_vsock(3)
    .build().await
```

Custom paths:

```
VmBuilder::new()
    .kernel(path)
    .rootfs(path)
    .firecracker_path("/usr/bin/firecracker")
    .chroot_path("/tmp/my-vms")
    .build().await
```

### Error handling

All async methods return Result<T, VmError>. Errors: Create, Start, Stop, Config, InvalidState, Firepilot, Io, Timeout

### State machine

Creating -> Running -> Stopped
Methods: start() resumes stopped VM, stop() graceful, kill() force, destroy() cleanup
