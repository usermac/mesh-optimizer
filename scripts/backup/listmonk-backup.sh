#!/bin/bash

################################################################################
# Listmonk - Automated PostgreSQL Backup Script
################################################################################
# This script:
# - Dumps PostgreSQL database from listmonk_db container
# - Compresses backups using gzip
# - Stores locally in /root/backups/listmonk/ (7 days retention)
# - Uploads to Hetzner Storage Box (30 days retention)
# - Sends email reports via Resend API
################################################################################

set -euo pipefail

# Load environment variables
if [ -f "/root/mesh-optimizer/.env" ]; then
    set -a
    source "/root/mesh-optimizer/.env"
    set +a
fi

# Configuration
BACKUP_DIR="/root/backups/listmonk"
LOG_DIR="/var/log/mesh"
LOG_FILE="$LOG_DIR/listmonk-backup.log"
LOCAL_RETENTION_DAYS=7
REMOTE_RETENTION_DAYS=30
CONTAINER_NAME="listmonk_db"

# Storage Box Configuration (from environment)
STORAGE_BOX_USER="${STORAGE_BOX_USER:-}"
STORAGE_BOX_HOST="${STORAGE_BOX_HOST:-}"
STORAGE_BOX_PATH="${STORAGE_BOX_PATH:-/backups}/listmonk"
STORAGE_BOX_PASSWORD="${STORAGE_BOX_PASSWORD:-}"

# Email Configuration (from environment)
RESEND_API_KEY="${RESEND_API_KEY:-}"
BACKUP_EMAIL="${BACKUP_EMAIL:-}"

# Timestamp
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
DATE_READABLE=$(date +"%Y-%m-%d %H:%M:%S %Z")
BACKUP_NAME="listmonk-backup-${TIMESTAMP}"
BACKUP_FILE="${BACKUP_DIR}/${BACKUP_NAME}.sql.gz"

################################################################################
# Logging Function
################################################################################
log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE" >&2
}

################################################################################
# Error Handler
################################################################################
error_exit() {
    log "ERROR: $1"
    send_failure_email "$1"
    exit 1
}

################################################################################
# Send Success Email
################################################################################
send_success_email() {
    local backup_size="$1"
    local storage_box_status="$2"
    local subscriber_count="$3"

    if [[ -z "$RESEND_API_KEY" ]] || [[ -z "$BACKUP_EMAIL" ]]; then
        log "WARNING: Email not configured. Skipping notification."
        return 0
    fi

    log "Sending success notification email..."

    local subject="✅ Listmonk Backup Successful - $TIMESTAMP"
    local body=$(cat <<EOF
{
  "from": "Mesh Optimizer <notifications@webdeliveryengine.com>",
  "to": ["$BACKUP_EMAIL"],
  "subject": "$subject",
  "text": "Listmonk Backup Completed Successfully\n\nTimestamp: $DATE_READABLE\nBackup Size: $backup_size\nSubscriber Count: $subscriber_count\nBackup Name: ${BACKUP_NAME}.sql.gz\n\nStorage Locations:\n- Local: $BACKUP_DIR (kept for $LOCAL_RETENTION_DAYS days)\n- Storage Box: $storage_box_status\n\n---\nAutomated backup from Mesh Optimizer Server"
}
EOF
)

    curl -s -X POST "https://api.resend.com/emails" \
        -H "Authorization: Bearer $RESEND_API_KEY" \
        -H "Content-Type: application/json" \
        -d "$body" >> "$LOG_FILE" 2>&1 || log "WARNING: Failed to send email"
}

################################################################################
# Send Failure Email
################################################################################
send_failure_email() {
    local error_message="$1"

    if [[ -z "$RESEND_API_KEY" ]] || [[ -z "$BACKUP_EMAIL" ]]; then
        return 0
    fi

    log "Sending failure notification email..."

    local subject="❌ Listmonk Backup FAILED - $TIMESTAMP"
    local body=$(cat <<EOF
{
  "from": "Mesh Optimizer <notifications@webdeliveryengine.com>",
  "to": ["$BACKUP_EMAIL"],
  "subject": "$subject",
  "text": "Listmonk Backup Failed\n\nTimestamp: $DATE_READABLE\n\nError:\n$error_message\n\n---\nAction Required: SSH into server and investigate.\n\nssh root@webdeliveryengine.com\nCheck logs at: $LOG_FILE\n\n---\nAutomated alert from Mesh Optimizer Server"
}
EOF
)

    curl -s -X POST "https://api.resend.com/emails" \
        -H "Authorization: Bearer $RESEND_API_KEY" \
        -H "Content-Type: application/json" \
        -d "$body" >> "$LOG_FILE" 2>&1 || true
}

