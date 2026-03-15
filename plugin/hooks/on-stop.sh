#!/bin/bash
# Herald hook: Stop
# Sends assistant response as conversation entry and notifies daemon

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOKEN=$(echo "$INPUT" | jq -r '.herald_token // empty')
LAST_MSG=$(echo "$INPUT" | jq -r '.last_assistant_message // .last_message // ""' | head -c 2000)

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Send session stopped
MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    --arg lmsg "$LAST_MSG" \
    '{"type": "SessionStopped", "session_id": $sid, "token": $token, "last_message": $lmsg}')

echo "$MSG" | herald ipc-send 2>/dev/null || true

# Send assistant response as conversation entry
if [ -n "$LAST_MSG" ]; then
    TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    CONV_MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        --arg etype "assistant_response" \
        --arg content "$LAST_MSG" \
        --arg ts "$TIMESTAMP" \
        '{"type": "ConversationEntry", "session_id": $sid, "token": $token, "entry_type": $etype, "content": $content, "timestamp": $ts}')

    echo "$CONV_MSG" | herald ipc-send 2>/dev/null || true
fi
