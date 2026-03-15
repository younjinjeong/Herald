#!/bin/bash
# Herald hook: Notification
# Relays notifications (permission requests, etc.) to Telegram

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOKEN=$(echo "$INPUT" | jq -r '.herald_token // empty')
NOTIF_TYPE=$(echo "$INPUT" | jq -r '.type // "unknown"')
MESSAGE=$(echo "$INPUT" | jq -r '.message // ""' | head -c 500)

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    --arg ntype "$NOTIF_TYPE" \
    --arg msg "$MESSAGE" \
    '{"type": "Notification", "session_id": $sid, "token": $token, "notification_type": $ntype, "message": $msg}')

if [ -n "$HERALD_DAEMON_ADDR" ]; then
    echo "$MSG" | herald ipc-send --tcp "$HERALD_DAEMON_ADDR" 2>/dev/null || true
else
    echo "$MSG" | herald ipc-send 2>/dev/null || true
fi
