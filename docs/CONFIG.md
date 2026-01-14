# Configuration Reference

Complete reference for all Bouvet configuration options.

---

## Environment Variables

All configuration is done via environment variables. The server reads these at startup.

### Core Paths

| Variable             | Default                              | Description                    |
| -------------------- | ------------------------------------ | ------------------------------ |
| `BOUVET_KERNEL`      | `/var/lib/bouvet/vmlinux`            | Path to the Linux kernel image |
| `BOUVET_ROOTFS`      | `/var/lib/bouvet/debian-devbox.ext4` | Path to the ext4 rootfs image  |
| `BOUVET_FIRECRACKER` | `/usr/local/bin/firecracker`         | Path to the Firecracker binary |
| `BOUVET_CHROOT`      | `/tmp/bouvet`                        | Working directory for VM state |

### Rootfs Source

| Variable            | Default                                                                  | Description                                   |
| ------------------- | ------------------------------------------------------------------------ | --------------------------------------------- |
| `BOUVET_ROOTFS_URL` | `https://bouvet-artifacts.s3.us-east-1.amazonaws.com/debian-devbox.ext4` | URL to download rootfs if not present locally |

> [!NOTE]
> The rootfs is downloaded automatically on first startup if `BOUVET_ROOTFS` doesn't exist at the configured path.

---

## Transport Configuration

| Variable           | Default   | Options                 | Description       |
| ------------------ | --------- | ----------------------- | ----------------- |
| `BOUVET_TRANSPORT` | `both`    | `stdio`, `http`, `both` | Transport mode    |
| `BOUVET_HTTP_HOST` | `0.0.0.0` | Any valid IP            | HTTP bind address |
| `BOUVET_HTTP_PORT` | `8080`    | `1-65535`               | HTTP port         |

### Transport Modes

| Mode    | stdio | HTTP | Use Case                                |
| ------- | ----- | ---- | --------------------------------------- |
| `both`  | ✓     | ✓    | Default — maximum compatibility         |
| `stdio` | ✓     | ✗    | Local AI tools (Claude Desktop, Cursor) |
| `http`  | ✗     | ✓    | Remote AI agents, production servers    |

---

## Warm Pool Settings

The warm pool pre-boots sandboxes for faster allocation (~150ms vs ~500ms cold start).

| Variable                | Default | Description                           |
| ----------------------- | ------- | ------------------------------------- |
| `BOUVET_POOL_ENABLED`   | `true`  | Enable warm sandbox pool              |
| `BOUVET_POOL_MIN_SIZE`  | `3`     | Minimum warm sandboxes to maintain    |
| `BOUVET_POOL_MAX_BOOTS` | `2`     | Max concurrent boots during pool fill |

> [!TIP]
> Disable pooling (`BOUVET_POOL_ENABLED=false`) for development or low-memory environments.

---

## Logging

| Variable   | Default | Description                                           |
| ---------- | ------- | ----------------------------------------------------- |
| `RUST_LOG` | `info`  | Log level (`error`, `warn`, `info`, `debug`, `trace`) |

Examples:

```bash
# Basic info logging
RUST_LOG=info

# Debug for bouvet only
RUST_LOG=bouvet_mcp=debug,info

# Trace all components
RUST_LOG=trace
```

---

## HTTP Endpoints

When HTTP transport is enabled:

| Endpoint  | Method | Description                  |
| --------- | ------ | ---------------------------- |
| `/health` | GET    | Health check (returns JSON)  |
| `/mcp`    | POST   | MCP JSON-RPC requests        |
| `/mcp`    | GET    | SSE stream for server events |
| `/`       | GET    | Server info page             |

### Health Check Response

```json
{
  "status": "healthy",
  "version": "0.1.0"
}
```

---

## MCP Tools Reference

