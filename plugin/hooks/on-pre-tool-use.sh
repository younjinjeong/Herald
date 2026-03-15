#!/bin/bash
# Herald hook: PreToolUse
# Sends permission requests to Telegram and polls for approve/deny decisions.
# For AskUserQuestion: sends notification only (no blocking).

source "$(dirname "$0")/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty')
# Use original session ID in headless mode
[ -n "$HERALD_HEADLESS_SESSION" ] && SESSION_ID="$HERALD_HEADLESS_SESSION"

if [ -z "$SESSION_ID" ] || [ -z "$TOOL_NAME" ]; then
    exit 0
fi

herald_read_token "$SESSION_ID"

# --- AskUserQuestion: relay with structured options to Telegram ---
if [ "$TOOL_NAME" = "AskUserQuestion" ]; then
    QUESTION=$(echo "$INPUT" | jq -r '.tool_input.question // .tool_input // ""' | head -c 500)
    # Extract structured questions array for Telegram inline buttons
    QUESTIONS_JSON=$(echo "$INPUT" | jq -c '.tool_input.questions // []' 2>/dev/null)

    if herald_is_unix_transport; then
        MSG=$(jq -n \
            --arg sid "$SESSION_ID" \
            --arg ntype "ask_user_question" \
            --arg msg "$QUESTION" \
            --arg extras "$QUESTIONS_JSON" \
            '{"type": "Notification", "session_id": $sid, "notification_type": $ntype, "message": $msg, "extras": $extras}')
    else
        MSG=$(jq -n \
            --arg sid "$SESSION_ID" \
            --arg token "$TOKEN" \
            --arg ntype "ask_user_question" \
            --arg msg "$QUESTION" \
            --arg extras "$QUESTIONS_JSON" \
            '{"type": "Notification", "session_id": $sid, "token": $token, "notification_type": $ntype, "message": $msg, "extras": $extras}')
    fi

    herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null
    # Allow immediately — user can respond via Telegram buttons
    echo '{"hookSpecificOutput": {"permissionDecision": "allow"}}'
    exit 0
fi

# --- Query session modes ---
MODE_MSG=$(jq -n --arg sid "$SESSION_ID" '{"type": "ModeQuery", "session_id": $sid}')
MODE_RESPONSE=$(herald_ipc_send "$MODE_MSG" 2>/dev/null)
PLAN_MODE=$(echo "$MODE_RESPONSE" | jq -r '.plan_mode // false' 2>/dev/null)
BYPASS_PERMS=$(echo "$MODE_RESPONSE" | jq -r '.bypass_permissions // false' 2>/dev/null)

# Plan mode: deny mutating tools
if [ "$PLAN_MODE" = "true" ]; then
    echo '{"hookSpecificOutput": {"permissionDecision": "deny", "permissionDecisionReason": "Session is in Plan mode — no edits allowed. Use /sessions in Telegram to disable Plan mode."}}'
    exit 0
fi

# Bypass permissions: allow everything immediately, no Telegram prompt
if [ "$BYPASS_PERMS" = "true" ]; then
    echo '{"hookSpecificOutput": {"permissionDecision": "allow"}}'
    exit 0
fi

# --- Permission request for mutating tools ---
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input // {}' | head -c 1000)
REQUEST_ID="${SESSION_ID}_$(date +%s%N)"

if herald_is_unix_transport; then
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg rid "$REQUEST_ID" \
        --arg tool "$TOOL_NAME" \
        --arg tinput "$TOOL_INPUT" \
        '{"type": "PermissionRequest", "session_id": $sid, "request_id": $rid, "tool_name": $tool, "tool_input": $tinput}')
else
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        --arg rid "$REQUEST_ID" \
        --arg tool "$TOOL_NAME" \
        --arg tinput "$TOOL_INPUT" \
        '{"type": "PermissionRequest", "session_id": $sid, "token": $token, "request_id": $rid, "tool_name": $tool, "tool_input": $tinput}')
fi

# Send permission request to daemon
herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null

# Poll for decision (1s intervals, up to 30s)
ELAPSED=0
while [ "$ELAPSED" -lt 30 ]; do
    sleep 1
    ELAPSED=$((ELAPSED + 1))

    CHECK_MSG=$(jq -n --arg rid "$REQUEST_ID" '{"type": "PermissionCheck", "request_id": $rid}')
    RESPONSE=$(herald_ipc_send "$CHECK_MSG")

    DECISION=$(echo "$RESPONSE" | jq -r '.decision // "pending"' 2>/dev/null)
    if [ "$DECISION" = "allow" ]; then
        echo '{"hookSpecificOutput": {"permissionDecision": "allow"}}'
        exit 0
    elif [ "$DECISION" = "deny" ]; then
        echo '{"hookSpecificOutput": {"permissionDecision": "deny"}}'
        exit 0
    fi
    # "pending" — continue polling
done

# Timeout: auto-allow
echo '{"hookSpecificOutput": {"permissionDecision": "allow"}}'
