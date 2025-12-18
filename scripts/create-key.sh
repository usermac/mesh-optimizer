#!/bin/bash
# create-key.sh - Grant credits to users (creates key if new, adds credits if existing)

# Load environment variables from .env file
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE="$SCRIPT_DIR/../.env"

if [ -f "$ENV_FILE" ]; then
  set -a
  source "$ENV_FILE"
  set +a
else
  echo "Warning: .env file not found at $ENV_FILE"
fi

EMAIL="$1"
CREDITS="${2:-25}"
SECRET="${ADMIN_SECRET:-}"
API_URL="${API_BASE_URL:-https://webdeliveryengine.com}/admin/grant-credits"

if [ -z "$EMAIL" ]; then
  echo "Usage: ./create-key.sh user@example.com [credits]"
  echo ""
  echo "  - New users: creates account with API key, sends welcome email"
  echo "  - Existing users: adds credits, sends confirmation email"
  exit 1
fi

if [ -z "$SECRET" ]; then
  echo "Error: ADMIN_SECRET not set in .env file"
  exit 1
fi

RESPONSE=$(curl -s -X POST "$API_URL" \
  -H "Content-Type: application/json" \
  -d "{\"email\": \"$EMAIL\", \"initial_credits\": $CREDITS, \"secret\": \"$SECRET\"}")

# Parse and display result
ACTION=$(echo "$RESPONSE" | jq -r '.action // empty')

if [ "$ACTION" = "created_key" ]; then
  echo "NEW USER created for $EMAIL"
  echo "$RESPONSE" | jq .
elif [ "$ACTION" = "added_credits" ]; then
  NEW_BALANCE=$(echo "$RESPONSE" | jq -r '.new_balance')
  echo "EXISTING USER - Added $CREDITS credits to $EMAIL (new balance: $NEW_BALANCE)"
  echo "$RESPONSE" | jq .
else
  echo "Error or unexpected response:"
  echo "$RESPONSE" | jq .
fi
