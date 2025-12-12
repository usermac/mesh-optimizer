#!/bin/bash

################################################################################
# Mesh Optimizer - Automated Database Backup Script
################################################################################
# This script:
# - Backs up stats.db and database.json
# - Compresses backups using gzip
# - Stores locally in /root/backups/ (7 days retention)
# - Uploads to Hetzner Storage Box via rsync (30 days retention)
# - Sends email reports via Resend API
# - Logs all activity
# - Sends alerts on failure
################################################################################

set -euo pipefail

# Load environment variables
if [ -f "/root/mesh-optimizer/.env" ]; then
    set -a
    source "/root/mesh-optimizer/.env"
    set +a
fi

# Configuration
BACKUP_DIR="/root/backups"
LOG_DIR="/var/log/mesh"
LOG_FILE="$LOG_DIR/backup.log"
DB_DIR="/root/mesh-optimizer/server"
STATS_DB="$DB_DIR/stats.db"
DATABASE_JSON="$DB_DIR/database.json"
LOCAL_RETENTION_DAYS=7
REMOTE_RETENTION_DAYS=30
SEND_REPORT_SCRIPT="/root/mesh-optimizer/scripts/backup/send_report.sh"

# Storage Box Configuration (from environment)
STORAGE_BOX_USER="${STORAGE_BOX_USER:-}"
STORAGE_BOX_HOST="${STORAGE_BOX_HOST:-}"
STORAGE_BOX_PATH="${STORAGE_BOX_PATH:-/backups}"
STORAGE_BOX_PASSWORD="${STORAGE_BOX_PASSWORD:-}"

# Email Configuration (from environment)
RESEND_API_KEY="${RESEND_API_KEY:-}"
BACKUP_EMAIL="${BACKUP_EMAIL:-}"

# Timestamp
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
DATE_READABLE=$(date +"%Y-%m-%d %H:%M:%S %Z")
BACKUP_NAME="mesh-backup-${TIMESTAMP}"
BACKUP_PATH="$BACKUP_DIR/$BACKUP_NAME"

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
# Send Success Email via Resend
################################################################################
send_success_email() {
    local backup_size="$1"
    local files_backed_up="$2"
    local storage_box_status="$3"

    if [[ -z "$RESEND_API_KEY" ]] || [[ -z "$BACKUP_EMAIL" ]]; then
        log "WARNING: RESEND_API_KEY or BACKUP_EMAIL not set. Skipping email notification."
        return 0
    fi

    log "Sending success notification email..."

    local subject="✅ Database Backup Successful - $TIMESTAMP"
    local message="Backup Completed Successfully

Timestamp: $DATE_READABLE
Backup Size: $backup_size
Backup Name: ${BACKUP_NAME}.tar.gz

Storage Locations:
- Local: $BACKUP_DIR (kept for $LOCAL_RETENTION_DAYS days)
- Storage Box: $storage_box_status

---
Automated backup from Mesh Optimizer Server (webdeliveryengine.com)"

    if [[ -x "$SEND_REPORT_SCRIPT" ]]; then
        "$SEND_REPORT_SCRIPT" "$subject" "$message" >> "$LOG_FILE" 2>&1 || log "WARNING: Failed to execute send_report.sh"
    else
        log "WARNING: send_report.sh not found or not executable at $SEND_REPORT_SCRIPT"
    fi
}

