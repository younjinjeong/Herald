# Herald

**Claude Code Telegram Remote Control**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange.svg)](https://www.rust-lang.org/)

Herald relays Claude Code session I/O to Telegram, letting you monitor and control Claude Code from your phone. It runs as a Linux daemon (`heraldd`) that connects to Telegram via long polling — no inbound ports required.

---

## Architecture

```
┌────────────────────────────────────────────────────────┐
│                  Developer's Machine                   │
│                                                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │ Claude Code  │  │ Claude Code  │  │ Claude Code  │ │
│  │  Session #1  │  │  Session #2  │  │  Session #N  │ │
│  │  (Plugin)    │  │  (Plugin)    │  │  (Plugin)    │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘ │
│         │  Hook scripts   │                 │          │
│         └────────────────►├◄────────────────┘          │
│                           │                            │
│                  Unix Domain Socket                    │
│                           │                            │
│                ┌──────────▼──────────┐                 │
│                │      heraldd       │                 │
│                │  (systemd service) │                 │
│                │                    │                 │
│                │  Session Registry  │                 │
│                │  Message Queue     │                 │
│                │  Content Filter    │                 │
│                └──────────┬─────────┘                 │
│                           │                            │
└───────────────────────────┼────────────────────────────┘
                            │ HTTPS (outbound only)
                            ▼
                  ┌──────────────────┐
                  │ Telegram Bot API │
                  │  (Long Polling)  │
                  └────────┬─────────┘
                           │
                           ▼
                  ┌──────────────────┐
                  │ Telegram Mobile  │
                  └──────────────────┘
```

## Features

- **Setup wizard** — guided setup with OTP-based Telegram verification
- **Multi-session** — monitor multiple Claude Code sessions simultaneously
- **Outbound only** — works behind firewalls, no inbound ports needed
- **Secure** — bot token in system keyring, Unix socket with `SO_PEERCRED`, content filtering
- **Headless control** — send prompts to Claude Code from Telegram via `claude -p`
- **PTY injection** — inject input into existing interactive sessions
- **systemd integration** — runs as a user service with security hardening
- **Plugin hooks** — automatic session registration via Claude Code plugin system

## Prerequisites

- **Rust** 1.75+ (2024 edition)
- **Linux** (WSL2 works) with systemd
- **Telegram Bot** token from [@BotFather](https://t.me/BotFather)
- **Claude Code** installed and accessible in `$PATH`
- **jq** (used by plugin hook scripts)

## Installation

### Build from source

```bash
git clone https://github.com/younjinjeong/Herald.git
cd Herald
cargo build --release

# Install binaries to ~/.local/bin
cp target/release/herald target/release/heraldd ~/.local/bin/
```

### Install systemd service (optional)

```bash
mkdir -p ~/.config/systemd/user
cp systemd/heraldd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable heraldd
```

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

Or via systemd:

```bash
systemctl --user start heraldd
```

### 4. Use from Telegram

Send `/start` to your bot — you're connected.

## Telegram Commands

| Command | Description |
|---------|-------------|
| `/start` | Connect and show auth status |
| `/sessions` | List active Claude Code sessions (with selection buttons) |
| `/status` | Show daemon uptime, session count, connection status |
| `/help` | Command reference |

Send any **text message** to forward it as a prompt to the selected Claude Code session.

## Claude Code Plugin

Load the Herald plugin when starting Claude Code:

```bash
claude --plugin-dir /path/to/Herald/plugin
```

The plugin automatically:
- Registers the session with `heraldd` on start
- Relays tool output (Bash, Write, Edit, Read) to Telegram
- Forwards notifications and permission requests
- Unregisters the session on exit

### Plugin hooks

| Event | Script | Behavior |
|-------|--------|----------|
| `SessionStart` | `on-session-start.sh` | Registers session with daemon |
| `SessionEnd` | `on-session-end.sh` | Unregisters session |
| `PostToolUse` | `on-post-tool-use.sh` | Relays tool output (async) |
| `Notification` | `on-notification.sh` | Relays notifications (async) |
| `Stop` | `on-stop.sh` | Sends session stopped event |

## Configuration

Config file: `~/.config/herald/config.toml`

```toml
[daemon]
socket_path = "/run/user/1000/herald/herald.sock"
log_level = "INFO"

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

## CLI Reference

```
herald setup     # Interactive setup wizard
herald start     # Start the daemon
herald stop      # Stop the daemon
herald status    # Show daemon and session status
herald send <session> <message>   # Send prompt to a session
```

## Security

- **Bot token**: stored in OS keyring via `libsecret` / GNOME Keyring — never in plaintext
- **IPC authentication**: Unix socket with `SO_PEERCRED` — only same-UID processes can connect
- **Session tokens**: UUID v4, validated on every IPC message, invalidated on daemon restart
- **OTP verification**: 6-digit code, 5-minute TTL, 3 attempt limit
- **Content filtering**: API keys, tokens, passwords automatically redacted before relay
- **systemd hardening**: `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=read-only`, `PrivateTmp`

## Project Structure

```
Herald/
├── crates/
│   ├── herald-core/        # Shared library
│   │   └── src/
│   │       ├── config.rs       # TOML config + keyring
│   │       ├── ipc/            # Unix socket protocol
│   │       ├── auth/           # OTP + chat_id auth
│   │       ├── telegram/       # Bot, commands, handlers
│   │       ├── session/        # Registry + tokens
│   │       └── security/       # SO_PEERCRED + content filter
│   ├── herald-cli/         # CLI binary (herald)
│   │   └── src/commands/       # setup, start, stop, status, send
│   └── herald-daemon/      # Daemon binary (heraldd)
│       └── src/
│           ├── service.rs      # IPC + Telegram orchestration
│           ├── headless.rs     # claude -p execution
│           ├── pty.rs          # PTY stdin injection
│           └── queue.rs        # Rate-limited message queue
├── plugin/                 # Claude Code plugin
│   ├── .claude-plugin/
│   ├── hooks/                  # Shell scripts for each event
│   └── commands/               # /herald slash command
└── systemd/
    └── heraldd.service     # systemd user unit
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Check without building
cargo check

# Lint
cargo clippy
```

### Key dependencies

| Crate | Purpose |
|-------|---------|
| [teloxide](https://github.com/teloxide/teloxide) | Telegram Bot API |
| [tokio](https://tokio.rs) | Async runtime |
| [clap](https://github.com/clap-rs/clap) | CLI argument parsing |
| [nix](https://github.com/nix-rust/nix) | Unix socket credentials |
| [keyring](https://github.com/hwchen/keyring-rs) | Secure token storage |

## License

[MIT](LICENSE)
