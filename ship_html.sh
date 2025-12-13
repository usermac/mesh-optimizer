#!/bin/bash
# ship_html.sh - Deploys website frontend changes (HTML, CSS, JS) to production.
#
# Usage: ./ship_html.sh
#
# This script syncs the server/public directory to the server.
# Changes take effect immediately upon browser refresh (no server restart needed).

set -euo pipefail

# Configuration
SERVER="root@www.webdeliveryengine.com"
REMOTE_DIR="/root/mesh-optimizer"
PUBLIC_DIR="server/public"

echo "🚀 Deploying Website Frontend (HTML/CSS/JS) to Production..."

# 1. Verify public directory exists locally
if [ ! -d "$PUBLIC_DIR" ]; then
    echo "❌ Error: Directory $PUBLIC_DIR not found."
    exit 1
fi

echo "✅ Local files verified."

# 2. Sync the public directory
# --inplace: modifies files in place (good for Docker mounts)
# --delete: removes files on remote that are gone locally (keeps it clean)
# -r: recursive (implied by -a)
# Trailing slashes on directories are important for rsync to sync contents, not the folder itself
echo "📤 Uploading server/public/..."
rsync -avz --inplace --delete "$PUBLIC_DIR/" "$SERVER:$REMOTE_DIR/$PUBLIC_DIR/"

if [ $? -ne 0 ]; then
    echo "❌ Failed to sync website files."
    exit 1
fi

echo ""
echo "🎉 Deployment Complete! New website version is live."
echo ""
echo "Note: Users may need to hard refresh (Cmd+Shift+R) to see changes immediately."
