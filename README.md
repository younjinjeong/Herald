# Herald

**Claude Code Telegram Remote Control**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange.svg)](https://www.rust-lang.org/)

Herald bridges Claude Code and Telegram -- monitor sessions, approve tool executions, and send prompts from your phone. Runs as a lightweight daemon with outbound-only connections (no inbound ports needed).

- **Permission gating** -- approve/deny Bash, Write, Edit from Telegram
- **Multi-session** -- color-coded tags, token usage tracking, cost estimates
- **Headless control** -- send prompts to Claude Code from Telegram
- **Multi-machine** -- connect remote Claude Code sessions via TCP

---

## How It Works

```
Claude Code ──[hooks]──> heraldd ──[HTTPS]──> Telegram Bot ──> Your Phone
               IPC          |
          (Unix / TCP)      +-- Session Registry
                            +-- Permission Gate
                            +-- Token Monitor
```

Plugin hooks fire on Claude Code events (session start, tool use, prompts). The daemon (`heraldd`) forwards formatted messages to Telegram. You can monitor, approve tools, and send prompts back.

---

## Quick Start

### 1. Build & install

```bash
git clone https://github.com/younjinjeong/Herald.git
cd Herald
cargo build --release
cp target/release/{herald,heraldd} ~/.local/bin/
```

### 2. Create a Telegram bot

Message [@BotFather](https://t.me/BotFather) on Telegram, send `/newbot`, and copy the token.

### 3. Run setup

```bash
herald setup
```

This stores the bot token in your system keyring and verifies your Telegram account via a 6-digit OTP.

### 4. Start the daemon

```bash
herald start
```

### 5. Connect Claude Code

```bash
# Option A: Install as plugin (persistent)
claude plugins marketplace add /path/to/Herald/plugin
claude plugins install herald@herald-local
claude

# Option B: Load directly (development)
claude --plugin-dir /path/to/Herald/plugin
```

You'll see **Session started** in Telegram. Use `/sessions` to select and interact.

---

## What You'll See

```
🟢 [my-project] Session started
📁 ~/projects/my-project

🟢 [my-project] 🔨 Working on:
> Fix the auth bug in login.rs

🟢 [my-project] 🔐 Permission request
⚙️ Tool: Bash
  git status
  [ ✅ Approve ]  [ ❌ Deny ]

🟢 [my-project] ✅ Done (3 tools)
━━━━━━━━━━━━━━━━━━━━
📊 1.2K in / 450 out · $0.0120
━━━━━━━━━━━━━━━━━━━━
Fixed the token validation -- it was skipping
the expiry check.

🟢 [my-project] Session ended
```

---

## Telegram Commands

| Command | Description |
|---------|-------------|
| `/sessions` | List active sessions, tap to select |
| `/status` | Daemon uptime and connection info |
| `/tokens` | Token usage and cost per session |
| `/log` | Recent conversation for selected session |
| `/bypass` | Toggle permission bypass globally |

**Sending prompts:** Type a message to send to the active session. Use `@name` prefix or reply to a tagged message to target other sessions.

---

## Multi-Machine Setup

On the daemon host, enable TCP in `~/.config/herald/config.toml`:

```toml
[daemon]
transport = "both"
listen_addr = "0.0.0.0:7272"
```

On remote machines, inside Claude Code:

```
/herald-connect 192.168.1.100:7272
```

---

<details>
<summary><strong>Configuration Reference</strong></summary>

Config file: `~/.config/herald/config.toml`

| Section | Key | Default | Description |
|---------|-----|---------|-------------|
| `[daemon]` | `transport` | `unix` | `unix`, `tcp`, or `both` |
| | `socket_path` | `$XDG_RUNTIME_DIR/herald/herald.sock` | Unix socket |
| | `listen_addr` | `0.0.0.0:7272` | TCP address |
| | `log_output` | `file` | `file`, `stdout`, or `both` |
| `[sessions]` | `debounce_seconds` | `15` | Idle time before completion summary |
| | `default_bypass_permissions` | `false` | Auto-allow all tools |
| | `max_concurrent` | `10` | Max simultaneous sessions |
| `[output_filter]` | `mask_secrets` | `true` | Redact API keys and passwords |
| | `max_message_length` | `4096` | Telegram message limit |

**Environment variables:**

| Variable | Description |
|----------|-------------|
| `HERALD_BOT_TOKEN` | Bot token (overrides keyring) |
| `HERALD_DAEMON_ADDR` | Remote daemon address (`host:7272`) |
| `HERALD_CONTAINER` | Container mode (stdout logging, token-only auth) |

</details>

<details>
<summary><strong>Prerequisites</strong></summary>

- Rust 1.75+ (or Docker)
- Linux / macOS / WSL2
- Telegram bot token from @BotFather
- Claude Code installed (`claude` in PATH)
- `jq` (used by hook scripts)

</details>

---

## License

[MIT](LICENSE)
