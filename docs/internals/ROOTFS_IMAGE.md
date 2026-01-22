# Rootfs Image Architecture

> **Layer**: 1.5 (Between Firecracker and VM — the disk image)  
> **Related Code**: [`images/`](file:///Users/vrn21/Developer/rust/petty/images/) directory, [Makefile](file:///Users/vrn21/Developer/rust/petty/Makefile)

This document describes the ext4 rootfs image that boots inside Firecracker microVMs. The rootfs is a complete Linux filesystem containing the guest operating system, pre-installed runtimes, and the bouvet-agent.

---

## Base Image

### Debian Bookworm Slim

The rootfs is built on **Debian Bookworm (12) slim** as the base:

```dockerfile
FROM debian:bookworm-slim
```

| Property | Value | Rationale |
|----------|-------|-----------|
| Distribution | Debian 12 (Bookworm) | Stable, well-tested, large package ecosystem |
| Variant | `slim` | Reduced size by excluding docs and man pages |
| Architecture | aarch64 or x86_64 | Matches host architecture |

> [!NOTE]
> Debian Bookworm was chosen over Bullseye for newer package versions, particularly Node.js 20.x support and Python 3.11.

### Size Constraints and Optimization

The rootfs image is optimized for minimal disk footprint:

| Optimization | Implementation |
|--------------|----------------|
| Remove documentation | `rm -rf /usr/share/doc /usr/share/man /usr/share/info` |
| Clean apt cache | `apt-get clean && rm -rf /var/cache/apt/*` |
| Truncate logs | `find /var/log -type f -exec truncate -s 0 {} \;` |
| Remove temp files | `rm -rf /tmp/* /var/tmp/*` |
| Strip Rust docs | `rm -rf /usr/local/rustup/toolchains/*/share/doc` |
| npm cache clear | `npm cache clean --force` |

Final image size: **~800MB** (after `resize2fs -M` shrinks to minimum)

### ext4 Filesystem Layout

The rootfs uses the ext4 filesystem format:

```
┌─────────────────────────────────────────────────────┐
│                 debian-devbox.ext4                   │
├─────────────────────────────────────────────────────┤
│  Label: bouvet-rootfs                                │
│  Type:  ext4                                         │
│  Build: mke2fs -t ext4 -d <rootfs_dir>              │
└─────────────────────────────────────────────────────┘
         │
         ├── /bin, /sbin, /usr/bin     # System binaries
         ├── /lib, /lib64              # System libraries  
         ├── /etc                      # Configuration
         ├── /usr/local/bin            # bouvet-agent
         ├── /usr/local/cargo/bin      # Rust toolchain
         └── /root                     # Root home (workdir)
```

Key filesystem entries:

| Path | Purpose |
|------|---------|
| `/etc/fstab` | Contains `/dev/vda / ext4 defaults 0 1` |
| `/etc/hostname` | Set to `bouvet` |
| `/etc/network/interfaces` | Auto DHCP on eth0 |

---

## Pre-installed Runtimes

The devbox image includes multiple language runtimes for code execution:

| Runtime | Version | Binary Path | Package Source |
|---------|---------|-------------|----------------|
| Python | 3.11 | `/usr/bin/python3` | Debian apt |
| Node.js | 20.x | `/usr/bin/node` | NodeSource repository |
| Bash | 5.2 | `/bin/bash` | Debian apt |
| Rust | stable | `/usr/local/cargo/bin/rustc` | rustup installer |

### Python Environment

```dockerfile
RUN apt-get install -y --no-install-recommends \
    python3 \
    python3-pip \
    python3-venv \
    python3-dev
    
# Symlink for convenience
RUN ln -sf /usr/bin/python3 /usr/bin/python
```

### Node.js Environment

```dockerfile
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g npm@latest
```

### Rust Environment

```dockerfile
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain stable --profile minimal \
    && rustup component add clippy rustfmt
```

### Additional Development Tools

| Category | Packages |
|----------|----------|
| Build essentials | `build-essential`, `pkg-config`, `cmake`, `clang` |
| Networking | `curl`, `wget`, `openssh-client`, `ca-certificates` |
| Version control | `git` |
| Utilities | `vim`, `nano`, `less`, `tree`, `htop`, `jq` |
| Debugging | `strace`, `gdb` |
| Compression | `zip`, `unzip`, `xz-utils` |

---

## Agent Installation

### Binary Location

The bouvet-agent binary is installed at:

```
/usr/local/bin/bouvet-agent
```

The agent is cross-compiled for the target architecture using musl libc for maximum compatibility:

```dockerfile
# From Dockerfile.devbox
COPY bouvet-agent /usr/local/bin/bouvet-agent
RUN chmod +x /usr/local/bin/bouvet-agent
```

### Systemd Service

The agent runs as a systemd service defined in [`bouvet-agent.service`](file:///Users/vrn21/Developer/rust/petty/images/bouvet-agent.service):

```ini
[Unit]
Description=Bouvet Guest Agent
After=network.target
After=systemd-modules-load.service

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/bouvet-agent
Restart=on-failure
RestartSec=2
StartLimitIntervalSec=60
StartLimitBurst=5
StandardOutput=journal+console
StandardError=journal+console
Environment="RUST_LOG=debug"
Environment="RUST_BACKTRACE=1"

[Install]
WantedBy=multi-user.target
```

| Setting | Value | Purpose |
|---------|-------|---------|
| Type | `simple` | Agent doesn't fork (stays in foreground) |
| User | `root` | Required for vsock access |
| Restart | `on-failure` | Auto-restart on crashes |
| RestartSec | `2` | 2-second delay between restarts |
| StartLimitBurst | `5` | Max 5 restarts per 60 seconds |

### Service Enablement

The service is enabled via symlink during image build:

```dockerfile
RUN mkdir -p /etc/systemd/system/multi-user.target.wants && \
    ln -sf /etc/systemd/system/bouvet-agent.service \
    /etc/systemd/system/multi-user.target.wants/bouvet-agent.service
```

---

## Boot Sequence

The complete boot sequence from Firecracker start to agent ready:

```
┌──────────────────────────────────────────────────────────────────┐
│                        Boot Sequence                              │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Firecracker loads vmlinux kernel                             │
│     └── Kernel version: 5.10 (from AWS S3)                       │
│                                                                   │
│  2. Kernel mounts rootfs.ext4                                    │
│     └── Mount point: / (root filesystem)                         │
│     └── Device: /dev/vda (Firecracker virtio-blk)                │
│                                                                   │
│  3. systemd starts as PID 1                                      │
│     └── CMD ["/sbin/init"]                                       │
│                                                                   │
│  4. systemd-modules-load.service runs                            │
│     └── Loads modules from /etc/modules-load.d/vsock.conf:       │
│         • vsock                                                   │
│         • vmw_vsock_virtio_transport                             │
│                                                                   │
│  5. bouvet-agent.service starts                                  │
│     └── ExecStart=/usr/local/bin/bouvet-agent                    │
│                                                                   │
│  6. Agent binds vsock port 52                                    │
│     └── VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY, 52))  │
│                                                                   │
│  7. Agent ready for host connections                             │
│     └── Host connects via /tmp/bouvet/{vm-id}/v.sock             │
│                                                                   │
└──────────────────────────────────────────────────────────────────┘
```

### Boot Time

| Phase | Duration | Notes |
|-------|----------|-------|
| Firecracker start | ~50ms | VM creation and kernel load |
| Kernel boot | ~200ms | Linux kernel initialization |
| systemd init | ~150ms | Service manager startup |
| Agent ready | ~100ms | vsock bind and listen |
| **Total** | **~500ms** | Cold start time |

> [!TIP]
> The warm pool reduces effective startup time to ~150ms by pre-booting sandboxes.

---

## Build Process

The rootfs build is a multi-stage process orchestrated by the [Makefile](file:///Users/vrn21/Developer/rust/petty/Makefile):

### Build Pipeline

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Build Pipeline                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  make rootfs                                                         │
│      │                                                               │
│      ▼                                                               │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │ Stage 1: Cross-compile bouvet-agent (Dockerfile.agent)         │ │
│  │   • Uses rust:slim-bookworm + musl-tools                       │ │
│  │   • Target: aarch64-unknown-linux-musl (or x86_64)             │ │
│  │   • Output: images/output/bouvet-agent                         │ │
│  └────────────────────────────────────────────────────────────────┘ │
│      │                                                               │
│      ▼                                                               │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │ Stage 2: Build devbox Docker image (Dockerfile.devbox)        │ │
│  │   • Debian Bookworm slim base                                  │ │
│  │   • Install Python, Node.js, Rust, dev tools                   │ │
│  │   • COPY bouvet-agent → /usr/local/bin/                       │ │
│  │   • COPY bouvet-agent.service → /etc/systemd/system/          │ │
│  └────────────────────────────────────────────────────────────────┘ │
│      │                                                               │
│      ▼                                                               │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │ Stage 3: Export Docker image as tarball                       │ │
│  │   • docker create --name bouvet-export-temp                    │ │
│  │   • docker export bouvet-export-temp > rootfs.tar              │ │
│  │   • Output: images/output/rootfs.tar                           │ │
│  └────────────────────────────────────────────────────────────────┘ │
│      │                                                               │
│      ▼                                                               │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │ Stage 4: Convert to ext4 (Dockerfile.ext4)                    │ │
│  │   • Extract rootfs.tar to /rootfs directory                   │ │
│  │   • mke2fs -t ext4 -d /rootfs rootfs.ext4                     │ │
│  │   • e2fsck -f -y && resize2fs -M (shrink to minimum)          │ │
│  │   • Output: images/output/debian-devbox.ext4                   │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Cross-Compilation for aarch64/x86_64

The build supports both architectures via Docker platform flags:

```makefile
ARCH ?= aarch64

ifeq ($(ARCH),aarch64)
    RUST_TARGET := aarch64-unknown-linux-musl
    DOCKER_PLATFORM := linux/arm64
else ifeq ($(ARCH),x86_64)
    RUST_TARGET := x86_64-unknown-linux-musl
    DOCKER_PLATFORM := linux/amd64
endif
```

Building for a specific architecture:

```bash
# ARM64 (default for Apple Silicon)
make rootfs ARCH=aarch64

# x86_64
make rootfs ARCH=x86_64
```

### Agent Cross-Compilation

The [Dockerfile.agent](file:///Users/vrn21/Developer/rust/petty/images/Dockerfile.agent) handles cross-compilation:

```dockerfile
FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    musl-tools \
    musl-dev

ARG RUST_TARGET=aarch64-unknown-linux-musl
RUN rustup target add ${RUST_TARGET}

# Build with musl for static linking
RUN cargo build -p bouvet-agent --release --target ${RUST_TARGET}
```

| Feature | Benefit |
|---------|---------|
| musl libc | Statically linked, no glibc dependency |
| Docker QEMU | Transparent ARM64/x86_64 emulation |
| Multi-stage | Minimal output (binary only) |

### ext4 Creation (Dockerfile.ext4)

The [Dockerfile.ext4](file:///Users/vrn21/Developer/rust/petty/images/Dockerfile.ext4) converts the tarball to ext4:

```dockerfile
FROM debian:bookworm-slim AS builder

RUN apt-get install -y --no-install-recommends e2fsprogs

ARG IMAGE_SIZE=1500M
ARG IMAGE_LABEL=bouvet-rootfs

# Create ext4 directly from directory (no mount needed)
RUN mke2fs -t ext4 -d /rootfs -L "${IMAGE_LABEL}" /rootfs.ext4 "${IMAGE_SIZE}"

# Shrink to minimum size
RUN e2fsck -f -y /rootfs.ext4 || true
RUN resize2fs -M /rootfs.ext4
```

> [!IMPORTANT]
> Using `mke2fs -d` allows creating an ext4 filesystem from a directory without requiring mount privileges. This enables building on macOS through Docker.

### Image Size Optimization

| Technique | Command | Savings |
|-----------|---------|---------|
| Initial overallocation | `IMAGE_SIZE=1500M` | Ensures space for all files |
| Filesystem check | `e2fsck -f -y` | Fixes any issues before resize |
| Shrink to minimum | `resize2fs -M` | Removes unused blocks |

---

## vsock Module Loading

The rootfs is configured to automatically load vsock kernel modules at boot:

```dockerfile
RUN mkdir -p /etc/modules-load.d && \
    echo "vsock" > /etc/modules-load.d/vsock.conf && \
    echo "vmw_vsock_virtio_transport" >> /etc/modules-load.d/vsock.conf
```

This creates `/etc/modules-load.d/vsock.conf` containing:
```
vsock
vmw_vsock_virtio_transport
```

These modules are loaded by `systemd-modules-load.service` before the agent starts.

---

## Serial Console

A serial console is enabled for debugging:

```dockerfile
RUN mkdir -p /etc/systemd/system/getty.target.wants && \
    ln -sf /lib/systemd/system/serial-getty@.service \
    /etc/systemd/system/getty.target.wants/serial-getty@ttyS0.service
```

This allows connecting to the VM console via Firecracker's serial socket for debugging boot issues.

---

## Root User

The root password is set to `root` for debugging access:

```dockerfile
RUN echo 'root:root' | chpasswd
```

> [!WARNING]
> This is intentional for development. The VM is isolated by Firecracker's security boundary, and the sandbox is ephemeral.

---

## Output Artifacts

After building, the following files are created in `images/output/`:

| File | Size | Description |
|------|------|-------------|
| `bouvet-agent` | ~3 MB | Statically linked Linux binary |
| `rootfs.tar` | ~1.2 GB | Intermediate tarball (deleted after build) |
| `debian-devbox.ext4` | ~800 MB | Final ext4 filesystem image |

---

## Makefile Targets

| Target | Command | Description |
|--------|---------|-------------|
| `rootfs` | `make rootfs` | Build complete ext4 image (default) |
| `agent` | `make agent` | Cross-compile agent only |
| `docker-image` | `make docker-image` | Build Docker image (no ext4) |
| `clean` | `make clean` | Remove all build artifacts |
| `rebuild` | `make rebuild` | Force clean rebuild |

Configuration variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `ARCH` | `aarch64` | Target architecture |
| `IMAGE_SIZE` | `2500M` | Initial ext4 size (shrunk later) |
| `IMAGE_NAME` | `bouvet-devbox` | Docker image name |

---

## See Also

- [AGENT_INTERNALS.md](file:///Users/vrn21/Developer/rust/petty/docs/internals/AGENT_INTERNALS.md) — Agent implementation details
- [VM_LAYER.md](file:///Users/vrn21/Developer/rust/petty/docs/internals/VM_LAYER.md) — Firecracker wrapper documentation
- [VSOCK_COMMUNICATION.md](file:///Users/vrn21/Developer/rust/petty/docs/internals/VSOCK_COMMUNICATION.md) — Host-guest communication
