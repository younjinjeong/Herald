#!/bin/bash
# Herald hook: PostToolUse
# Relays tool output to Telegram via Herald daemon

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOKEN=$(echo "$INPUT" | jq -r '.herald_token // empty')
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')
TOOL_INPUT=$(echo "$INPUT" | jq -r '.tool_input // {} | tostring' | head -c 500)
TOOL_RESPONSE=$(echo "$INPUT" | jq -r '.tool_response // {} | tostring' | head -c 1000)

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    --arg tool "$TOOL_NAME" \
    --arg tinput "$TOOL_INPUT" \
    --arg tresp "$TOOL_RESPONSE" \
    '{"type": "Output", "session_id": $sid, "token": $token, "tool_name": $tool, "tool_input_summary": $tinput, "tool_response_summary": $tresp}')

echo "$MSG" | herald ipc-send 2>/dev/null || true
