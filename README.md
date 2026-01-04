<p align="center">
  <!-- TODO: Add logo -->
  <img src="docs/assets/logo.png" alt="Bouvet Logo" width="200" />
</p>

<h1 align="center">Bouvet | പെട്ടി </h1>

<p align="center">
  <strong>Isolated code execution sandboxes for AI agents</strong>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#deployment">Deployment</a> •
  <a href="#documentation">Documentation</a> •
  <a href="#license">License</a>
</p>

<p align="center">
  <!-- TODO: Add badges -->
  <img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License" />
  <img src="https://img.shields.io/badge/rust-1.75+-orange.svg" alt="Rust" />
  <img src="https://img.shields.io/badge/firecracker-1.5-red.svg" alt="Firecracker" />
</p>

---

Bouvet creates secure, ephemeral microVMs where AI agents can run arbitrary code without affecting your host system. Each sandbox boots in ~200ms and is completely isolated.

<!-- TODO: Add demo GIF -->
<!-- ![Demo](docs/assets/demo.gif) -->

---

## Features

🔒 **Secure Isolation** — Each sandbox runs in its own Firecracker microVM

⚡ **Fast Startup** — Warm pool enables sub-200ms sandbox creation

🐍 **Multi-Language** — Python, Node.js, Bash out of the box

🔌 **MCP Native** — Works with Claude, Cursor, and any MCP client

🌐 **Dual Transport** — Local (stdio) and remote (HTTP/SSE) support

🚀 **Self-Host or Cloud** — Run on your own hardware or deploy to AWS

---

## Quick Start

### With Claude Desktop

Add to your MCP config (`~/.config/claude/config.json`):

```json
{
  "mcpServers": {
    "bouvet": {
      "command": "docker",
      "args": [
        "run",
        "--rm",
        "-i",
        "--privileged",
        "ghcr.io/vrn21/bouvet-mcp:latest"
      ]
    }
  }
}
```

### With HTTP API

```bash
# Start server
docker run --privileged -p 8080:8080 ghcr.io/vrn21/bouvet-mcp:latest

# Test connection
curl http://localhost:8080/health
```

---

## Deployment

### Self-Host

Run on any Linux machine with KVM support:

```bash
docker run --privileged -p 8080:8080 ghcr.io/vrn21/bouvet-mcp:latest
```

**Requirements:** Linux, Docker, `/dev/kvm`

### Cloud (AWS)

Deploy to AWS c5.metal with Terraform:

```bash
cd terraform
terraform apply -var="ssh_key_name=your-key"
```

See [Terraform README](terraform/README.md) for details.

---

## MCP Tools

| Tool              | Description                   |
| ----------------- | ----------------------------- |
| `create_sandbox`  | Create a new isolated sandbox |
| `destroy_sandbox` | Destroy a sandbox             |
| `list_sandboxes`  | List active sandboxes         |
| `execute_code`    | Run Python, Node.js, or Bash  |
| `run_command`     | Execute shell commands        |
| `read_file`       | Read file contents            |
| `write_file`      | Write file contents           |
| `list_directory`  | List directory contents       |

---

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────────────────┐
│  AI Agent   │────▶│  bouvet-mcp  │────▶│  Firecracker microVM    │
│  (Claude)   │     │  (MCP Server)│     │  ┌─────────────────┐   │
└─────────────┘     └─────────────┘     │  │  bouvet-agent    │   │
                                         │  │  (guest daemon) │   │
                                         │  └─────────────────┘   │
                                         └─────────────────────────┘
```

---

## Documentation

- [Configuration](docs/CONFIG.md) — Environment variables and options
- [Terraform](terraform/README.md) — AWS deployment guide
- [Architecture](docs/ARCHITECTURE.md) — Technical deep dive
- [Development](docs/dev/) — Design documents

---

## License

Apache 2.0 — See [LICENSE](LICENSE) for details.

---

<p align="center">
  Built with 🔥 Firecracker and ❤️ Rust
</p>
