#!/bin/bash
# Herald hook: SessionEnd
# Unregisters a Claude Code session from the Herald daemon

source "$(dirname "$0")/herald-common.sh"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty')

if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Read token (only needed for TCP transport)
herald_read_token "$SESSION_ID"

if herald_is_unix_transport; then
    # Unix transport: no token needed (peercred auth)
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        '{"type": "Unregister", "session_id": $sid}')
else
    if [ -z "$TOKEN" ]; then
        exit 0
    fi
    MSG=$(jq -n \
        --arg sid "$SESSION_ID" \
        --arg token "$TOKEN" \
        '{"type": "Unregister", "session_id": $sid, "token": $token}')
fi

herald_ipc_send_with_retry "$SESSION_ID" "$MSG" >/dev/null

# Clean up token file (if it exists)
rm -f "/tmp/herald/tokens/$SESSION_ID" 2>/dev/null
