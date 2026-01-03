# Phase 7: DevOps & AWS Deployment

> Scaling Petty to bare-metal EC2 instances for production workload.

---

## Purpose

Petty requires KVM (Hardware Virtualization), which isn't available on standard nested VMs.
Phase 7 focuses on:

1.  **Terraform**: Automated provisioning of AWS Bare Metal (`.metal`) instances.
2.  **Petty Daemon**: Wrapping the Petty stack into a single, deployable unit.
3.  **Networking**: Automated setup of TAP devices and host NAT.

---

## Infrastructure (Terraform)

### Target: EC2 Bare Metal

- **Instance Types**: `c5.metal`, `m5.metal`, or `i3.metal`.
- **OS**: Ubuntu 22.04 or Debian 12.
- **Resource Provisioning**:
  - Host hardening.
  - KVM kernel module configuration.
  - Firecracker binary installation.

---

## The Petty Daemon

To make deployment simple, the `petty-mcp` and `petty-core` stack should be treated as a single service.

### Components:

1.  **Installation Script**: Installs Firecracker, prepares chroot jail, sets up bridge networking.
2.  **Petty Service**: Systemd unit to keep the MCP server (or future API server) running.
3.  **Image Distribution**: Mechanism to pull `.ext4` rootfs and `vmlinux` images from S3 or Registry.

---

## Implementation Tasks

### Task 1: Terraform Modules

- Define `main.tf` for an EC2 bare-metal instance.
- Configure security groups (SSH, future API ports).
- Use `user_data` to bootstrap the host.

### Task 2: Host Bootstrap Script

- Update `apt` and install `docker`, `git`, `build-essential`.
- Enable KVM: `modprobe kvm`.
- Set `/dev/kvm` permissions for the petty user.
- Install Firecracker binary.

### Task 3: Network Automation

- Create a script to setup `petty0` bridge.
- Add `iptables` rules for NAT (giving VMs internet access).
- Automate TAP device creation/deletion in `petty-vm`.

### Task 4: Rootfs Lifecycle on Host

- Script to download toolchain images from S3/Registry.
- Verify checksums and store in `/var/lib/petty/images`.

### Task 5: Petty Systemd Service

- Wrap `petty-mcp` or a new `petty-daemon` into a background service.
- Ensure it starts after the network bridge and images are ready.

---

## Build Pipeline (CI/CD)

### GitHub Actions:

1.  **Binary Build**: Cross-compile `petty-agent` and `petty-mcp` for Linux.
2.  **Image Build**: Run Phase 5 `Makefile` to generate `.ext4` images.
3.  **Release**: Upload binaries and images to GitHub Releases or S3.

---

## Acceptance Criteria

- [ ] Terraform can provision a functional bare-metal instance.
- [ ] Host is automatically configured with KVM and Networking.
- [ ] Petty service starts up and is ready for MCP connections.
- [ ] VMs inside Petty can access the internet (NAT works).
- [ ] CI/CD automatically produces all required artifacts.

---

## Final Goal

A single `terraform apply` command should result in a fully functional Petty sandbox host ready to serve AI agents.
