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
│    │ Permission Gate │     │       │  │   heraldd      │  │
│    └────────┬────────┘     │       │  │   (container)  │  │
│             │              │       │  │   TCP :7272    │  │
└─────────────┼──────────────┘       │  └────────┬───────┘  │
              │ HTTPS (outbound)     │           │ stdout   │
              ▼                      │  Promtail → Loki     │
    ┌──────────────────┐             └──────────────────────┘
    │ Telegram Bot API │
    │  (Long Polling)  │
    └────────┬─────────┘
             ▼
    ┌──────────────────┐
    │ Telegram Mobile  │
    │  (Approve/Deny)  │
    └──────────────────┘
```

### Data Flow

1. **Plugin hooks** fire on Claude Code events (session start, tool use, prompts, responses)
2. Hooks serialize JSON messages and send them to the daemon via **Unix socket** (local) or **TCP** (remote)
3. The daemon maintains a **session registry** and forwards formatted messages to **Telegram** via the Bot API
4. Users can **monitor**, **send prompts**, and **approve/deny tool permissions** directly from the Telegram mobile app
5. The daemon supports **debounce-based completion detection** — it batches tool activity into a single summary with token usage and cost

---

## Features

- **Permission gating** — approve or deny tool execution (Bash, Write, Edit) from Telegram with inline buttons; auto-allows after 30s timeout
- **Multi-machine** — connect Claude Code sessions from multiple machines via TCP
- **Token monitoring** — track input/output tokens and cost per session in real-time
- **Conversation logging** — log user prompts and Claude responses to `/var/log/herald-relay.log`
- **Setup wizard** — guided setup with OTP-based Telegram verification
- **Multi-session** — monitor multiple Claude Code sessions simultaneously with color-coded tags
- **Outbound only** — works behind firewalls, no inbound ports needed
- **Cross-platform** — Linux and macOS support
- **Container-ready** — Docker, docker-compose, and Kubernetes with Loki log aggregation
- **Secure** — bot token in system keyring, peer credential verification, content filtering
- **Headless control** — send prompts to Claude Code from Telegram via `claude -p`
- **Plugin system** — installs as a Claude Code plugin with hooks and slash commands
- **MarkdownV2** — all Telegram messages are consistently formatted with MarkdownV2

---

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

### Install the Claude Code plugin

```bash
# Option A: Load directly from repo (recommended for development)
claude --plugin-dir /path/to/Herald/plugin

# Option B: Install via marketplace (persistent)
claude plugins marketplace add /path/to/Herald/plugin
claude plugins install herald@herald-local
```

See [Quick Start step 4](#4-connect-claude-code) for details on each method.

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

### Linux systemd

```bash
mkdir -p ~/.config/systemd/user
cp systemd/heraldd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now heraldd
```

---

## Updating

After pulling new changes or rebuilding locally:

### Binaries (daemon + CLI)

```bash
# Stop daemon, rebuild, install, restart
systemctl --user stop heraldd
cargo build --release
cp target/release/herald target/release/heraldd ~/.local/bin/
systemctl --user start heraldd
```

### Plugin (hooks + commands)

If you use `--plugin-dir`, **no action needed** — it always loads from source.

If you installed via the marketplace:

```bash
claude plugins update herald@herald-local
```

This copies the latest hook scripts and commands to `~/.claude/plugins/cache/`.

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
# Or via systemd:
systemctl --user start heraldd
```

### 4. Connect Claude Code

**Method A: `--plugin-dir` (recommended for development)**

Load the plugin directly from your repo checkout. No install step needed — always uses the latest source:

```bash
claude --plugin-dir /path/to/Herald/plugin
```

**Method B: Plugin marketplace (persistent install)**

Install once, then just run `claude` without extra flags:

```bash
claude plugins marketplace add /path/to/Herald/plugin
claude plugins install herald@herald-local

# From now on, just:
claude
```

