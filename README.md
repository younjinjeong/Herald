# Herald

**Claude Code Telegram Remote Control**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange.svg)](https://www.rust-lang.org/)

Herald relays Claude Code session I/O to Telegram, letting you monitor and control Claude Code from your phone. It runs as a daemon (`heraldd`) that connects to Telegram via long polling — no inbound ports required. Supports Linux, macOS, Docker, and Kubernetes.

---

## Architecture

```
  Machine A (daemon host)              Machine B (remote)
┌────────────────────────────┐       ┌──────────────────────┐
│ Claude Code #1  #2  #N     │       │ Claude Code #3       │
│   │  Plugin hooks  │       │       │   │  Plugin hooks     │
│   └───────┬────────┘       │       │   └──────┬────────────┤
│           │ Unix Socket    │       │          │ TCP :7272   │
│    ┌──────▼──────────┐     │       │          │             │
│    │    heraldd      │◄────┼───────┼──────────┘             │
│    │                 │     │       └──────────────────────────
│    │ Session Registry│     │
│    │ Token Monitor   │     │       ┌──────────────────────┐
│    │ Conversation Log│     │       │    Docker / K8s      │
│    │ Content Filter  │     │       │  ┌────────────────┐  │
│    └────────┬────────┘     │       │  │   heraldd      │  │
│             │              │       │  │   (container)  │  │
└─────────────┼──────────────┘       │  │   TCP :7272    │  │
              │ HTTPS (outbound)     │  └────────┬───────┘  │
              ▼                      │           │ stdout   │
    ┌──────────────────┐             │  Promtail → Loki     │
    │ Telegram Bot API │             └──────────────────────┘
    │  (Long Polling)  │
    └────────┬─────────┘
             ▼
    ┌──────────────────┐
    │ Telegram Mobile  │
    └──────────────────┘
```

## Features

- **Multi-machine** — connect Claude Code sessions from multiple machines via TCP
- **Token monitoring** — track input/output tokens and cost per session in real-time
- **Conversation logging** — log user prompts and Claude responses to `/var/log/herald-relay.log`
- **Setup wizard** — guided setup with OTP-based Telegram verification
- **Multi-session** — monitor multiple Claude Code sessions simultaneously
- **Outbound only** — works behind firewalls, no inbound ports needed
- **Cross-platform** — Linux and macOS support
- **Container-ready** — Docker, docker-compose, and Kubernetes with Loki log aggregation
- **Secure** — bot token in system keyring, peer credential verification, content filtering
- **Headless control** — send prompts to Claude Code from Telegram via `claude -p`
- **Plugin hooks** — automatic session registration via Claude Code plugin system

## Prerequisites

- **Rust** 1.75+ (2024 edition)
- **Linux** (WSL2 works) or **macOS**
- **Telegram Bot** token from [@BotFather](https://t.me/BotFather)
- **Claude Code** installed and accessible in `$PATH`
- **jq** (used by plugin hook scripts)

Or use **Docker** (no Rust toolchain needed).

---

## Installation

### Build from source

```bash
git clone https://github.com/younjinjeong/Herald.git
cd Herald
cargo build --release

# Install binaries
cp target/release/herald target/release/heraldd ~/.local/bin/
```

### Docker

```bash
docker build -t herald .
docker run -e HERALD_BOT_TOKEN=your_token herald
```

### macOS

```bash
cargo build --release
cp target/release/herald target/release/heraldd /usr/local/bin/

# Install as LaunchAgent (auto-start on login)
cp launchd/com.herald.daemon.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.herald.daemon.plist
```

### Linux systemd (optional)

```bash
mkdir -p ~/.config/systemd/user
cp systemd/heraldd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now heraldd
```

---

## Quick Start

### 1. Create a Telegram Bot

1. Open Telegram and search for **@BotFather**
2. Send `/newbot`
3. Choose a display name (e.g., "Herald Relay")
4. Choose a username ending with `bot` (e.g., `my_herald_bot`)
5. Copy the bot token

### 2. Run Setup

```
$ herald setup

  Herald - Claude Code Telegram Relay
  ====================================

  Step 1/4: Bot Token
  Enter your Telegram Bot Token (from @BotFather):
  > ********

  Step 2/4: Validating bot token...
  Bot: @my_herald_bot

  Step 3/4: Storing bot token securely
  Stored in system keyring

  Step 4/4: Authentication

  Send this code to @my_herald_bot in Telegram:

    >>> 483291 <<<

  Waiting for verification (5 minute timeout)...

  Verified! (chat_id: 123456789)
  Config written to ~/.config/herald/config.toml

  ====================================
  Setup complete!
```

### 3. Start the Daemon

```bash
herald start
```

### 4. Connect Claude Code

```bash
claude --plugin-dir /path/to/Herald/plugin
```

### 5. Use from Telegram

Send `/start` to your bot — you're connected.

---

## Multi-Machine Setup

Connect Claude Code sessions from remote machines to a central Herald daemon via TCP.

**On the daemon host**, configure TCP transport:

```toml
# ~/.config/herald/config.toml
[daemon]
transport = "both"          # Unix socket + TCP
listen_addr = "0.0.0.0:7272"
```

**On remote machines**, set the daemon address:

```bash
export HERALD_DAEMON_ADDR="daemon-host:7272"
claude --plugin-dir /path/to/Herald/plugin
```

The plugin hooks auto-detect `HERALD_DAEMON_ADDR` and connect via TCP instead of Unix socket.

You can also send prompts manually:

```bash
herald send --tcp daemon-host:7272 session-id "Fix the auth bug"
```

---

## Telegram Commands

| Command | Description |
|---------|-------------|
| `/start` | Connect and show auth status |
| `/sessions` | List sessions with color tags and active indicator |
| `/status` | Show daemon uptime, session count, connection status |
| `/tokens` | Show token usage and cost across all sessions |
| `/log` | Show recent conversation log for selected session |
| `/help` | Command reference |

### Sending Prompts

Three ways to target a session when sending a message:

| Method | Example | Effect |
|--------|---------|--------|
| **Active session** | Just type text | Goes to session selected via `/sessions` |
| **@prefix** | `@ml-pipeline run the tests` | Routes to named session without switching active |
| **Reply** | Reply to any tagged message | Auto-routes to that session |

---

## Session Tagging

Each session gets a color emoji and short name (derived from working directory). All messages in the shared bot chat are tagged:

```
🟢 [project-api] 🔧 Tool: Edit login.rs (+5 -2)
🟡 [ml-pipeline] 🤖 Claude: Starting training job...
🟢 [project-api] 📊 Tokens: 12.4K in / 3.2K out
🟡 [ml-pipeline] 👤 You: "Show the loss curve"
```

Colors cycle through 🟢 🟡 🔵 🟣 🟠 as sessions register. The `/sessions` command shows an active indicator:

```
Active Sessions:

🟢 [project-api] (api) ◀ active
🟡 [ml-pipeline] (ml-pipeline)

Tap a session to select it.
Tip: @name or reply to target a session.
```

---

## Token Monitoring

Herald tracks token consumption per session in real-time.

The `/tokens` command shows:

```
Token Usage Summary
━━━━━━━━━━━━━━━━━━━━
Total: 45.2K in / 12.1K out
Cache: 32.0K read / 8.5K created
Cost:  $0.3172

Per session:
  abc123 — 25.1K in / 7.2K out ($0.1830)
  def456 — 20.1K in / 4.9K out ($0.1342)
```

Token data is extracted from the Claude Code session transcript after each tool use. Cost is estimated using Claude Sonnet 4 pricing.

---

## Conversation Logging

Herald captures user prompts and Claude's descriptive responses (not code or raw CLI output).

**Telegram delivery** — conversation entries appear in real-time:

```
👤 You: "Fix the authentication bug in login.rs"

🤖 Claude: "I found the issue — the token validation was
skipping the expiry check. I've updated the verify_token
function to include a timestamp comparison."

🔧 Tool: Edit login.rs (+5 -2)
```

**File log** — written to `/var/log/herald-relay.log` (Linux) or `~/Library/Logs/herald/herald-relay.log` (macOS):

```
2026-03-15T14:32:05+00:00 [session:abc123] USER: Fix the authentication bug in login.rs
2026-03-15T14:32:08+00:00 [session:abc123] CLAUDE: I found the issue — the token validation was skipping the expiry check.
2026-03-15T14:32:10+00:00 [session:abc123] TOOL: Edit login.rs (+5 -2)
2026-03-15T14:32:15+00:00 [session:abc123] TOKENS: in=1200 out=450 cache_read=800 cost=$0.0120
```

Code blocks and command outputs are automatically stripped from logged responses. Secrets (API keys, passwords) are redacted.

Use `/log` in Telegram to view the last 10 conversation entries for the selected session.

---

## Docker Deployment

### Standalone

```bash
docker build -t herald .
docker run -d \
  -e HERALD_BOT_TOKEN=your_token \
  -p 7272:7272 \
  herald
```

### With Loki monitoring

```bash
# Start Herald + Loki + Promtail + Grafana
docker compose --profile monitoring up -d
```

Container mode automatically:
- Outputs structured JSON logs to stdout (for Loki/Promtail)
- Uses TCP transport (no Unix socket)
- Skips systemd integration
- Uses token-only authentication (no peer credentials)

### Kubernetes

```bash
# Create bot token secret
kubectl create secret generic herald-secrets \
  --from-literal=bot-token=YOUR_BOT_TOKEN

# Deploy
kubectl apply -f k8s/deployment.yaml

# Optional: deploy Promtail with Herald-aware scrape config
kubectl apply -f k8s/promtail-config.yaml
```

---

## Claude Code Plugin

Load the Herald plugin when starting Claude Code:

```bash
claude --plugin-dir /path/to/Herald/plugin
```

The plugin automatically:
- Registers the session with `heraldd` on start
- Captures your prompts (`UserPromptSubmit` hook)
- Relays tool output (Bash, Write, Edit, Read) to Telegram
- Extracts token usage from session transcript
- Captures Claude's responses for conversation logging
- Forwards notifications and permission requests
- Unregisters the session on exit

### Plugin hooks

| Event | Script | Behavior |
|-------|--------|----------|
| `SessionStart` | `on-session-start.sh` | Registers session with daemon |
| `UserPromptSubmit` | `on-user-prompt.sh` | Captures user prompt for conversation log |
| `PostToolUse` | `on-post-tool-use.sh` | Relays tool output + extracts token usage |
| `Notification` | `on-notification.sh` | Relays notifications (async) |
| `Stop` | `on-stop.sh` | Captures assistant response + session stop |
| `SessionEnd` | `on-session-end.sh` | Unregisters session |

---

## Configuration

Config file: `~/.config/herald/config.toml`

```toml
[daemon]
socket_path = "/run/user/1000/herald/herald.sock"
listen_addr = "0.0.0.0:7272"     # TCP listener for remote connections
transport = "unix"                # "unix" | "tcp" | "both"
log_level = "INFO"
log_output = "file"               # "file" | "stdout" | "both"
auth_mode = "peercred"            # "peercred" | "token_only"

[auth]
allowed_chat_ids = [123456789]
otp_timeout_seconds = 300
otp_max_attempts = 3

[output_filter]
mode = "summary"
max_message_length = 4096
code_preview_lines = 5
mask_secrets = true

[sessions]
max_concurrent = 10
```

Bot token is stored separately in the system keyring (not in the config file).

**Environment variables:**

| Variable | Description |
|----------|-------------|
| `HERALD_BOT_TOKEN` | Bot token (overrides keyring) |
| `HERALD_DAEMON_ADDR` | Remote daemon address for TCP (e.g., `host:7272`) |
| `HERALD_CONTAINER` | Set to `1` for container mode (stdout logging, token-only auth) |

---

## CLI Reference

```
herald setup                         # Interactive setup wizard
herald start                         # Start the daemon
herald stop                          # Stop the daemon
herald status                        # Show daemon and session status
herald send <session> <message>      # Send prompt to a session
herald send --tcp host:7272 <s> <m>  # Send prompt via TCP to remote daemon
```

---

## Security

| Layer | Linux | macOS | Container |
|-------|-------|-------|-----------|
| **IPC auth** | `SO_PEERCRED` (UID match) | `getpeereid` (UID match) | Token-only |
| **Bot token storage** | System keyring (libsecret) | System keyring (macOS Keychain) | Env variable |
| **Session tokens** | UUID v4, per-message validation | Same | Same |
| **OTP verification** | 6-digit, 5 min TTL, 3 attempts | Same | Same |
| **Content filtering** | API keys, passwords redacted | Same | Same |
| **Service hardening** | systemd `NoNewPrivileges`, `ProtectSystem=strict` | launchd | K8s resource limits |

---

## Project Structure

```
Herald/
├── crates/
│   ├── herald-core/           # Shared library
│   │   └── src/
│   │       ├── config.rs          # TOML config + keyring
│   │       ├── logging.rs         # Conversation logger (file + stdout JSON)
│   │       ├── ipc/               # Unix socket + TCP protocol
│   │       ├── auth/              # OTP + chat_id auth
│   │       ├── telegram/          # Bot, commands, handlers
│   │       ├── session/           # Registry + tokens + token usage
│   │       └── security/          # Peer credentials + content filter
│   ├── herald-cli/            # CLI binary (herald)
│   │   └── src/commands/          # setup, start, stop, status, send
│   └── herald-daemon/         # Daemon binary (heraldd)
│       └── src/
│           ├── service.rs         # IPC + Telegram orchestration
│           ├── headless.rs        # claude -p execution
│           ├── pty.rs             # PTY stdin injection (Linux only)
│           └── queue.rs           # Rate-limited message queue
├── plugin/                    # Claude Code plugin
│   ├── .claude-plugin/
│   ├── hooks/                     # Shell scripts for each event
│   └── commands/                  # /herald slash command
├── systemd/                   # Linux systemd unit
├── launchd/                   # macOS LaunchAgent plist
├── k8s/                       # Kubernetes manifests
│   ├── deployment.yaml
│   └── promtail-config.yaml
├── Dockerfile
├── docker-compose.yml
└── scripts/
    └── herald-relay.logrotate
```

---

## Development

```bash
# Build
cargo build

# Build without systemd (for macOS or containers)
cargo build --no-default-features

# Run tests
cargo test

# Lint
cargo clippy
```

### Key dependencies

| Crate | Purpose |
|-------|---------|
| [teloxide](https://github.com/teloxide/teloxide) | Telegram Bot API |
| [tokio](https://tokio.rs) | Async runtime (Unix sockets, TCP, signals) |
| [clap](https://github.com/clap-rs/clap) | CLI argument parsing |
| [nix](https://github.com/nix-rust/nix) | Unix socket credentials (SO_PEERCRED / getpeereid) |
| [keyring](https://github.com/hwchen/keyring-rs) | Secure token storage |
| [tracing](https://github.com/tokio-rs/tracing) | Structured logging |

---

## License

[MIT](LICENSE)
