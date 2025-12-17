#!/bin/bash
# create-key.sh - Create API key for promo users

EMAIL="$1"
CREDITS="${2:-25}"
SECRET="YOUR_ADMIN_SECRET_HERE"
API_URL="https://yoursite.com/admin/create-key"

if [ -z "$EMAIL" ]; then
  echo "Usage: ./create-key.sh user@example.com [credits]"
  exit 1
fi

curl -s -X POST "$API_URL" \
  -H "Content-Type: application/json" \
  -d "{\"email\": \"$EMAIL\", \"initial_credits\": $CREDITS, \"secret\": \"$SECRET\"}" | jq .
