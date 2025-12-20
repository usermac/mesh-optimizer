#!/bin/bash

# Configuration
SERVER="root@webdeliveryengine.com"
REMOTE_DIR="/root/mesh-optimizer"

echo "🚀 Deploying to Production..."

# Ensure remote directory exists
ssh $SERVER "mkdir -p $REMOTE_DIR"

# Install system tools for monitoring and backups
ssh $SERVER "apt-get update && apt-get install -y htop sshpass" || true

# 1. Sync Files
rsync -avz --exclude 'target' \
           --exclude 'node_modules' \
           --exclude 'uploads' \
           --exclude '.git' \
           --exclude '.DS_Store' \
           --exclude 'server/database.json' \
           --exclude 'server/stats.db' \
           --exclude 'backups' \
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

if [ $? -eq 0 ]; then
  echo "🎉 Deployment Complete! API is live."
else
  echo "❌ Deployment Failed."
  exit 1
fi
