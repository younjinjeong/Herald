#!/bin/bash
# Herald hook: Stop
# Notifies daemon that the session has stopped

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOKEN=$(echo "$INPUT" | jq -r '.herald_token // empty')
LAST_MSG=$(echo "$INPUT" | jq -r '.last_message // ""' | head -c 500)

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    --arg lmsg "$LAST_MSG" \
    '{"type": "SessionStopped", "session_id": $sid, "token": $token, "last_message": $lmsg}')

echo "$MSG" | herald ipc-send 2>/dev/null || true
