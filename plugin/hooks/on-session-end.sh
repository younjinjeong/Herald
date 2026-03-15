#!/bin/bash
# Herald hook: SessionEnd
# Unregisters a Claude Code session from the Herald daemon

source "/home/younjinjeong/.config/herald/plugin/hooks/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Read persisted token from file
TOKEN_FILE="/tmp/herald/tokens/$SESSION_ID"
TOKEN=""
if [ -f "$TOKEN_FILE" ]; then
    TOKEN=$(cat "$TOKEN_FILE")
fi

if [ -z "$TOKEN" ]; then
    exit 0
fi

MSG=$(jq -n \
    --arg sid "$SESSION_ID" \
    --arg token "$TOKEN" \
    '{"type": "Unregister", "session_id": $sid, "token": $token}')

herald_ipc_send "$MSG" >/dev/null

# Clean up token file
rm -f "$TOKEN_FILE" 2>/dev/null
