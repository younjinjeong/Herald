#!/bin/bash
# Herald hook: UserPromptSubmit
# Captures user prompt and sends to daemon for conversation logging

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOKEN=$(echo "$INPUT" | jq -r '.herald_token // empty')
PROMPT=$(echo "$INPUT" | jq -r '.prompt // empty')

if [ -z "$SESSION_ID" ] || [ -z "$PROMPT" ]; then
    exit 0
fi

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    --arg etype "user_prompt" \
    --arg content "$PROMPT" \
    --arg ts "$TIMESTAMP" \
    '{"type": "ConversationEntry", "session_id": $sid, "token": $token, "entry_type": $etype, "content": $content, "timestamp": $ts}')

if [ -n "$HERALD_DAEMON_ADDR" ]; then
    echo "$MSG" | herald ipc-send --tcp "$HERALD_DAEMON_ADDR" 2>/dev/null || true
else
    echo "$MSG" | herald ipc-send 2>/dev/null || true
fi
