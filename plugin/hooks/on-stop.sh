#!/bin/bash
# Herald hook: Stop
# Sends assistant response as conversation entry and notifies daemon

source "$(dirname "$0")/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

herald_read_token "$SESSION_ID"
LAST_MSG=$(echo "$INPUT" | jq -r '.last_assistant_message // .last_message // ""' | head -c 2000)

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Send assistant response as conversation entry FIRST (while session is still registered)
if [ -n "$LAST_MSG" ]; then
    TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    if herald_is_unix_transport; then
        CONV_MSG=$(jq -n \
            --arg sid "$SESSION_ID" \
            --arg etype "assistant_response" \
            --arg content "$LAST_MSG" \
            --arg ts "$TIMESTAMP" \
            '{"type": "ConversationEntry", "session_id": $sid, "entry_type": $etype, "content": $content, "timestamp": $ts}')
    else
        CONV_MSG=$(jq -n \
            --arg sid "$SESSION_ID" \
            --arg token "$TOKEN" \
            --arg etype "assistant_response" \
            --arg content "$LAST_MSG" \
            --arg ts "$TIMESTAMP" \
            '{"type": "ConversationEntry", "session_id": $sid, "token": $token, "entry_type": $etype, "content": $content, "timestamp": $ts}')
    fi

    herald_ipc_send_with_retry "$SESSION_ID" "$CONV_MSG" >/dev/null

    # Re-read token (may have been updated by re-registration)
    herald_read_token "$SESSION_ID"
fi

# Send session stopped AFTER conversation entry (so token is still valid above)
if herald_is_unix_transport; then
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg lmsg "$LAST_MSG" \
        '{"type": "SessionStopped", "session_id": $sid, "last_message": $lmsg}')
else
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        --arg lmsg "$LAST_MSG" \
        '{"type": "SessionStopped", "session_id": $sid, "token": $token, "last_message": $lmsg}')
fi

herald_ipc_send "$MSG" >/dev/null
