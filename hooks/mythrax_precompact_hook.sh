#!/bin/bash
# Mythrax PreCompact Emergency-Save Hook
# Safe under Bash 3.2+ and macOS

# Strict umask for safe temporary file creation
umask 077

if [ -z "$MYTHRAX_TOKEN" ]; then
    echo "Error: MYTHRAX_TOKEN environment variable is not set." >&2
    exit 1
fi

DAEMON_PORT="${MYTHRAX_DAEMON_PORT:-8090}"
DAEMON_URL="http://127.0.0.1:${DAEMON_PORT}/v1/hooks/precompact"

# Create a temporary file to hold the payload securely
TEMP_PAYLOAD=$(mktemp -t mythrax_precompact.XXXXXX) || exit 1
trap 'rm -f "$TEMP_PAYLOAD"' EXIT

# Read stdin with a byte cap of 5MB to prevent memory exhaustion
head -c 5242880 > "$TEMP_PAYLOAD"

# Perform the HTTP call
RESPONSE=$(curl -s -w "\n%{http_code}" \
    -H "X-Mythrax-Token: $MYTHRAX_TOKEN" \
    -H "Content-Type: application/json" \
    -d @"$TEMP_PAYLOAD" \
    "$DAEMON_URL")

EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
    echo "Error: Failed to connect to Mythrax daemon at $DAEMON_URL (curl exit code $EXIT_CODE)." >&2
    exit 0 # Non-blocking for the host agent
fi

# Parse HTTP status code from the last line
HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | sed '$d')

if [ "$HTTP_STATUS" -ne 200 ]; then
    echo "Error: Mythrax daemon returned HTTP status $HTTP_STATUS." >&2
    echo "Response: $BODY" >&2
    exit 0 # Non-blocking
fi

echo "Mythrax PreCompact Hook successfully completed."
exit 0
