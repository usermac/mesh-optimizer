#!/bin/bash
set -e  # Exit immediately if any command fails

# Configuration
REMOTE_DIR="/root/mesh-optimizer"

# Server mapping - add new environments here
case "${1:-prod}" in
  prod)    SERVER="root@webdeliveryengine.com" ;;
  staging) SERVER="root@staging.webdeliveryengine.com" ;;
  *)       echo "Unknown target: $1. Valid targets: prod, staging"; exit 1 ;;
esac

echo "🚀 Deploying to ${1:-prod} ($SERVER)..."

# Ensure remote directory exists
ssh $SERVER "mkdir -p $REMOTE_DIR"

# Install system tools for monitoring and backups
ssh $SERVER "apt-get update && apt-get install -y htop sshpass" || true

# 1. Sync Files (whitelist approach - only sync what's needed for production)
# If you add new top-level prod files/folders, update this include list
rsync -avz \
           --include 'crates/***' \
           --exclude 'crates/*/test_*' \
           --exclude 'crates/**/test_*' \
           --include 'scripts/***' \
           --include 'server/***' \
           --exclude 'server/database.json' \
           --exclude 'server/stats.db' \
           --include 'Cargo.toml' \
           --include 'Cargo.lock' \
           --include 'Dockerfile' \
           --include 'Caddyfile' \
           --include 'deploy.sh' \
           --include '.env' \
           --exclude '*' \
           . $SERVER:$REMOTE_DIR

# 2. Make backup scripts executable
echo "🔧 Setting backup script permissions..."
ssh $SERVER "chmod +x $REMOTE_DIR/scripts/backup/*.sh 2>/dev/null || true"

echo "✅ Files Synced."
echo "🔨 Rebuilding Docker Container..."

# 3. Remote Build & Restart
# Added 'touch' command to ensure DB file exists so Docker doesn't make a directory
ssh $SERVER "cd $REMOTE_DIR && \
             touch server/database.json && \
             touch server/stats.db && \
             mkdir -p /root/uploads && \
             docker build -t mesh-api . && \
             docker rm -f api || true && \
             docker run -d -p 80:80 -p 443:443 \
               --env-file .env \
               -v /root/mesh-optimizer/server/database.json:/app/server/database.json \
               -v /root/mesh-optimizer/server/stats.db:/app/server/stats.db \
               -v /root/mesh-optimizer/server/pricing.json:/app/server/pricing.json \
               -v /root/mesh-optimizer/caddy_data:/data \
               -v /root/uploads:/app/uploads \
               --restart always --name api mesh-api"

# 4. Verify API is responding
echo "⏳ Waiting for API to start..."
sleep 5
if curl -sf --max-time 10 https://webdeliveryengine.com/health > /dev/null 2>&1; then
    echo "✅ Health check passed"
else
    echo "❌ Health check FAILED - API not responding"
    exit 1
fi

echo "🎉 Deployment Complete! API is live."
