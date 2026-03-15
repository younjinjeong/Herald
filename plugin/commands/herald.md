---
description: Show Herald status and Telegram connection info
allowed-tools: [Bash]
---

# Herald Status

Check the Herald daemon status and display Telegram connection information.

Run: `herald status`

If Herald is not running, suggest: `herald setup` to configure, or `herald start` to start the daemon.

## Remote Daemon

If `HERALD_DAEMON_ADDR` is configured (check `${CLAUDE_PLUGIN_ROOT}/config.env`), show the current remote address.

- Use `/herald-connect <host:port>` to connect to a remote Herald daemon.
- Use `/herald-disconnect` to revert to the local Unix socket.