| Tool              | Parameters                       | Description                         |
| ----------------- | -------------------------------- | ----------------------------------- |
| `create_sandbox`  | —                                | Create a new isolated sandbox       |
| `destroy_sandbox` | `sandbox_id`                     | Destroy a sandbox                   |
| `list_sandboxes`  | —                                | List all active sandboxes           |
| `execute_code`    | `sandbox_id`, `language`, `code` | Run code (python, node, bash, rust) |
| `run_command`     | `sandbox_id`, `command`          | Execute shell command               |
| `read_file`       | `sandbox_id`, `path`             | Read file contents                  |
| `write_file`      | `sandbox_id`, `path`, `content`  | Write file contents                 |
| `list_directory`  | `sandbox_id`, `path`             | List directory contents             |

### Supported Languages

| Language | Value                 | Runtime                 |
| -------- | --------------------- | ----------------------- |
| Python   | `python`              | Python 3.11             |
| Node.js  | `node`, `javascript`  | Node.js 20              |
| Bash     | `bash`, `shell`, `sh` | Bash 5.x                |
| Rust     | `rust`                | `rustc` (compile & run) |

---

## Sandbox Resources

Each microVM is allocated:

| Resource | Value                         |
| -------- | ----------------------------- |
| Memory   | 256 MB                        |
| vCPUs    | 1                             |
| Disk     | Shared rootfs (copy-on-write) |
| Network  | Isolated (no external access) |

---

## Limits

| Limit              | Value | Description                  |
| ------------------ | ----- | ---------------------------- |
| Max input size     | 10 MB | Maximum code/content input   |
| Max command length | 1 MB  | Maximum shell command length |
| Execution timeout  | 60s   | Default command timeout      |

---

## Docker Configuration

When running with Docker:

```bash
docker run -d \
  --privileged \
  --device=/dev/kvm \
  -p 8080:8080 \
  -e BOUVET_TRANSPORT=http \
  -e BOUVET_HTTP_PORT=8080 \
  -e BOUVET_POOL_MIN_SIZE=5 \
  -e RUST_LOG=info \
  ghcr.io/vrn21/bouvet-mcp:latest
```

### Required Docker Flags

| Flag                | Required | Description         |
| ------------------- | -------- | ------------------- |
| `--privileged`      | Yes\*    | Full access for KVM |
| `--device=/dev/kvm` | Yes\*    | KVM device access   |
| `-p 8080:8080`      | For HTTP | Expose MCP endpoint |

\*Either `--privileged` OR `--device=/dev/kvm` is required.

---

## Claude Desktop Configuration

remote HTTP:

```json
{
  "mcpServers": {
    "bouvet": {
      "url": "http://your-server-ip:8080/mcp"
    }
  }
}
```

---

## Example Configurations

### Development (Local)

```bash
export BOUVET_TRANSPORT=stdio
export BOUVET_POOL_ENABLED=false
export RUST_LOG=debug
```

### Production (Remote)

```bash
export BOUVET_TRANSPORT=http
export BOUVET_HTTP_HOST=0.0.0.0
export BOUVET_HTTP_PORT=8080
export BOUVET_POOL_ENABLED=true
export BOUVET_POOL_MIN_SIZE=5
export RUST_LOG=info
```

### High-Throughput

```bash
export BOUVET_POOL_ENABLED=true
export BOUVET_POOL_MIN_SIZE=10
export BOUVET_POOL_MAX_BOOTS=4
```

---

## Terraform Variables

For AWS deployment (see [Self-Hosting Guide](SELF_HOST.md)):

| Variable            | Description            | Default                           |
| ------------------- | ---------------------- | --------------------------------- |
| `ssh_key_name`      | AWS EC2 key pair name  | _required_                        |
| `aws_region`        | AWS region             | `us-east-1`                       |
| `instance_type`     | EC2 instance type      | `c5.metal`                        |
| `docker_image`      | Docker image to deploy | `ghcr.io/vrn21/bouvet-mcp:latest` |
| `rootfs_url`        | Rootfs download URL    | S3-hosted default                 |
| `allowed_ssh_cidrs` | SSH access CIDR blocks | `["0.0.0.0/0"]`                   |
| `volume_size`       | Root volume size (GB)  | `50`                              |
