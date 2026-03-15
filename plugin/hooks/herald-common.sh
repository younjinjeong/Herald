#!/bin/bash
# Herald hooks: shared helper functions

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
        TOKEN_DIR="/tmp/herald/tokens"
        mkdir -p "$TOKEN_DIR"
        chmod 700 "$TOKEN_DIR"
        echo "$new_token" > "$TOKEN_DIR/$session_id"
        chmod 600 "$TOKEN_DIR/$session_id"
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

    # Check for 401 (invalid token — daemon was likely restarted)
    local error_code
    error_code=$(echo "$response" | jq -r '.code // empty' 2>/dev/null)
    if [ "$error_code" = "401" ]; then
        # Re-register to get a new token
        if herald_reregister "$session_id"; then
            # Update the token in the message and retry
            msg=$(echo "$msg" | jq --arg token "$TOKEN" '.token = $token')
            response=$(herald_ipc_send "$msg")
        fi
    fi

    echo "$response"
}