################################################################################
# Upload to Storage Box
################################################################################
upload_to_storage_box() {
    local backup_file="$1"

    if [[ -z "$STORAGE_BOX_USER" ]] || [[ -z "$STORAGE_BOX_HOST" ]]; then
        log "WARNING: Storage Box not configured. Skipping remote upload."
        echo "Storage Box not configured"
        return 0
    fi

    log "Uploading to Storage Box..."

    # SSH command setup
    local ssh_cmd="ssh -p 23 -o BatchMode=yes -o ConnectTimeout=10"
    local rsync_ssh="ssh -p 23 -o BatchMode=yes -o ConnectTimeout=10"

    if [[ -n "$STORAGE_BOX_PASSWORD" ]] && command -v sshpass >/dev/null 2>&1; then
        log "Using password authentication"
        ssh_cmd="sshpass -p '${STORAGE_BOX_PASSWORD}' ssh -p 23 -o ConnectTimeout=10"
        rsync_ssh="sshpass -p '${STORAGE_BOX_PASSWORD}' ssh -p 23 -o ConnectTimeout=10"
    else
        log "Using SSH key authentication"
    fi

    # Ensure remote directory exists
    eval "$ssh_cmd ${STORAGE_BOX_USER}@${STORAGE_BOX_HOST} 'mkdir -p ${STORAGE_BOX_PATH}'" 2>/dev/null || true

    # Upload using rsync
    if eval "rsync -avz -e \"$rsync_ssh\" '$backup_file' '${STORAGE_BOX_USER}@${STORAGE_BOX_HOST}:${STORAGE_BOX_PATH}/'" 2>&1 | tee -a "$LOG_FILE" >&2; then
        log "✅ Uploaded to Storage Box successfully"

        # Clean up old backups on Storage Box
        log "Cleaning old backups on Storage Box (keeping last $REMOTE_RETENTION_DAYS days)..."
        eval "$ssh_cmd ${STORAGE_BOX_USER}@${STORAGE_BOX_HOST} 'find ${STORAGE_BOX_PATH} -name \"listmonk-backup-*.sql.gz\" -type f -mtime +${REMOTE_RETENTION_DAYS} -delete'" 2>&1 | tee -a "$LOG_FILE" >&2 || true

        echo "Successfully uploaded to Storage Box"
    else
        log "ERROR: Failed to upload to Storage Box"
        echo "Storage Box upload failed"
    fi
}

################################################################################
# Main Backup Process
################################################################################
main() {
    log "=========================================="
    log "Starting Listmonk backup process..."
    log "Timestamp: $DATE_READABLE"

    # Create directories
    mkdir -p "$BACKUP_DIR"
    mkdir -p "$LOG_DIR"

    # Check if container is running
    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        error_exit "Container $CONTAINER_NAME is not running"
    fi

    # Get subscriber count for reporting
    log "Getting subscriber count..."
    subscriber_count=$(docker exec "$CONTAINER_NAME" psql -U listmonk -d listmonk -t -c "SELECT COUNT(*) FROM subscribers;" 2>/dev/null | tr -d ' ' || echo "unknown")
    log "Current subscriber count: $subscriber_count"

    # Dump PostgreSQL database
    log "Dumping PostgreSQL database..."
    if docker exec "$CONTAINER_NAME" pg_dump -U listmonk -d listmonk | gzip > "$BACKUP_FILE"; then
        log "✅ Database dumped successfully"
    else
        error_exit "Failed to dump PostgreSQL database"
    fi

    # Verify the backup
    log "Verifying backup integrity..."
    if gzip -t "$BACKUP_FILE" 2>/dev/null; then
        log "✅ Backup verified (gzip integrity check passed)"
    else
        error_exit "Backup verification failed - corrupted gzip file"
    fi

    # Get file size
    local archive_size=$(du -h "$BACKUP_FILE" | cut -f1)
    log "Backup created: ${BACKUP_NAME}.sql.gz ($archive_size)"

    # Upload to Storage Box
    local storage_box_status=$(upload_to_storage_box "$BACKUP_FILE")

    # Send success email
    send_success_email "$archive_size" "$storage_box_status" "$subscriber_count"

    # Clean up old local backups
    log "Cleaning up old local backups (keeping last $LOCAL_RETENTION_DAYS days)..."
    find "$BACKUP_DIR" -name "listmonk-backup-*.sql.gz" -type f -mtime +$LOCAL_RETENTION_DAYS -delete

    local remaining_backups=$(find "$BACKUP_DIR" -name "listmonk-backup-*.sql.gz" -type f | wc -l | tr -d ' ')
    log "Local cleanup complete. $remaining_backups backup(s) retained locally."

    log "✅ Listmonk backup process completed successfully"
    log "=========================================="
}

################################################################################
# Execute
################################################################################
main
