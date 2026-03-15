#!/bin/bash
# Herald hook: SessionStart
# Registers a new Claude Code session with the Herald daemon

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
if [ -z "$SESSION_ID" ]; then
    exit 0
fi

PID=$$
CWD=$(pwd)

# Build IPC register message
MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --argjson pid "$PID" \
    --arg cwd "$CWD" \
    '{"type": "Register", "session_id": $sid, "pid": $pid, "cwd": $cwd}')

# Send to daemon via herald CLI
# Send to daemon (TCP if HERALD_DAEMON_ADDR set, else Unix socket)
if [ -n "$HERALD_DAEMON_ADDR" ]; then
    echo "$MSG" | herald ipc-send --tcp "$HERALD_DAEMON_ADDR" 2>/dev/null || true
else
    echo "$MSG" | herald ipc-send 2>/dev/null || true
fi

# Output context for Claude
echo '{"hookSpecificOutput": {"additionalContext": "Herald: session registered for Telegram relay"}}'
