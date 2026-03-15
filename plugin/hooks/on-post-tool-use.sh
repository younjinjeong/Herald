#!/bin/bash
# Herald hook: PostToolUse
# Relays tool output to Telegram via Herald daemon
# Also extracts token usage from transcript

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')
TOKEN=$(echo "$INPUT" | jq -r '.herald_token // empty')
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')
TOOL_INPUT=$(echo "$INPUT" | jq -r '.tool_input // {} | tostring' | head -c 500)
TOOL_RESPONSE=$(echo "$INPUT" | jq -r '.tool_response // {} | tostring' | head -c 1000)
TRANSCRIPT=$(echo "$INPUT" | jq -r '.transcript_path // empty')

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Send tool output
MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    --arg tool "$TOOL_NAME" \
    --arg tinput "$TOOL_INPUT" \
    --arg tresp "$TOOL_RESPONSE" \
    '{"type": "Output", "session_id": $sid, "token": $token, "tool_name": $tool, "tool_input_summary": $tinput, "tool_response_summary": $tresp}')

if [ -n "$HERALD_DAEMON_ADDR" ]; then
    echo "$MSG" | herald ipc-send --tcp "$HERALD_DAEMON_ADDR" 2>/dev/null || true
else
    echo "$MSG" | herald ipc-send 2>/dev/null || true
fi

# Extract token usage from transcript (if available)
if [ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ]; then
    # Get the last line with usage data
    USAGE_LINE=$(tac "$TRANSCRIPT" 2>/dev/null | grep -m1 '"usage"' || true)
    if [ -n "$USAGE_LINE" ]; then
        INPUT_TOKENS=$(echo "$USAGE_LINE" | jq -r '.usage.input_tokens // 0' 2>/dev/null || echo 0)
        OUTPUT_TOKENS=$(echo "$USAGE_LINE" | jq -r '.usage.output_tokens // 0' 2>/dev/null || echo 0)
        CACHE_READ=$(echo "$USAGE_LINE" | jq -r '.usage.cache_read_input_tokens // 0' 2>/dev/null || echo 0)
        CACHE_CREATE=$(echo "$USAGE_LINE" | jq -r '.usage.cache_creation_input_tokens // 0' 2>/dev/null || echo 0)

        # Estimate cost (Claude Sonnet 4 pricing: $3/M input, $15/M output)
        COST=$(echo "scale=6; ($INPUT_TOKENS * 0.000003) + ($OUTPUT_TOKENS * 0.000015)" | bc 2>/dev/null || echo "0")

        TOKEN_MSG=$(jq -n \
            --arg sid "$SESSION_ID" \
            --arg token "$TOKEN" \
            --argjson itok "${INPUT_TOKENS:-0}" \
            --argjson otok "${OUTPUT_TOKENS:-0}" \
            --argjson cread "${CACHE_READ:-0}" \
            --argjson ccreate "${CACHE_CREATE:-0}" \
            --argjson cost "${COST:-0}" \
            '{"type": "TokenUpdate", "session_id": $sid, "token": $token, "input_tokens": $itok, "output_tokens": $otok, "cache_read_tokens": $cread, "cache_creation_tokens": $ccreate, "total_cost_usd": $cost}')

        echo "$TOKEN_MSG" | herald ipc-send 2>/dev/null || true
    fi
fi
