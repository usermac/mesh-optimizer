#!/bin/bash

################################################################################
# Send HTML Email Report via Resend API
################################################################################
# Usage:
#   ./send_html_report.sh "Subject" "<html>Body Content</html>"
#
# Description:
#   Sends an HTML email to the configured BACKUP_EMAIL using the Resend API.
#   Uses Python to safely construct the JSON payload, preventing issues with
#   escaping special characters in the body text.
################################################################################

set -euo pipefail

ENV_FILE="/root/mesh-optimizer/.env"

# Load environment variables
if [ -f "$ENV_FILE" ]; then
    set -a
    source "$ENV_FILE"
    set +a
fi

# Validate arguments
if [ "$#" -lt 2 ]; then
    echo "Usage: $0 <subject> <html_body>" >&2
    exit 1
fi

SUBJECT="$1"
HTML_BODY="$2"

# Validate configuration
if [[ -z "${RESEND_API_KEY:-}" ]] || [[ -z "${BACKUP_EMAIL:-}" ]]; then
    echo "Error: RESEND_API_KEY or BACKUP_EMAIL not set in environment or $ENV_FILE" >&2
    exit 1
fi

# Construct JSON payload using Python
PAYLOAD=$(python3 -c "
import json, sys, os

try:
    payload = {
        'from': 'Mesh Optimizer <support@webdeliveryengine.com>',
        'to': [os.environ.get('BACKUP_EMAIL')],
        'subject': sys.argv[1],
        'html': sys.argv[2]
    }
    print(json.dumps(payload))
except Exception as e:
    sys.stderr.write(f'Error constructing JSON: {e}')
    sys.exit(1)
" "$SUBJECT" "$HTML_BODY")

# Send via Curl
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST \
    "https://api.resend.com/emails" \
    -H "Authorization: Bearer $RESEND_API_KEY" \
    -H "Content-Type: application/json" \
    -d "$PAYLOAD")

# Parse response
HTTP_CODE=$(echo "$RESPONSE" | tail -n1)
CONTENT=$(echo "$RESPONSE" | sed '$d')

if [[ "$HTTP_CODE" == "200" ]]; then
    echo "Email sent successfully to $BACKUP_EMAIL"
else
    echo "Failed to send email (HTTP $HTTP_CODE)" >&2
    echo "Response: $CONTENT" >&2
    exit 1
fi
