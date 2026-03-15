#!/bin/bash
# Herald hook: Notification
# Relays notifications (permission requests, etc.) to Telegram

source "/home/younjinjeong/.config/herald/plugin/hooks/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

# Read persisted token from file
TOKEN=""
if [ -n "$SESSION_ID" ] && [ -f "/tmp/herald/tokens/$SESSION_ID" ]; then
    TOKEN=$(cat "/tmp/herald/tokens/$SESSION_ID")
fi
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

herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null
