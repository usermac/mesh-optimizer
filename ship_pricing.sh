#!/bin/bash
# ship_pricing.sh - Deploys only pricing changes to production.
#
# Usage: ./ship_pricing.sh
#
# This script syncs only the pricing.json file to the server and restarts
# the Docker container to apply the new pricing configuration.
# Use this for rapid pricing updates without a full rebuild.

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

echo "✅ Pricing file synced."

# 4. Restart the Docker container to apply the new pricing
echo "🔄 Restarting API to load new configuration..."
ssh "$SERVER" "docker restart api"

if [ $? -eq 0 ]; then
    echo ""
    echo "🎉 Deployment Complete! New pricing is live."
    echo ""
    echo "Pricing changes take effect immediately for new checkout sessions."
else
    echo "❌ Failed to restart container."
    exit 1
fi
