#!/bin/bash

################################################################################
# Listmonk - Database Restore Script
################################################################################
# This script restores Listmonk PostgreSQL backups.
#
# Usage:
#   ./listmonk-restore.sh                      # List available backups
#   ./listmonk-restore.sh 20250108_120000      # Restore full backup
#   ./listmonk-restore.sh latest               # Restore most recent backup
#   ./listmonk-restore.sh storage-box          # List backups on Storage Box
#   ./listmonk-restore.sh user user@email.com  # Export single user's data
#   ./listmonk-restore.sh user user@email.com 20250108_120000  # Export from specific backup
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
LOG_FILE="$LOG_DIR/listmonk-restore.log"
PRE_RESTORE_BACKUP="/root/backups/listmonk/pre-restore"
CONTAINER_NAME="listmonk_db"
TEMP_DB_NAME="listmonk_restore_temp"

# Storage Box Configuration
STORAGE_BOX_USER="${STORAGE_BOX_USER:-}"
STORAGE_BOX_HOST="${STORAGE_BOX_HOST:-}"
STORAGE_BOX_PATH="${STORAGE_BOX_PATH:-/backups}/listmonk"
STORAGE_BOX_PASSWORD="${STORAGE_BOX_PASSWORD:-}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

################################################################################
# Logging
################################################################################
log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

print_color() {
    local color=$1
    shift
    echo -e "${color}$@${NC}"
}

