#!/bin/bash
# Herald hook: UserPromptSubmit
# Captures user prompt and sends to daemon for conversation logging

source "/home/younjinjeong/.config/herald/plugin/hooks/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

# Read persisted token from file
TOKEN=""
if [ -n "$SESSION_ID" ] && [ -f "/tmp/herald/tokens/$SESSION_ID" ]; then
    TOKEN=$(cat "/tmp/herald/tokens/$SESSION_ID")
fi
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

herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null
