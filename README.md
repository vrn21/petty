<p align="center">
  <img src="docs/assets/bouvet.png" alt="Bouvet Logo" />
</p>

<h1 align="center">Bouvet</h1>

<p align="center">
  <strong>Isolated code execution sandboxes for AI agents</strong>
</p>

<p align="center">
  <a href="#what-is-bouvet">About</a> •
  <a href="#how-it-works">How It Works</a> •
  <a href="#mcp-tools">MCP Tools</a> •
  <a href="#documentation">Documentation</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License" />
  <img src="https://img.shields.io/badge/rust-nightly-orange.svg" alt="Rust" />
  <img src="https://img.shields.io/badge/firecracker-1.5-red.svg" alt="Firecracker" />
  <a href="https://deepwiki.com/vrn21/bouvet"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki"></a>
</p>

---

## What is Bouvet?

Bouvet ("boo-veh") is an MCP server that creates secure, isolated sandboxes for AI agents to execute code.

When an AI agent needs to run Python, Node.js, or shell commands, Bouvet spins up a lightweight microVM in ~200ms. The code runs in complete isolation separate kernel filesystem and network then the sandbox is destroyed. Nothing persists, nothing leaks.

**The problem it solves:** AI agents need a safe place to run untrusted code. Docker isn't enough containers share the host kernel. Bouvet uses [Firecracker](https://firecracker-microvm.github.io/) microVMs for true hardware-level isolation the same technology that powers AWS Lambda.

**Who it's for:** Developers building AI agents with Claude, Cursor, or any MCP-compatible client who need secure code execution without managing infrastructure.

---

## How It Works

```
┌─────────────┐     ┌─────────────┐     ┌─────────────────────────┐
│  AI Agent   │────▶│ bouvet-mcp  │────▶│  Firecracker microVM    │
│  (Claude)   │     │ (MCP Server)│     │  ┌─────────────────┐    │
└─────────────┘     └─────────────┘     │  │  bouvet-agent   │    │
                                        │  │  (guest daemon) │    │
                                        │  └─────────────────┘    │
                                        └─────────────────────────┘
```

1. AI agent requests a sandbox via MCP
2. Bouvet boots a microVM with your chosen toolchain
3. Agent executes code, reads/writes files
4. Sandbox is destroyed when done

Each microVM has ~256MB RAM, 1 vCPU, and a full Linux environment with Python, Node.js, and common dev tools pre-installed.

---

## Features

- **True Isolation** — Each sandbox is a separate VM, not a container
- **Fast Startup** — Warm pool enables sub-200ms sandbox creation
- **Multi-Language** — Python, Node.js, Rust, Bash, and shell access
- **MCP Native** — Works with Claude, Cursor, and any MCP client

---

## MCP Tools

| Tool              | Description                          |
| ----------------- | ------------------------------------ |
| `create_sandbox`  | Create a new isolated sandbox        |
| `destroy_sandbox` | Destroy a sandbox and free resources |
| `list_sandboxes`  | List all active sandboxes            |
| `execute_code`    | Run Python, Node.js, or Bash code    |
| `run_command`     | Execute shell commands               |
| `read_file`       | Read file contents from sandbox      |
| `write_file`      | Write file contents to sandbox       |
| `list_directory`  | List directory contents              |

---

## Documentation

| Document                                | Description                              |
| --------------------------------------- | ---------------------------------------- |
| [Self-Hosting Guide](docs/SELF_HOST.md) | Deploy Bouvet on your own infrastructure |
| [Configuration](docs/CONFIG.md)         | Environment variables and options        |
| [Architecture](docs/ARCHITECTURE.md)    | Technical deep dive                      |

---

## License

Apache 2.0 — See [LICENSE](LICENSE) for details.

---

<p align="center">
  Built with 🔥 Firecracker and 🦀 Rust
</p>
