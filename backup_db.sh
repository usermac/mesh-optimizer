#!/bin/bash
SERVER="root@webdeliveryengine.com"
REMOTE_FILE="/root/mesh-optimizer/server/database.json"
LOCAL_BACKUP_DIR="./backups"

# Create backup folder with timestamp
mkdir -p $LOCAL_BACKUP_DIR
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

echo "📥 Pulling Customer Database..."

# Download file and rename it with date
scp $SERVER:$REMOTE_FILE "$LOCAL_BACKUP_DIR/database_$TIMESTAMP.json"

echo "✅ Saved to $LOCAL_BACKUP_DIR/database_$TIMESTAMP.json"
