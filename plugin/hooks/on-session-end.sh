#!/bin/bash
# Herald hook: SessionEnd
# Unregisters a Claude Code session from the Herald daemon

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOKEN=$(echo "$INPUT" | jq -r '.herald_token // empty')

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    '{"type": "Unregister", "session_id": $sid, "token": $token}')

echo "$MSG" | herald ipc-send 2>/dev/null || true
