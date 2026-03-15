---
description: Disconnect from remote Herald daemon and revert to local Unix socket
allowed-tools: [Bash]
---

# Disconnect from Remote Herald Daemon

Remove the remote daemon address and revert to using the local Unix socket.

1. Remove `HERALD_DAEMON_ADDR` from config.env:
```bash
PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$(dirname "$0")")}"
CONFIG_FILE="$PLUGIN_ROOT/config.env"

if [ -f "$CONFIG_FILE" ]; then
    grep -v '^HERALD_DAEMON_ADDR=' "$CONFIG_FILE" > "$CONFIG_FILE.tmp" && mv "$CONFIG_FILE.tmp" "$CONFIG_FILE"
    echo "Removed HERALD_DAEMON_ADDR from config.env"
else
    echo "No config.env found — already using local Unix socket"
fi
```

2. Confirm: "Herald hooks will now connect via local Unix socket."
