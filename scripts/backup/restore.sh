#!/bin/bash

################################################################################
# Mesh Optimizer - Database Restore Script
################################################################################
# This script restores database backups from local or Storage Box storage.
#
# Usage:
#   ./restore.sh                    # List available backups
#   ./restore.sh 20250108_120000    # Restore specific backup by timestamp
#   ./restore.sh latest             # Restore most recent backup
#   ./restore.sh storage-box        # List backups on Storage Box
################################################################################

set -euo pipefail

# Configuration
BACKUP_DIR="/root/backups"
DB_DIR="/root/mesh-optimizer/server"
LOG_DIR="/var/log/mesh"
LOG_FILE="$LOG_DIR/restore.log"
PRE_RESTORE_BACKUP="/root/backups/pre-restore"

# Storage Box Configuration (from environment)
STORAGE_BOX_USER="${STORAGE_BOX_USER:-}"
STORAGE_BOX_HOST="${STORAGE_BOX_HOST:-}"
STORAGE_BOX_PATH="${STORAGE_BOX_PATH:-/backups}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

################################################################################
# Logging Function
################################################################################
log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

################################################################################
# Print colored message
################################################################################
print_color() {
    local color=$1
    shift
    echo -e "${color}$@${NC}"
}

################################################################################
# List Local Backups
################################################################################
list_local_backups() {
    print_color "$BLUE" "\n📦 Available Local Backups:"
    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ ! -d "$BACKUP_DIR" ]]; then
        print_color "$RED" "❌ Backup directory not found: $BACKUP_DIR"
        return 1
    fi

    local backups=($(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r))

    if [[ ${#backups[@]} -eq 0 ]]; then
        print_color "$YELLOW" "⚠️  No local backups found."
        return 1
    fi

    local count=1
    for backup in "${backups[@]}"; do
        local filename=$(basename "$backup")
        local timestamp=$(echo "$filename" | sed 's/mesh-backup-\(.*\)\.tar\.gz/\1/')
        local size=$(du -h "$backup" | cut -f1)
        local date=$(date -r "$backup" "+%Y-%m-%d %H:%M:%S" 2>/dev/null || stat -f "%Sm" -t "%Y-%m-%d %H:%M:%S" "$backup")

        printf "%2d. %s  (%s)  [%s]\n" "$count" "$timestamp" "$size" "$date"
        count=$((count + 1))
    done

    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    print_color "$GREEN" "\n💡 To restore, run: ./restore.sh <timestamp>"
    print_color "$GREEN" "   Example: ./restore.sh $timestamp"
}

################################################################################
# List Storage Box Backups
################################################################################
list_storage_box_backups() {
    if [[ -z "$STORAGE_BOX_USER" ]] || [[ -z "$STORAGE_BOX_HOST" ]]; then
        print_color "$RED" "❌ Storage Box not configured."
        return 1
    fi

    print_color "$BLUE" "\n☁️  Available Storage Box Backups:"
    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    ssh -p 23 "${STORAGE_BOX_USER}@${STORAGE_BOX_HOST}" "ls -lh ${STORAGE_BOX_PATH}/mesh-backup-*.tar.gz 2>/dev/null || echo 'No backups found'" | grep -v "^total" || true

    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

################################################################################
# Create Pre-Restore Backup
################################################################################
create_pre_restore_backup() {
    print_color "$YELLOW" "\n🛡️  Creating safety backup of current databases..."

    mkdir -p "$PRE_RESTORE_BACKUP"
    local timestamp=$(date +"%Y%m%d_%H%M%S")
    local safety_backup="$PRE_RESTORE_BACKUP/before-restore-${timestamp}.tar.gz"

    if [[ -f "$DB_DIR/stats.db" ]]; then
        tar -czf "$safety_backup" -C "$DB_DIR" stats.db database.json 2>/dev/null || \
        tar -czf "$safety_backup" -C "$DB_DIR" stats.db
        print_color "$GREEN" "✅ Safety backup created: $safety_backup"
        log "Safety backup created: $safety_backup"
    else
        print_color "$YELLOW" "⚠️  No existing database to backup."
    fi
}

################################################################################
# Verify Backup Integrity
################################################################################
verify_backup() {
    local backup_file="$1"

    print_color "$YELLOW" "\n🔍 Verifying backup integrity..."

    # Test if archive is valid
    if ! tar -tzf "$backup_file" > /dev/null 2>&1; then
        print_color "$RED" "❌ Backup archive is corrupted!"
        return 1
    fi

    # Extract to temporary location for verification
    local temp_dir=$(mktemp -d)
    tar -xzf "$backup_file" -C "$temp_dir"

    # Find the backup directory
    local backup_content=$(find "$temp_dir" -name "mesh-backup-*" -type d | head -1)

    if [[ ! -f "$backup_content/stats.db" ]]; then
        print_color "$RED" "❌ stats.db not found in backup!"
        rm -rf "$temp_dir"
        return 1
    fi

    # Verify checksums if present
    if [[ -f "$backup_content/checksums.txt" ]]; then
        print_color "$YELLOW" "Verifying checksums..."
        cd "$backup_content"
        if sha256sum -c checksums.txt 2>/dev/null || shasum -a 256 -c checksums.txt 2>/dev/null; then
            print_color "$GREEN" "✅ Checksums verified successfully"
        else
            print_color "$RED" "❌ Checksum verification failed!"
            rm -rf "$temp_dir"
            return 1
        fi
        cd - > /dev/null
    fi

    rm -rf "$temp_dir"
    print_color "$GREEN" "✅ Backup integrity verified"
    return 0
}

################################################################################
# Restore Backup
################################################################################
restore_backup() {
    local timestamp="$1"
    local backup_file="$BACKUP_DIR/mesh-backup-${timestamp}.tar.gz"

    # Check if backup exists locally
    if [[ ! -f "$backup_file" ]]; then
        print_color "$YELLOW" "⚠️  Backup not found locally. Checking Storage Box..."

        if [[ -z "$STORAGE_BOX_USER" ]] || [[ -z "$STORAGE_BOX_HOST" ]]; then
            print_color "$RED" "❌ Storage Box not configured. Cannot download backup."
            return 1
        fi

        # Download from Storage Box
        print_color "$BLUE" "📥 Downloading from Storage Box..."
        mkdir -p "$BACKUP_DIR"

        if rsync -avz -e "ssh -p 23" \
            "${STORAGE_BOX_USER}@${STORAGE_BOX_HOST}:${STORAGE_BOX_PATH}/mesh-backup-${timestamp}.tar.gz" \
            "$backup_file"; then
            print_color "$GREEN" "✅ Downloaded from Storage Box"
        else
            print_color "$RED" "❌ Failed to download backup from Storage Box"
            return 1
        fi
    fi

    print_color "$BLUE" "\n🔄 Starting restore process..."
    log "Starting restore of backup: mesh-backup-${timestamp}.tar.gz"

    # Verify backup integrity
    if ! verify_backup "$backup_file"; then
        print_color "$RED" "❌ Backup verification failed. Restore aborted."
        return 1
    fi

    # Create safety backup of current state
    create_pre_restore_backup

    # Stop Docker container (optional, uncomment if needed)
    # print_color "$YELLOW" "\n⏸️  Stopping Docker container..."
    # docker stop api || true

    # Extract backup to temporary location
    local temp_dir=$(mktemp -d)
    print_color "$YELLOW" "\n📂 Extracting backup..."
    tar -xzf "$backup_file" -C "$temp_dir"

    local backup_content=$(find "$temp_dir" -name "mesh-backup-*" -type d | head -1)

    # Restore database files
    print_color "$YELLOW" "\n📥 Restoring database files..."

    if [[ -f "$backup_content/stats.db" ]]; then
        cp "$backup_content/stats.db" "$DB_DIR/stats.db"
        print_color "$GREEN" "✅ Restored stats.db"
        log "Restored stats.db"
    fi

    if [[ -f "$backup_content/database.json" ]]; then
        cp "$backup_content/database.json" "$DB_DIR/database.json"
        print_color "$GREEN" "✅ Restored database.json"
        log "Restored database.json"
    fi

    # Set proper permissions
    chmod 644 "$DB_DIR/stats.db" 2>/dev/null || true
    chmod 644 "$DB_DIR/database.json" 2>/dev/null || true

    # Cleanup
    rm -rf "$temp_dir"

    # Restart Docker container (optional, uncomment if needed)
    # print_color "$YELLOW" "\n▶️  Restarting Docker container..."
    # docker start api || true

    print_color "$GREEN" "\n✅ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    print_color "$GREEN" "✅ Restore completed successfully!"
    print_color "$GREEN" "✅ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    print_color "$BLUE" "\n💡 Safety backup stored at:"
    print_color "$BLUE" "   $PRE_RESTORE_BACKUP"

    log "Restore completed successfully: mesh-backup-${timestamp}.tar.gz"
}

################################################################################
# Main
################################################################################
main() {
    mkdir -p "$LOG_DIR"

    print_color "$BLUE" "\n╔════════════════════════════════════════════════════════════╗"
    print_color "$BLUE" "║       Mesh Optimizer - Database Restore Tool              ║"
    print_color "$BLUE" "╚════════════════════════════════════════════════════════════╝"

    # Load environment variables if .env exists
    if [[ -f "/root/mesh-optimizer/.env" ]]; then
        set -a
        source /root/mesh-optimizer/.env
        set +a
    fi

    if [[ $# -eq 0 ]]; then
        # No arguments - list available backups
        list_local_backups
        exit 0
    fi

    case "$1" in
        storage-box)
            list_storage_box_backups
            ;;
        latest)
            # Restore latest backup
            local latest=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r | head -1)
            if [[ -z "$latest" ]]; then
                print_color "$RED" "❌ No backups found."
                exit 1
            fi
            local timestamp=$(basename "$latest" | sed 's/mesh-backup-\(.*\)\.tar\.gz/\1/')

            print_color "$YELLOW" "\n⚠️  WARNING: This will restore from backup: $timestamp"
            print_color "$YELLOW" "⚠️  Current database will be backed up to: $PRE_RESTORE_BACKUP"
            read -p "Are you sure? (type 'yes' to confirm): " confirm

            if [[ "$confirm" == "yes" ]]; then
                restore_backup "$timestamp"
            else
                print_color "$RED" "❌ Restore cancelled."
            fi
            ;;
        *)
            # Restore specific backup by timestamp
            local timestamp="$1"

            print_color "$YELLOW" "\n⚠️  WARNING: This will restore from backup: $timestamp"
            print_color "$YELLOW" "⚠️  Current database will be backed up to: $PRE_RESTORE_BACKUP"
            read -p "Are you sure? (type 'yes' to confirm): " confirm

            if [[ "$confirm" == "yes" ]]; then
                restore_backup "$timestamp"
            else
                print_color "$RED" "❌ Restore cancelled."
            fi
            ;;
    esac
}

################################################################################
# Execute Main Function
################################################################################
main "$@"