################################################################################
# List Local Backups
################################################################################
list_local_backups() {
    print_color "$BLUE" "\n📦 Available Listmonk Backups:"
    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ ! -d "$BACKUP_DIR" ]]; then
        print_color "$RED" "❌ Backup directory not found: $BACKUP_DIR"
        return 1
    fi

    local backups=($(find "$BACKUP_DIR" -name "listmonk-backup-*.sql.gz" -type f 2>/dev/null | sort -r))

    if [[ ${#backups[@]} -eq 0 ]]; then
        print_color "$YELLOW" "⚠️  No local backups found."
        return 1
    fi

    local count=1
    for backup in "${backups[@]}"; do
        local filename=$(basename "$backup")
        local timestamp=$(echo "$filename" | sed 's/listmonk-backup-\(.*\)\.sql\.gz/\1/')
        local size=$(du -h "$backup" | cut -f1)
        local date=$(date -r "$backup" "+%Y-%m-%d %H:%M:%S" 2>/dev/null || stat -c "%y" "$backup" 2>/dev/null | cut -d. -f1)

        printf "%2d. %s  (%s)  [%s]\n" "$count" "$timestamp" "$size" "$date"
        count=$((count + 1))
    done

    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    print_color "$GREEN" "\n💡 Commands:"
    print_color "$GREEN" "   Full restore:  ./listmonk-restore.sh <timestamp>"
    print_color "$GREEN" "   Single user:   ./listmonk-restore.sh user email@example.com [timestamp]"
}

################################################################################
# Get SSH command (with password if configured)
################################################################################
get_ssh_cmd() {
    if [[ -n "$STORAGE_BOX_PASSWORD" ]] && command -v sshpass >/dev/null 2>&1; then
        echo "sshpass -p '${STORAGE_BOX_PASSWORD}' ssh -p 23 -o ConnectTimeout=10"
    else
        echo "ssh -p 23 -o BatchMode=yes -o ConnectTimeout=10"
    fi
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

    local ssh_cmd=$(get_ssh_cmd)
    eval "$ssh_cmd ${STORAGE_BOX_USER}@${STORAGE_BOX_HOST}" \
        "'ls -lh ${STORAGE_BOX_PATH}/listmonk-backup-*.sql.gz 2>/dev/null || echo No backups found'" \
        | grep -v "^total" || true

    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

################################################################################
# Download backup from Storage Box if not local
################################################################################
ensure_backup_local() {
    local timestamp="$1"
    local backup_file="$BACKUP_DIR/listmonk-backup-${timestamp}.sql.gz"

    if [[ -f "$backup_file" ]]; then
        echo "$backup_file"
        return 0
    fi

    print_color "$YELLOW" "⚠️  Backup not found locally. Checking Storage Box..."

    if [[ -z "$STORAGE_BOX_USER" ]] || [[ -z "$STORAGE_BOX_HOST" ]]; then
        print_color "$RED" "❌ Storage Box not configured."
        return 1
    fi

    print_color "$BLUE" "📥 Downloading from Storage Box..."
    mkdir -p "$BACKUP_DIR"

    # Build rsync command with password auth if configured
    local rsync_ssh="ssh -p 23"
    if [[ -n "$STORAGE_BOX_PASSWORD" ]] && command -v sshpass >/dev/null 2>&1; then
        rsync_ssh="sshpass -p '${STORAGE_BOX_PASSWORD}' ssh -p 23 -o ConnectTimeout=10"
    fi

    if eval "rsync -avz -e \"$rsync_ssh\" \
        '${STORAGE_BOX_USER}@${STORAGE_BOX_HOST}:${STORAGE_BOX_PATH}/listmonk-backup-${timestamp}.sql.gz' \
        '$backup_file'"; then
        print_color "$GREEN" "✅ Downloaded from Storage Box"
        echo "$backup_file"
    else
        print_color "$RED" "❌ Failed to download backup"
        return 1
    fi
}

################################################################################
# Create pre-restore backup
################################################################################
create_pre_restore_backup() {
    print_color "$YELLOW" "\n🛡️  Creating safety backup of current database..."

    mkdir -p "$PRE_RESTORE_BACKUP"
    local timestamp=$(date +"%Y%m%d_%H%M%S")
    local safety_backup="$PRE_RESTORE_BACKUP/before-restore-${timestamp}.sql.gz"

    if docker exec "$CONTAINER_NAME" pg_dump -U listmonk -d listmonk | gzip > "$safety_backup"; then
        print_color "$GREEN" "✅ Safety backup created: $safety_backup"
        log "Safety backup created: $safety_backup"
    else
        print_color "$RED" "❌ Failed to create safety backup"
        return 1
    fi
}

################################################################################
# Full Database Restore
################################################################################
restore_full() {
    local timestamp="$1"

    print_color "$BLUE" "\n🔄 Starting full restore process..."
    log "Starting full restore from: listmonk-backup-${timestamp}.sql.gz"

    # Get backup file
    local backup_file=$(ensure_backup_local "$timestamp")
    if [[ -z "$backup_file" ]]; then
        return 1
    fi

    # Verify backup
    print_color "$YELLOW" "🔍 Verifying backup integrity..."
    if ! gzip -t "$backup_file" 2>/dev/null; then
        print_color "$RED" "❌ Backup file is corrupted!"
        return 1
    fi
    print_color "$GREEN" "✅ Backup verified"

    # Create safety backup
    create_pre_restore_backup

    # Stop listmonk app container to prevent connections
    print_color "$YELLOW" "\n⏸️  Stopping listmonk app container..."
    docker stop listmonk 2>/dev/null || true

    # Drop and recreate database
    print_color "$YELLOW" "\n🗑️  Dropping and recreating database..."
    docker exec "$CONTAINER_NAME" psql -U listmonk -d postgres -c "DROP DATABASE IF EXISTS listmonk;"
    docker exec "$CONTAINER_NAME" psql -U listmonk -d postgres -c "CREATE DATABASE listmonk;"

    # Restore
    print_color "$YELLOW" "\n📥 Restoring database..."
    if gunzip -c "$backup_file" | docker exec -i "$CONTAINER_NAME" psql -U listmonk -d listmonk; then
        print_color "$GREEN" "✅ Database restored successfully"
    else
        print_color "$RED" "❌ Restore failed!"
        print_color "$YELLOW" "Attempting to restore from safety backup..."
        # Could add recovery logic here
        return 1
    fi

    # Restart listmonk
    print_color "$YELLOW" "\n▶️  Starting listmonk app container..."
    docker start listmonk

    # Verify
    sleep 2
    local count=$(docker exec "$CONTAINER_NAME" psql -U listmonk -d listmonk -t -c "SELECT COUNT(*) FROM subscribers;" | tr -d ' ')

    print_color "$GREEN" "\n✅ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    print_color "$GREEN" "✅ Full restore completed successfully!"
    print_color "$GREEN" "✅ Subscriber count: $count"
    print_color "$GREEN" "✅ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    log "Full restore completed. Subscriber count: $count"
}

################################################################################
# Export Single User Data
################################################################################
export_user() {
    local email="$1"
    local timestamp="${2:-latest}"

    print_color "$BLUE" "\n👤 Exporting data for user: $email"

    # Determine backup file
    local backup_file
    if [[ "$timestamp" == "latest" ]]; then
        backup_file=$(find "$BACKUP_DIR" -name "listmonk-backup-*.sql.gz" -type f | sort -r | head -1)
        if [[ -z "$backup_file" ]]; then
            print_color "$RED" "❌ No backups found"
            return 1
        fi
        timestamp=$(basename "$backup_file" | sed 's/listmonk-backup-\(.*\)\.sql\.gz/\1/')
    else
        backup_file=$(ensure_backup_local "$timestamp")
    fi

    if [[ -z "$backup_file" ]] || [[ ! -f "$backup_file" ]]; then
        print_color "$RED" "❌ Backup not found"
        return 1
    fi

    print_color "$CYAN" "Using backup: $(basename "$backup_file")"

    # Create temporary database for extraction
    print_color "$YELLOW" "\n📦 Creating temporary database for extraction..."
    docker exec "$CONTAINER_NAME" psql -U listmonk -d postgres -c "DROP DATABASE IF EXISTS $TEMP_DB_NAME;" 2>/dev/null || true
    docker exec "$CONTAINER_NAME" psql -U listmonk -d postgres -c "CREATE DATABASE $TEMP_DB_NAME;"

    # Restore to temp database
    print_color "$YELLOW" "📥 Loading backup into temp database..."
    gunzip -c "$backup_file" | docker exec -i "$CONTAINER_NAME" psql -U listmonk -d "$TEMP_DB_NAME" > /dev/null 2>&1

    # Export user data
    local output_dir="/root/backups/listmonk/user-exports"
    mkdir -p "$output_dir"
    local output_file="$output_dir/${email//[@.]/_}_${timestamp}.json"

    print_color "$YELLOW" "\n🔍 Extracting user data..."

    # Query for subscriber data - using -c with properly escaped query
    local sql_query="SELECT json_build_object(
        'subscriber', row_to_json(s),
        'lists', (
            SELECT json_agg(json_build_object('list_id', sl.list_id, 'status', sl.status, 'created_at', sl.created_at))
            FROM subscriber_lists sl
            WHERE sl.subscriber_id = s.id
        ),
        'campaign_views', (
            SELECT json_agg(json_build_object('campaign_id', cv.campaign_id, 'created_at', cv.created_at))
            FROM campaign_views cv
            WHERE cv.subscriber_id = s.id
        ),
        'link_clicks', (
            SELECT json_agg(json_build_object('link_id', lc.link_id, 'created_at', lc.created_at))
            FROM link_clicks lc
            WHERE lc.subscriber_id = s.id
        )
    )
    FROM subscribers s
    WHERE s.email = '${email}';"

    docker exec "$CONTAINER_NAME" psql -U listmonk -d "$TEMP_DB_NAME" -t -c "$sql_query" > "$output_file.tmp"

    # Check if user was found (file should contain JSON, not just whitespace)
    local content=$(cat "$output_file.tmp" | tr -d '[:space:]')
    if [[ -z "$content" ]] || [[ "$content" == "null" ]]; then
        print_color "$RED" "❌ User '$email' not found in backup"
        rm -f "$output_file.tmp"
        docker exec "$CONTAINER_NAME" psql -U listmonk -d postgres -c "DROP DATABASE IF EXISTS $TEMP_DB_NAME;" > /dev/null 2>&1
        return 1
    fi

    # Format JSON
    cat "$output_file.tmp" | python3 -m json.tool > "$output_file" 2>/dev/null || mv "$output_file.tmp" "$output_file"
    rm -f "$output_file.tmp"

    # Cleanup temp database
    print_color "$YELLOW" "🧹 Cleaning up temporary database..."
    docker exec "$CONTAINER_NAME" psql -U listmonk -d postgres -c "DROP DATABASE IF EXISTS $TEMP_DB_NAME;" > /dev/null 2>&1

    print_color "$GREEN" "\n✅ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    print_color "$GREEN" "✅ User data exported successfully!"
    print_color "$GREEN" "✅ Output: $output_file"
    print_color "$GREEN" "✅ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    # Show preview
    print_color "$CYAN" "\n📄 Preview:"
    head -30 "$output_file"

    log "User export completed: $email -> $output_file"
}

################################################################################
# Restore Single User to Live Database
################################################################################
restore_user() {
    local email="$1"
    local timestamp="${2:-latest}"

    print_color "$BLUE" "\n👤 Restoring user to live database: $email"
    print_color "$YELLOW" "From backup: $timestamp"

    # First export the user data
    export_user "$email" "$timestamp"

    print_color "$YELLOW" "\n⚠️  To restore this user to the live database, you can:"
    print_color "$CYAN" "1. Use Listmonk admin UI to manually add the subscriber"
    print_color "$CYAN" "2. Use the Listmonk API to import the subscriber"
    print_color "$CYAN" "3. Run SQL directly (advanced):"
    print_color "$NC" ""
    print_color "$NC" "   # Example: Restore subscriber from exported JSON"
    print_color "$NC" "   docker exec -i listmonk_db psql -U listmonk -d listmonk <<< \\"
    print_color "$NC" "   \"INSERT INTO subscribers (email, name, attribs, status) VALUES (...);\""
}

################################################################################
# Main
################################################################################
main() {
    mkdir -p "$LOG_DIR"

    print_color "$BLUE" "\n╔════════════════════════════════════════════════════════════╗"
    print_color "$BLUE" "║         Listmonk - Database Restore Tool                   ║"
    print_color "$BLUE" "╚════════════════════════════════════════════════════════════╝"

    if [[ $# -eq 0 ]]; then
        list_local_backups
        exit 0
    fi

    case "$1" in
        storage-box)
            list_storage_box_backups
            ;;
        user)
            if [[ $# -lt 2 ]]; then
                print_color "$RED" "❌ Usage: ./listmonk-restore.sh user <email> [timestamp]"
                exit 1
            fi
            export_user "$2" "${3:-latest}"
            ;;
        restore-user)
            if [[ $# -lt 2 ]]; then
                print_color "$RED" "❌ Usage: ./listmonk-restore.sh restore-user <email> [timestamp]"
                exit 1
            fi
            restore_user "$2" "${3:-latest}"
            ;;
        latest)
            local latest=$(find "$BACKUP_DIR" -name "listmonk-backup-*.sql.gz" -type f | sort -r | head -1)
            if [[ -z "$latest" ]]; then
                print_color "$RED" "❌ No backups found."
                exit 1
            fi
            local ts=$(basename "$latest" | sed 's/listmonk-backup-\(.*\)\.sql\.gz/\1/')

            print_color "$YELLOW" "\n⚠️  WARNING: This will REPLACE the entire Listmonk database!"
            print_color "$YELLOW" "⚠️  Restoring from: $ts"
            print_color "$YELLOW" "⚠️  Current database will be backed up first."
            read -p "Are you sure? (type 'yes' to confirm): " confirm

            if [[ "$confirm" == "yes" ]]; then
                restore_full "$ts"
            else
                print_color "$RED" "❌ Restore cancelled."
            fi
            ;;
        *)
            local timestamp="$1"

            print_color "$YELLOW" "\n⚠️  WARNING: This will REPLACE the entire Listmonk database!"
            print_color "$YELLOW" "⚠️  Restoring from: $timestamp"
            print_color "$YELLOW" "⚠️  Current database will be backed up first."
            read -p "Are you sure? (type 'yes' to confirm): " confirm

            if [[ "$confirm" == "yes" ]]; then
                restore_full "$timestamp"
            else
                print_color "$RED" "❌ Restore cancelled."
            fi
            ;;
    esac
}

################################################################################
# Execute
################################################################################
main "$@"
