#!/bin/bash
# Herald hook: Stop
# Sends assistant response as conversation entry and notifies daemon

source "/home/younjinjeong/.config/herald/plugin/hooks/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

# Read persisted token from file
TOKEN=""
if [ -n "$SESSION_ID" ] && [ -f "/tmp/herald/tokens/$SESSION_ID" ]; then
    TOKEN=$(cat "/tmp/herald/tokens/$SESSION_ID")
fi
LAST_MSG=$(echo "$INPUT" | jq -r '.last_assistant_message // .last_message // ""' | head -c 2000)

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Send assistant response as conversation entry FIRST (while session is still registered)
if [ -n "$LAST_MSG" ]; then
    TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    CONV_MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        --arg etype "assistant_response" \
        --arg content "$LAST_MSG" \
        --arg ts "$TIMESTAMP" \
        '{"type": "ConversationEntry", "session_id": $sid, "token": $token, "entry_type": $etype, "content": $content, "timestamp": $ts}')

    herald_ipc_send_with_retry "$SESSION_ID" "$CONV_MSG" >/dev/null

    # Re-read token (may have been updated by re-registration)
    if [ -f "/tmp/herald/tokens/$SESSION_ID" ]; then
        TOKEN=$(cat "/tmp/herald/tokens/$SESSION_ID")
    fi
fi

# Send session stopped AFTER conversation entry (so token is still valid above)
MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    --arg lmsg "$LAST_MSG" \
    '{"type": "SessionStopped", "session_id": $sid, "token": $token, "last_message": $lmsg}')

herald_ipc_send "$MSG" >/dev/null
