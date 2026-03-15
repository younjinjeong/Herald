#!/bin/bash
# Herald hooks: shared helper functions

# Skip all Herald hooks when running inside headless execution
# (prevents headless `claude --continue` from overwriting the original session)
if [ "$HERALD_HEADLESS" = "1" ]; then
    exit 0
fi

# Load config.env from plugin root (fallback for env vars)
# CLAUDE_PLUGIN_ROOT is set by Claude Code when running hooks
_herald_config="${CLAUDE_PLUGIN_ROOT:+${CLAUDE_PLUGIN_ROOT}/config.env}"
if [ -n "$_herald_config" ] && [ -f "$_herald_config" ]; then
    # Source only known variables (don't blindly source arbitrary code)
    _addr=$(grep -E '^HERALD_DAEMON_ADDR=' "$_herald_config" 2>/dev/null | head -1 | cut -d= -f2-)
    if [ -n "$_addr" ] && [ -z "$HERALD_DAEMON_ADDR" ]; then
        HERALD_DAEMON_ADDR="$_addr"
    fi
    unset _addr
fi
unset _herald_config

# Detect transport type (Unix socket = peercred auth, TCP = token auth)
herald_is_unix_transport() {
    [ -z "$HERALD_DAEMON_ADDR" ]
}

# Send IPC message and return the response
# Usage: RESPONSE=$(herald_ipc_send "$MSG")
herald_ipc_send() {
    local msg="$1"
    if [ -n "$HERALD_DAEMON_ADDR" ]; then
        echo "$msg" | herald ipc-send --tcp "$HERALD_DAEMON_ADDR" 2>/dev/null || true
    else
        echo "$msg" | herald ipc-send 2>/dev/null || true
    fi
}

# Read token from file (only needed for TCP transport)
# Sets TOKEN variable
herald_read_token() {
    local session_id="$1"
    TOKEN=""
    if ! herald_is_unix_transport; then
        local token_file="/tmp/herald/tokens/$session_id"
        if [ -f "$token_file" ]; then
            TOKEN=$(cat "$token_file")
        fi
    fi
}

# Re-register the session with the daemon (e.g. after daemon restart)
# Sets TOKEN variable on success
herald_reregister() {
    local session_id="$1"
    local pid="$$"

    # Walk up to find claude process
    local walk_pid=$pid
    while [ "$walk_pid" -gt 1 ]; do
        local parent_comm
        parent_comm=$(ps -o comm= -p "$walk_pid" 2>/dev/null)
        if [ "$parent_comm" = "claude" ]; then
            pid=$walk_pid
            break
        fi
        walk_pid=$(ps -o ppid= -p "$walk_pid" 2>/dev/null | tr -d ' ')
    done

    local cwd
    cwd=$(pwd)

    local msg
    if [ -n "$TMUX_PANE" ]; then
        msg=$(jq -n --arg sid "$session_id" --argjson pid "$pid" --arg cwd "$cwd" --arg tmux "$TMUX_PANE" \
            '{"type":"Register","session_id":$sid,"pid":$pid,"cwd":$cwd,"tmux_pane":$tmux}')
    else
        msg=$(jq -n --arg sid "$session_id" --argjson pid "$pid" --arg cwd "$cwd" \
            '{"type":"Register","session_id":$sid,"pid":$pid,"cwd":$cwd}')
    fi

    local response
    response=$(herald_ipc_send "$msg")

    local new_token
    new_token=$(echo "$response" | jq -r '.token // empty' 2>/dev/null)
    if [ -n "$new_token" ]; then
        # Only persist token for TCP transport
        if ! herald_is_unix_transport; then
            TOKEN_DIR="/tmp/herald/tokens"
            mkdir -p "$TOKEN_DIR"
            chmod 700 "$TOKEN_DIR"
            echo "$new_token" > "$TOKEN_DIR/$session_id"
            chmod 600 "$TOKEN_DIR/$session_id"
        fi
        TOKEN="$new_token"
        return 0
    fi
    return 1
}

# Send IPC message, re-register on 401, and retry once
# Usage: RESPONSE=$(herald_ipc_send_with_retry "$SESSION_ID" "$MSG")
herald_ipc_send_with_retry() {
    local session_id="$1"
    local msg="$2"

    local response
    response=$(herald_ipc_send "$msg")

    # Check for 401 (invalid token) or 410 (session not registered) — daemon was likely restarted
    local error_code
    error_code=$(echo "$response" | jq -r '.code // empty' 2>/dev/null)
    if [ "$error_code" = "401" ] || [ "$error_code" = "410" ]; then
        # Re-register to get a new token
        if herald_reregister "$session_id"; then
            # Update the token in the message and retry
            msg=$(echo "$msg" | jq --arg token "$TOKEN" '.token = $token')
            response=$(herald_ipc_send "$msg")
        fi
    fi

    echo "$response"
}
