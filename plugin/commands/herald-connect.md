---
description: Connect to a remote Herald daemon
argument-hint: <host:port>
allowed-tools: [Bash]
---

# Connect to Remote Herald Daemon

Configure this Claude Code instance to relay messages to a remote Herald daemon at the specified address.

1. Write the daemon address to config.env:
```bash
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}"
CONFIG_FILE="$PLUGIN_ROOT/config.env"

# Create or update config.env with the daemon address
if [ -f "$CONFIG_FILE" ]; then
    # Remove existing HERALD_DAEMON_ADDR line if present
    grep -v '^HERALD_DAEMON_ADDR=' "$CONFIG_FILE" > "$CONFIG_FILE.tmp" && mv "$CONFIG_FILE.tmp" "$CONFIG_FILE"
fi
echo "HERALD_DAEMON_ADDR=$ARGUMENTS" >> "$CONFIG_FILE"
```

2. Test connectivity by sending a Health request:
```bash
echo '{"type":"Health"}' | herald ipc-send --tcp "$ARGUMENTS" 2>&1
```

3. If the Health check succeeds (returns `HealthStatus`), report success. If it fails, warn the user but keep the config (they may start the daemon later).
