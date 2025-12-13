#!/bin/bash
# ship_pricing.sh - Deploys pricing changes to production.
#
# Usage: ./ship_pricing.sh
#
# This script syncs only the pricing.json file to the server.
# Thanks to hot-reload, pricing changes take effect immediately
# without restarting the server or interrupting running jobs.

set -euo pipefail

# Configuration
SERVER="root@www.webdeliveryengine.com"
REMOTE_DIR="/root/mesh-optimizer"
PRICING_FILE="server/pricing.json"

echo "🚀 Deploying Pricing Update to Production..."

# 1. Verify pricing.json exists locally
if [ ! -f "$PRICING_FILE" ]; then
    echo "❌ Error: $PRICING_FILE not found."
    exit 1
fi

# 2. Validate JSON syntax
if ! python3 -c "import json; json.load(open('$PRICING_FILE'))" 2>/dev/null; then
    echo "❌ Error: $PRICING_FILE contains invalid JSON."
    exit 1
fi

echo "✅ Pricing file validated."

# 3. Sync only the pricing.json file
echo "📤 Uploading pricing.json..."
rsync -avz "$PRICING_FILE" "$SERVER:$REMOTE_DIR/$PRICING_FILE"

if [ $? -ne 0 ]; then
    echo "❌ Failed to sync pricing.json."
    exit 1
fi

echo ""
echo "🎉 Deployment Complete! New pricing is live."
echo ""
echo "Hot-reload enabled: No restart needed."
echo "The next user to open the Buy Credits modal will see the new pricing."
