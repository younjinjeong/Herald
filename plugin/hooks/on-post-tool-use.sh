#!/bin/bash
# Herald hook: PostToolUse — v2 with auto-retry

source "$(dirname "$0")/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

herald_read_token "$SESSION_ID"
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')
TOOL_INPUT=$(echo "$INPUT" | jq -r '.tool_input // {} | tostring' | head -c 500)
TOOL_RESPONSE=$(echo "$INPUT" | jq -r '.tool_response // {} | tostring' | head -c 1000)
TRANSCRIPT=$(echo "$INPUT" | jq -r '.transcript_path // empty')

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Send tool output (with auto re-register on 401)
if herald_is_unix_transport; then
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg tool "$TOOL_NAME" \
        --arg tinput "$TOOL_INPUT" \
        --arg tresp "$TOOL_RESPONSE" \
        '{"type": "Output", "session_id": $sid, "tool_name": $tool, "tool_input_summary": $tinput, "tool_response_summary": $tresp}')
else
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        --arg tool "$TOOL_NAME" \
        --arg tinput "$TOOL_INPUT" \
        --arg tresp "$TOOL_RESPONSE" \
        '{"type": "Output", "session_id": $sid, "token": $token, "tool_name": $tool, "tool_input_summary": $tinput, "tool_response_summary": $tresp}')
fi

herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null

# Extract token usage from transcript (if available)
if [ -n "$TRANSCRIPT" ] && [ -f "$TRANSCRIPT" ]; then
    # Re-read token (may have been updated by re-registration above)
    herald_read_token "$SESSION_ID"

    # Get the last line with usage data
    USAGE_LINE=$(grep '"usage"' "$TRANSCRIPT" 2>/dev/null | tail -1 || true)
    if [ -n "$USAGE_LINE" ]; then
        INPUT_TOKENS=$(echo "$USAGE_LINE" | jq -r '.usage.input_tokens // 0' 2>/dev/null || echo 0)
        OUTPUT_TOKENS=$(echo "$USAGE_LINE" | jq -r '.usage.output_tokens // 0' 2>/dev/null || echo 0)
        CACHE_READ=$(echo "$USAGE_LINE" | jq -r '.usage.cache_read_input_tokens // 0' 2>/dev/null || echo 0)
        CACHE_CREATE=$(echo "$USAGE_LINE" | jq -r '.usage.cache_creation_input_tokens // 0' 2>/dev/null || echo 0)

        # Estimate cost (Claude Sonnet 4 pricing: $3/M input, $15/M output)
        COST=$(echo "scale=6; ($INPUT_TOKENS * 0.000003) + ($OUTPUT_TOKENS * 0.000015)" | bc 2>/dev/null || echo "0")

        if herald_is_unix_transport; then
            TOKEN_MSG=$(jq -n \
                --arg sid "$SESSION_ID" \
                --argjson itok "${INPUT_TOKENS:-0}" \
                --argjson otok "${OUTPUT_TOKENS:-0}" \
                --argjson cread "${CACHE_READ:-0}" \
                --argjson ccreate "${CACHE_CREATE:-0}" \
                --argjson cost "${COST:-0}" \
                '{"type": "TokenUpdate", "session_id": $sid, "input_tokens": $itok, "output_tokens": $otok, "cache_read_tokens": $cread, "cache_creation_tokens": $ccreate, "total_cost_usd": $cost}')
        else
            TOKEN_MSG=$(jq -n \
                --arg sid "$SESSION_ID" \
                --arg token "$TOKEN" \
                --argjson itok "${INPUT_TOKENS:-0}" \
                --argjson otok "${OUTPUT_TOKENS:-0}" \
                --argjson cread "${CACHE_READ:-0}" \
                --argjson ccreate "${CACHE_CREATE:-0}" \
                --argjson cost "${COST:-0}" \
                '{"type": "TokenUpdate", "session_id": $sid, "token": $token, "input_tokens": $itok, "output_tokens": $otok, "cache_read_tokens": $cread, "cache_creation_tokens": $ccreate, "total_cost_usd": $cost}')
        fi

        herald_ipc_send "$TOKEN_MSG" >/dev/null
    fi
fi