Note: marketplace installs copy files to `~/.claude/plugins/cache/`, so after pulling changes you need to run `claude plugins update herald@herald-local`. See [Updating](#updating).

Once connected, you'll see a registration message in Telegram. Use `/sessions` in Telegram to see active sessions.

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

**On remote machines**, use the `/herald-connect` slash command inside Claude Code:

```
/herald-connect 192.168.1.100:7272
```

This writes `HERALD_DAEMON_ADDR` to the plugin's `config.env` and tests connectivity. To revert:

```
/herald-disconnect
```

You can also set the address manually:

```bash
export HERALD_DAEMON_ADDR="daemon-host:7272"
```

Or write it to the plugin's config file:

```bash
echo "HERALD_DAEMON_ADDR=daemon-host:7272" >> /path/to/Herald/plugin/config.env
```

---

## Permission Gating

When Claude Code attempts to use a mutating tool (Bash, Write, Edit, NotebookEdit), Herald sends a permission request to Telegram with **Approve** and **Deny** inline buttons.

```
🟢 [project-api] 🔐 Permission request
⚙️ Tool: Bash
┌─────────────────────────────────┐
│ git status                      │
└─────────────────────────────────┘
  [ ✅ Approve ]  [ ❌ Deny ]
```

- **Tap Approve** → tool executes normally
- **Tap Deny** → tool is blocked
- **No response within 30s** → auto-approves (timeout is configurable via hook timeout)

`AskUserQuestion` tool calls are relayed as informational notifications (no buttons, no blocking).

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

## Claude Code Slash Commands

These commands are available inside Claude Code when the Herald plugin is active:

| Command | Description |
|---------|-------------|
| `/herald` | Show Herald daemon status and connection info |
| `/herald-connect <host:port>` | Connect to a remote Herald daemon |
| `/herald-disconnect` | Disconnect from remote daemon, revert to local Unix socket |

---

## Session Tagging

Each session gets a color emoji and short name (derived from working directory). All messages in the shared bot chat are tagged:

```
🟢 [project-api] 🔨 Working on:
> Fix the authentication bug

🟢 [project-api] ✅ Done (5 tools)
━━━━━━━━━━━━━━━━━━━━
📊 12.3K in / 3.2K out · $0.0234
━━━━━━━━━━━━━━━━━━━━
I fixed the token validation...

🟡 [ml-pipeline] 🔐 Permission request
⚙️ Tool: Bash
```

Colors cycle through 🟢 🟡 🔵 🟣 🟠 as sessions register.

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

**Telegram delivery** — conversation entries appear in real-time with debounced completion summaries:

```
🟢 [project-api] 🔨 Working on:
> Fix the authentication bug in login.rs

🟢 [project-api] ✅ Done (3 tools)
━━━━━━━━━━━━━━━━━━━━
📊 1.2K in / 450 out · $0.0120
━━━━━━━━━━━━━━━━━━━━
I found the issue — the token validation was
skipping the expiry check. I've updated the
verify_token function to include a timestamp comparison.
```

**File log** — written to `/var/log/herald-relay.log` (Linux) or `~/Library/Logs/herald/herald-relay.log` (macOS).

Code blocks and command outputs are automatically stripped from logged responses. Secrets (API keys, passwords) are redacted.

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

## Plugin Hooks

The Herald plugin registers hooks for each Claude Code lifecycle event:

| Event | Script | Timeout | Behavior |
|-------|--------|---------|----------|
| `PreToolUse` | `on-pre-tool-use.sh` | 35s | Permission gating for Bash/Write/Edit/NotebookEdit; AskUserQuestion notification relay |
| `SessionStart` | `on-session-start.sh` | 5s | Registers session with daemon, persists token |
| `UserPromptSubmit` | `on-user-prompt.sh` | 5s | Captures user prompt for conversation log |
| `PostToolUse` | `on-post-tool-use.sh` | 5s | Relays tool output + extracts token usage |
| `Notification` | `on-notification.sh` | 5s | Relays notifications (async) |
| `Stop` | `on-stop.sh` | 5s | Captures assistant response + marks session stopped |
| `SessionEnd` | `on-session-end.sh` | 5s | Unregisters session, cleans up token file |

All hooks share `herald-common.sh` which provides:
- Transport auto-detection (Unix socket vs TCP)
- IPC message sending with retry on 401 (daemon restart recovery)
- Token file management for TCP transport
- Config loading from `config.env`

---

## IPC Protocol

Herald uses a length-prefixed JSON protocol (4-byte big-endian length + UTF-8 JSON body) over Unix socket or TCP.

### Request Types

| Type | Fields | Description |
|------|--------|-------------|
| `Register` | session_id, pid, cwd, tmux_pane? | Register a new session |
| `Unregister` | session_id, token? | Unregister a session |
| `Output` | session_id, token?, tool_name, tool_input_summary, tool_response_summary | Report tool execution |
| `Notification` | session_id, token?, notification_type, message | Send notification |
| `SessionStopped` | session_id, token?, last_message | Mark session as stopped |
| `Input` | session_id, prompt | Execute prompt via headless mode |
| `TokenUpdate` | session_id, token?, input/output/cache tokens, cost | Update token usage |
| `ConversationEntry` | session_id, token?, entry_type, content, timestamp | Log conversation entry |
| `PermissionRequest` | session_id, token?, request_id, tool_name, tool_input | Request tool permission |
| `PermissionCheck` | request_id | Poll for permission decision |
| `Health` | — | Health check |
| `ListSessions` | — | List active sessions |
| `Shutdown` | — | Graceful shutdown |

### Response Types

| Type | Fields | Description |
|------|--------|-------------|
| `Ok` | message? | Success |
| `Registered` | token | Session registered, returns auth token |
| `Error` | code, message | Error (401 = invalid token, 404 = not found, 500 = internal) |
| `SessionList` | sessions[] | List of active sessions |
| `HealthStatus` | uptime_secs, session_count, telegram_connected | Daemon health |
| `PermissionResult` | decision | "allow", "deny", or "pending" |

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
debounce_seconds = 5              # Idle time before sending completion summary
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
| **Permission gating** | Telegram approve/deny for mutating tools | Same | Same |

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
│   │       │   ├── protocol.rs    # Request/Response enums + wire format
│   │       │   ├── server.rs      # IPC listener (Unix + TCP)
│   │       │   └── client.rs      # IPC sender
│   │       ├── auth/              # OTP + chat_id auth
│   │       ├── telegram/          # Bot, commands, handlers
│   │       │   ├── bot.rs         # BotState, message queue, pending permissions
│   │       │   ├── handlers.rs    # Text + callback handlers (approve/deny)
│   │       │   ├── callbacks.rs   # Inline keyboards (session select, permission)
│   │       │   ├── formatting.rs  # MarkdownV2 message formatting
│   │       │   └── commands.rs    # /start, /sessions, /status, etc.
│   │       ├── session/           # Registry + tokens + token usage
│   │       └── security/          # Peer credentials + content filter
│   ├── herald-cli/            # CLI binary (herald)
│   │   └── src/commands/          # setup, start, stop, status, send
│   └── herald-daemon/         # Daemon binary (heraldd)
│       └── src/
│           ├── service.rs         # IPC + Telegram + permission orchestration
│           ├── headless.rs        # claude -p execution
│           └── pty.rs             # Process monitoring
├── plugin/                    # Claude Code plugin
│   ├── .claude-plugin/
│   │   └── plugin.json           # Plugin manifest
│   ├── hooks/
│   │   ├── hooks.json            # Hook registration (7 event types)
│   │   ├── herald-common.sh      # Shared: transport, IPC, retry, token mgmt
│   │   ├── on-pre-tool-use.sh    # Permission gating + AskUserQuestion relay
│   │   ├── on-session-start.sh   # Session registration
│   │   ├── on-session-end.sh     # Session cleanup
│   │   ├── on-user-prompt.sh     # Prompt capture
│   │   ├── on-post-tool-use.sh   # Tool output + token extraction
│   │   ├── on-notification.sh    # Notification relay
│   │   └── on-stop.sh            # Assistant response + session stop
│   ├── commands/
│   │   ├── herald.md             # /herald — status + remote info
│   │   ├── herald-connect.md     # /herald-connect — set remote daemon
│   │   └── herald-disconnect.md  # /herald-disconnect — revert to local
│   └── config.env.example        # Example config (HERALD_DAEMON_ADDR)
├── systemd/                   # Linux systemd unit
│   └── heraldd.service
├── launchd/                   # macOS LaunchAgent plist
│   └── com.herald.daemon.plist
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

### Build

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Build without systemd (for macOS or containers)
cargo build --no-default-features
```

### Test

```bash
cargo test
```

### Install locally (after build)

```bash
# Install binaries
cp target/release/herald target/release/heraldd ~/.local/bin/

# Install plugin (creates a local marketplace + symlink)
claude plugins marketplace add /path/to/Herald/plugin
claude plugins install herald@herald-local

# Restart daemon
systemctl --user restart heraldd
```

### Deploy cycle

```bash
# 1. Build
cargo build --release

# 2. Install binaries (stop daemon first if running)
systemctl --user stop heraldd
cp target/release/herald target/release/heraldd ~/.local/bin/

# 3. Update plugin (if hook/command files changed)
claude plugins update herald@herald-local

# 4. Restart daemon
systemctl --user start heraldd
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
