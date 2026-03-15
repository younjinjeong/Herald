#!/bin/bash
# Herald hook: SessionStart
# Registers a new Claude Code session with the Herald daemon

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Find the Claude process PID (ancestor of this hook shell)
# Walk up the process tree to find the 'claude' process
PID=$$
WALK_PID=$PID
while [ "$WALK_PID" -gt 1 ]; do
    PARENT_COMM=$(ps -o comm= -p "$WALK_PID" 2>/dev/null)
    if [ "$PARENT_COMM" = "claude" ]; then
        PID=$WALK_PID
        break
    fi
    WALK_PID=$(ps -o ppid= -p "$WALK_PID" 2>/dev/null | tr -d ' ')
done
CWD=$(pwd)

# Build IPC register message (include tmux pane if running in tmux)
if [ -n "$TMUX_PANE" ]; then
    MSG=$(jq -n --arg sid "$SESSION_ID" --argjson pid "$PID" --arg cwd "$CWD" --arg tmux "$TMUX_PANE" \
        '{"type":"Register","session_id":$sid,"pid":$pid,"cwd":$cwd,"tmux_pane":$tmux}')
else
    MSG=$(jq -n --arg sid "$SESSION_ID" --argjson pid "$PID" --arg cwd "$CWD" \
        '{"type":"Register","session_id":$sid,"pid":$pid,"cwd":$cwd}')
fi

# Send to daemon and capture response to extract token
if [ -n "$HERALD_DAEMON_ADDR" ]; then
    RESPONSE=$(echo "$MSG" | herald ipc-send --tcp "$HERALD_DAEMON_ADDR" 2>/dev/null) || true
else
    RESPONSE=$(echo "$MSG" | herald ipc-send 2>/dev/null) || true
fi

# Extract and persist the session token for use by subsequent hooks
HERALD_TOKEN=$(echo "$RESPONSE" | jq -r '.token // empty' 2>/dev/null)
if [ -n "$HERALD_TOKEN" ]; then
    TOKEN_DIR="/tmp/herald/tokens"
    mkdir -p "$TOKEN_DIR"
    chmod 700 "$TOKEN_DIR"
    echo "$HERALD_TOKEN" > "$TOKEN_DIR/$SESSION_ID"
    chmod 600 "$TOKEN_DIR/$SESSION_ID"
fi

# Output context for Claude
echo '{"hookSpecificOutput": {"additionalContext": "Herald: session registered for Telegram relay"}}'