################################################################################
# Send Failure Email via Resend
################################################################################
send_failure_email() {
    local error_message="$1"

    if [[ -z "$RESEND_API_KEY" ]] || [[ -z "$BACKUP_EMAIL" ]]; then
        return 0
    fi

    log "Sending failure notification email..."

    local subject="❌ Database Backup FAILED - $TIMESTAMP"
    local message="Backup Failed

Timestamp: $DATE_READABLE

Error:
$error_message

---
Action Required: Please SSH into the server and investigate immediately.

ssh root@webdeliveryengine.com

Check logs at: $LOG_FILE

---
Automated alert from Mesh Optimizer Server"

    if [[ -x "$SEND_REPORT_SCRIPT" ]]; then
        "$SEND_REPORT_SCRIPT" "$subject" "$message" >> "$LOG_FILE" 2>&1 || true
    else
        log "WARNING: send_report.sh not found or not executable at $SEND_REPORT_SCRIPT"
    fi
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

    # Determine authentication method
    local ssh_cmd="ssh -p 23 -o BatchMode=yes -o ConnectTimeout=10"
    local rsync_ssh="ssh -p 23 -o BatchMode=yes -o ConnectTimeout=10"

    if [[ -n "$STORAGE_BOX_PASSWORD" ]]; then
        # Check if sshpass is installed
        if command -v sshpass >/dev/null 2>&1; then
            log "Using password authentication"
            ssh_cmd="sshpass -p '${STORAGE_BOX_PASSWORD}' ssh -p 23 -o ConnectTimeout=10"
            rsync_ssh="sshpass -p '${STORAGE_BOX_PASSWORD}' ssh -p 23 -o ConnectTimeout=10"
        else
            log "WARNING: STORAGE_BOX_PASSWORD set but sshpass not installed. Install with: apt-get install sshpass"
            log "Falling back to SSH key authentication"
        fi
    else
        log "Using SSH key authentication"
    fi

    # Ensure remote directory exists
    eval "$ssh_cmd ${STORAGE_BOX_USER}@${STORAGE_BOX_HOST} 'mkdir -p ${STORAGE_BOX_PATH}'" 2>/dev/null || true

    # Upload using rsync (efficient, resumable)
    if eval "rsync -avz -e \"$rsync_ssh\" '$backup_file' '${STORAGE_BOX_USER}@${STORAGE_BOX_HOST}:${STORAGE_BOX_PATH}/'" 2>&1 | tee -a "$LOG_FILE" >&2; then
        log "✅ Uploaded to Storage Box successfully"

        # Clean up old backups on Storage Box (keep last 30 days)
        log "Cleaning old backups on Storage Box (keeping last $REMOTE_RETENTION_DAYS days)..."
        eval "$ssh_cmd ${STORAGE_BOX_USER}@${STORAGE_BOX_HOST} 'find ${STORAGE_BOX_PATH} -name \"mesh-backup-*.tar.gz\" -type f -mtime +${REMOTE_RETENTION_DAYS} -delete'" 2>&1 | tee -a "$LOG_FILE" >&2 || true

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
    log "Starting backup process..."
    log "Timestamp: $DATE_READABLE"

    # Create directories if they don't exist
    mkdir -p "$BACKUP_DIR"
    mkdir -p "$LOG_DIR"
    mkdir -p "$BACKUP_PATH"

    # Check if database files exist
    if [[ ! -f "$STATS_DB" ]]; then
        error_exit "stats.db not found at $STATS_DB"
    fi

    local files_list=""

    # Copy stats.db
    log "Copying stats.db..."
    cp "$STATS_DB" "$BACKUP_PATH/stats.db" || error_exit "Failed to copy stats.db"
    local stats_size=$(du -h "$STATS_DB" | cut -f1)
    files_list="<li>stats.db ($stats_size)</li>"

    # Copy database.json if it exists
    if [[ -f "$DATABASE_JSON" ]]; then
        log "Copying database.json..."
        cp "$DATABASE_JSON" "$BACKUP_PATH/database.json" || error_exit "Failed to copy database.json"
        local json_size=$(du -h "$DATABASE_JSON" | cut -f1)
        files_list="${files_list}<li>database.json ($json_size)</li>"
    else
        log "WARNING: database.json not found at $DATABASE_JSON (might be empty or unused)"
    fi

    # Create SHA256 checksums
    log "Creating checksums..."
    cd "$BACKUP_PATH"
    sha256sum stats.db > checksums.txt 2>/dev/null || shasum -a 256 stats.db > checksums.txt
    if [[ -f "database.json" ]]; then
        sha256sum database.json >> checksums.txt 2>/dev/null || shasum -a 256 database.json >> checksums.txt
    fi
    files_list="${files_list}<li>checksums.txt (verification)</li>"
    cd - > /dev/null

    # Create compressed archive
    log "Compressing backup..."
    tar -czf "${BACKUP_PATH}.tar.gz" -C "$BACKUP_DIR" "$BACKUP_NAME" || error_exit "Failed to create compressed archive"

    # Get file size
    local archive_size=$(du -h "${BACKUP_PATH}.tar.gz" | cut -f1)

    log "Backup created: ${BACKUP_NAME}.tar.gz ($archive_size)"

    # Remove temporary uncompressed directory
    rm -rf "$BACKUP_PATH"

    # Verify archive integrity
    log "Verifying archive integrity..."
    tar -tzf "${BACKUP_PATH}.tar.gz" > /dev/null || error_exit "Archive verification failed"
    log "✅ Archive verified successfully"

    # Upload to Storage Box
    local storage_box_status=$(upload_to_storage_box "${BACKUP_PATH}.tar.gz")

    # Send success email
    send_success_email "$archive_size" "$files_list" "$storage_box_status"

    # Clean up old local backups (keep last 7 days)
    log "Cleaning up old local backups (keeping last $LOCAL_RETENTION_DAYS days)..."
    find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f -mtime +$LOCAL_RETENTION_DAYS -delete

    local remaining_backups=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | wc -l | tr -d ' ')
    log "Local cleanup complete. $remaining_backups backup(s) retained locally."

    log "✅ Backup process completed successfully"
    log "=========================================="
}

################################################################################
# Execute Main Function
################################################################################
main
