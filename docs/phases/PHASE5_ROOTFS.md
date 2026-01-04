# Phase 5: Rootfs Images

> Debian-based ext4 images with bouvet-agent and language toolchains.

---

## Purpose

Create ext4 root filesystem images for Firecracker microVMs that include:

1. Minimal Debian base (bookworm-slim)
2. Compiled bouvet-agent binary
3. Language development toolchains (Python, Node.js, Rust, C++)
4. Common developer tools
5. Systemd service to auto-start bouvet-agent

---

## Image Strategy: Unified Devbox

We use a **single unified image** containing all toolchains. This is optimal for an agentic sandbox where the agent may need any language at runtime.

| Aspect      | Unified Image Approach                  |
| ----------- | --------------------------------------- |
| Image Name  | `debian-devbox.ext4`                    |
| Target Size | ~1.5-2 GB                               |
| Toolchains  | Python, Node.js, Rust, C/C++, Bash      |
| Rationale   | Agent doesn't know ahead which language |
| Maintenance | Single Dockerfile to maintain           |

---

## Directory Structure

```
images/
├── Dockerfile.devbox           # Unified dev environment
├── Dockerfile.ext4             # Container → ext4 converter
├── bouvet-agent.service         # Systemd unit file
├── .dockerignore               # Docker build exclusions
└── output/
    └── debian-devbox.ext4      # Generated image
```

---

## Build Process

### Overview

```
make rootfs
    │
    ├─► cargo build (x86_64-unknown-linux-musl)
    │   └─► bouvet-agent binary
    │
    ├─► docker build -f Dockerfile.devbox
    │   └─► bouvet-devbox:latest
    │
    ├─► docker export → rootfs.tar
    │
    └─► docker build -f Dockerfile.ext4
        └─► mke2fs -d (no mount needed!)
        └─► debian-devbox.ext4
```

### macOS Compatibility

The build works on macOS by running all Linux operations inside Docker:

- **Cross-compilation**: Rust's `x86_64-unknown-linux-musl` target
- **ext4 creation**: Uses `mke2fs -d` inside a Docker container
- **No ext4 mounting**: Avoids the "macOS can't mount ext4" problem

---

## Build Commands

```bash
# Build everything (default)
make rootfs

# Build just the agent binary
make agent

# Build Docker image only (no ext4)
make docker-image

# Clean generated files
make clean

# Custom image size
IMAGE_SIZE=4G make rootfs
```

---

## Image Contents

### Base System

- Debian Bookworm Slim
- systemd, iproute2, iputils-ping
- ca-certificates, openssh-client
- curl, wget

### Developer Tools

- git, vim, nano, less
- tree, htop, jq, rsync
- strace, gdb (debugging)
- zip, unzip, tar, xz-utils

### Python Stack

- python3, pip, venv
- python3-dev (for native extensions)

### Node.js Stack

- Node.js 20.x
- npm, npx

### Rust Stack

- rustc, cargo (stable)
- clippy, rustfmt

### C/C++ Stack

- gcc, g++, clang
- cmake, make
- build-essential, pkg-config

### Shell Tools

- bash, shellcheck

### Bouvet Agent

- `/usr/local/bin/bouvet-agent` (static musl binary)
- systemd service auto-starts on boot

---

## Systemd Service

File: `/etc/systemd/system/bouvet-agent.service`

```ini
[Unit]
Description=Bouvet Guest Agent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/bouvet-agent
Restart=always
RestartSec=1

[Install]
WantedBy=multi-user.target
```

---

## Size Optimization Techniques

1. **Use debian:bookworm-slim as base**
2. **--no-install-recommends** with apt-get
3. **Clean apt cache** after each layer: `rm -rf /var/lib/apt/lists/*`
4. **Remove docs/man pages**: `rm -rf /usr/share/doc /usr/share/man`
5. **Static bouvet-agent binary** (musl, no runtime deps)
6. **Shrink ext4** after creation: `resize2fs -M`

---

## Kernel Requirement

A compatible Linux kernel is also needed. Options:

- Build minimal kernel from Firecracker quickstart
- Use AWS demo kernel from Firecracker releases
- Pre-built kernels: `vmlinux.bin` (~5-10 MB)

---

## Environment Variables (Build-time)

| Variable     | Default        | Description       |
| ------------ | -------------- | ----------------- |
| `IMAGE_SIZE` | `2G`           | ext4 image size   |
| `IMAGE_NAME` | `bouvet-devbox` | Docker image name |

---

## Acceptance Criteria

- [x] Build works on macOS (via Docker)
- [ ] `make rootfs` produces valid ext4 image
- [ ] Image boots and bouvet-agent starts
- [ ] Python, Node, Rust, C++ toolchains work
- [ ] Image size under 2 GB
- [ ] Common dev tools available (git, vim, etc.)

---

## Build Requirements

- Docker (Docker Desktop on macOS)
- Rust toolchain with musl target
- ~5 GB disk space for build artifacts

---

## Future Enhancements

- Specialized images (if needed):
  - `debian-minimal.ext4` (~200 MB) - Shell only
  - `debian-ml.ext4` (~3-4 GB) - Python + ML libs
- Pre-built kernel included in repository
- CI/CD pipeline for automated builds
