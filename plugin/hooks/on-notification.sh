#!/bin/bash
# Herald hook: Notification
# Relays notifications (permission requests, etc.) to Telegram

source "$(dirname "$0")/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

herald_read_token "$SESSION_ID"
NOTIF_TYPE=$(echo "$INPUT" | jq -r '.type // "unknown"')
MESSAGE=$(echo "$INPUT" | jq -r '.message // ""' | head -c 500)

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

if herald_is_unix_transport; then
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg ntype "$NOTIF_TYPE" \
        --arg msg "$MESSAGE" \
        '{"type": "Notification", "session_id": $sid, "notification_type": $ntype, "message": $msg}')
else
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        --arg ntype "$NOTIF_TYPE" \
        --arg msg "$MESSAGE" \
        '{"type": "Notification", "session_id": $sid, "token": $token, "notification_type": $ntype, "message": $msg}')
fi

herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null
