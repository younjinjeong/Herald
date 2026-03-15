#!/bin/bash
# Herald hook: UserPromptSubmit
# Captures user prompt and sends to daemon for conversation logging

source "$(dirname "$0")/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
# Use original session ID in headless mode
[ -n "$HERALD_HEADLESS_SESSION" ] && SESSION_ID="$HERALD_HEADLESS_SESSION"

herald_read_token "$SESSION_ID"
PROMPT=$(echo "$INPUT" | jq -r '.prompt // empty')

if [ -z "$SESSION_ID" ] || [ -z "$PROMPT" ]; then
    exit 0
fi

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

if herald_is_unix_transport; then
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg etype "user_prompt" \
        --arg content "$PROMPT" \
        --arg ts "$TIMESTAMP" \
        '{"type": "ConversationEntry", "session_id": $sid, "entry_type": $etype, "content": $content, "timestamp": $ts}')
else
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        --arg etype "user_prompt" \
        --arg content "$PROMPT" \
        --arg ts "$TIMESTAMP" \
        '{"type": "ConversationEntry", "session_id": $sid, "token": $token, "entry_type": $etype, "content": $content, "timestamp": $ts}')
fi

herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null
