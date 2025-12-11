#!/bin/bash

################################################################################
# Mesh Optimizer - Backup Verification Script
################################################################################
# This script verifies the integrity of backup files.
#
# Usage:
#   ./verify_backup.sh                    # Verify all local backups
#   ./verify_backup.sh 20250108_120000    # Verify specific backup
#   ./verify_backup.sh latest             # Verify most recent backup
################################################################################

set -euo pipefail

# Configuration
BACKUP_DIR="/root/backups"
LOG_DIR="/var/log/mesh"
LOG_FILE="$LOG_DIR/verify.log"

# Storage Box Configuration (from environment)
STORAGE_BOX_USER="${STORAGE_BOX_USER:-}"
STORAGE_BOX_HOST="${STORAGE_BOX_HOST:-}"
STORAGE_BOX_PATH="${STORAGE_BOX_PATH:-/backups}"

# Email Configuration (from environment)
RESEND_API_KEY="${RESEND_API_KEY:-}"
BACKUP_EMAIL="${BACKUP_EMAIL:-}"

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
# Send Verification Report Email
################################################################################
send_verification_email() {
    local total="$1"
    local passed="$2"
    local failed="$3"
    local report="$4"

    if [[ -z "$RESEND_API_KEY" ]] || [[ -z "$BACKUP_EMAIL" ]]; then
        log "WARNING: Email not configured. Skipping notification."
        return 0
    fi

    local subject
    local status_icon
    if [[ $failed -eq 0 ]]; then
        subject="✅ Backup Verification Passed"
        status_icon="✅"
    else
        subject="⚠️ Backup Verification Issues Detected"
        status_icon="⚠️"
    fi

    log "Sending verification report email..."

    local json_payload=$(cat <<EOF
{
  "from": "Mesh Optimizer Backups <backups@webdeliveryengine.com>",
  "to": ["$BACKUP_EMAIL"],
  "subject": "$subject - $(date +'%Y-%m-%d')",
  "html": "<h2>$status_icon Backup Verification Report</h2><p><strong>Date:</strong> $(date +'%Y-%m-%d %H:%M:%S %Z')</p><p><strong>Summary:</strong></p><ul><li>Total Backups Checked: $total</li><li>Passed: $passed ✅</li><li>Failed: $failed ❌</li></ul><hr><h3>Details:</h3><pre style='background:#f5f5f5;padding:10px;border-radius:5px;font-family:monospace;'>$report</pre><hr><p><small>Automated verification from Mesh Optimizer Server</small></p>"
}
EOF
)

    local response=$(curl -s -w "\n%{http_code}" -X POST \
        "https://api.resend.com/emails" \
        -H "Authorization: Bearer $RESEND_API_KEY" \
        -H "Content-Type: application/json" \
        -d "$json_payload")

    local http_code=$(echo "$response" | tail -n1)

    if [[ "$http_code" == "200" ]]; then
        log "✅ Verification report email sent"
    else
        log "WARNING: Failed to send email. HTTP $http_code"
    fi
}

################################################################################
# Verify Single Backup
################################################################################
verify_single_backup() {
    local backup_file="$1"
    local filename=$(basename "$backup_file")
    local status="✅ PASS"
    local details=""

    print_color "$BLUE" "\n🔍 Verifying: $filename"

    # Check if file exists
    if [[ ! -f "$backup_file" ]]; then
        print_color "$RED" "  ❌ File not found"
        log "FAILED: $filename - File not found"
        echo "FAILED"
        return 1
    fi

    # Check file size
    local size=$(stat -f%z "$backup_file" 2>/dev/null || stat -c%s "$backup_file")
    if [[ $size -lt 1000 ]]; then
        print_color "$RED" "  ❌ File too small ($size bytes) - likely corrupted"
        log "FAILED: $filename - File too small ($size bytes)"
        echo "FAILED"
        return 1
    fi
    print_color "$GREEN" "  ✅ File size: $(du -h "$backup_file" | cut -f1)"

    # Test if archive is valid
    if ! tar -tzf "$backup_file" > /dev/null 2>&1; then
        print_color "$RED" "  ❌ Archive is corrupted (tar verification failed)"
        log "FAILED: $filename - Archive corrupted"
        echo "FAILED"
        return 1
    fi
    print_color "$GREEN" "  ✅ Archive structure valid"

    # Extract to temporary location for content verification
    local temp_dir=$(mktemp -d)
    if ! tar -xzf "$backup_file" -C "$temp_dir" 2>/dev/null; then
        print_color "$RED" "  ❌ Failed to extract archive"
        log "FAILED: $filename - Extraction failed"
        rm -rf "$temp_dir"
        echo "FAILED"
        return 1
    fi
    print_color "$GREEN" "  ✅ Archive extraction successful"

    # Find the backup directory
    local backup_content=$(find "$temp_dir" -name "mesh-backup-*" -type d | head -1)

    # Check for required files
    if [[ ! -f "$backup_content/stats.db" ]]; then
        print_color "$RED" "  ❌ stats.db not found in backup"
        log "FAILED: $filename - stats.db missing"
        rm -rf "$temp_dir"
        echo "FAILED"
        return 1
    fi
    print_color "$GREEN" "  ✅ stats.db present"

    # Check stats.db size
    local db_size=$(stat -f%z "$backup_content/stats.db" 2>/dev/null || stat -c%s "$backup_content/stats.db")
    if [[ $db_size -lt 1000 ]]; then
        print_color "$YELLOW" "  ⚠️  stats.db is very small ($db_size bytes)"
    else
        print_color "$GREEN" "  ✅ stats.db size: $(du -h "$backup_content/stats.db" | cut -f1)"
    fi

    # Verify checksums if present
    if [[ -f "$backup_content/checksums.txt" ]]; then
        cd "$backup_content"
        if sha256sum -c checksums.txt 2>/dev/null >/dev/null || shasum -a 256 -c checksums.txt 2>/dev/null >/dev/null; then
            print_color "$GREEN" "  ✅ Checksums verified"
        else
            print_color "$RED" "  ❌ Checksum verification failed"
            log "FAILED: $filename - Checksum mismatch"
            rm -rf "$temp_dir"
            cd - > /dev/null
            echo "FAILED"
            return 1
        fi
        cd - > /dev/null
    else
        print_color "$YELLOW" "  ⚠️  No checksums file found (older backup)"
    fi

    # Check database.json if present
    if [[ -f "$backup_content/database.json" ]]; then
        print_color "$GREEN" "  ✅ database.json present"
    fi

    # Cleanup
    rm -rf "$temp_dir"

    print_color "$GREEN" "  ✅ All checks passed for $filename"
    log "PASSED: $filename"
    echo "PASSED"
    return 0
}

################################################################################
# Verify All Backups
################################################################################
verify_all_backups() {
    local total=0
    local passed=0
    local failed=0
    local report=""

    print_color "$BLUE" "\n╔════════════════════════════════════════════════════════════╗"
    print_color "$BLUE" "║       Verifying All Local Backups                         ║"
    print_color "$BLUE" "╚════════════════════════════════════════════════════════════╝"

    if [[ ! -d "$BACKUP_DIR" ]]; then
        print_color "$RED" "\n❌ Backup directory not found: $BACKUP_DIR"
        return 1
    fi

    local backups=($(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r))

    if [[ ${#backups[@]} -eq 0 ]]; then
        print_color "$YELLOW" "\n⚠️  No backups found to verify."
        return 0
    fi

    print_color "$BLUE" "\nFound ${#backups[@]} backup(s) to verify...\n"

    for backup in "${backups[@]}"; do
        total=$((total + 1))
        local filename=$(basename "$backup")

        if verify_single_backup "$backup" > /dev/null; then
            passed=$((passed + 1))
            report="${report}✅ ${filename}\n"
        else
            failed=$((failed + 1))
            report="${report}❌ ${filename}\n"
        fi
    done

    # Summary
    print_color "$BLUE" "\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    print_color "$BLUE" "📊 Verification Summary:"
    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo -e "Total Backups: $total"
    print_color "$GREEN" "Passed: $passed"
    if [[ $failed -gt 0 ]]; then
        print_color "$RED" "Failed: $failed"
    else
        echo "Failed: $failed"
    fi
    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ $failed -eq 0 ]]; then
        print_color "$GREEN" "\n✅ All backups verified successfully!"
    else
        print_color "$RED" "\n⚠️  Some backups failed verification. Check logs for details."
    fi

    # Send email report
    send_verification_email "$total" "$passed" "$failed" "$(echo -e "$report")"

    if [[ $failed -gt 0 ]]; then
        return 1
    fi
    return 0
}

################################################################################
# Main
################################################################################
main() {
    mkdir -p "$LOG_DIR"

    print_color "$BLUE" "\n╔════════════════════════════════════════════════════════════╗"
    print_color "$BLUE" "║       Mesh Optimizer - Backup Verification Tool           ║"
    print_color "$BLUE" "╚════════════════════════════════════════════════════════════╝"

    log "=========================================="
    log "Starting backup verification..."

    # Load environment variables if .env exists
    if [[ -f "/root/mesh-optimizer/.env" ]]; then
        set -a
        source /root/mesh-optimizer/.env
        set +a
    fi

    if [[ $# -eq 0 ]]; then
        # No arguments - verify all backups
        verify_all_backups
    elif [[ "$1" == "latest" ]]; then
        # Verify latest backup
        local latest=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r | head -1)
        if [[ -z "$latest" ]]; then
            print_color "$RED" "\n❌ No backups found."
            exit 1
        fi
        verify_single_backup "$latest"
    else
        # Verify specific backup by timestamp
        local timestamp="$1"
        local backup_file="$BACKUP_DIR/mesh-backup-${timestamp}.tar.gz"
        verify_single_backup "$backup_file"
    fi

    log "Verification completed"
    log "=========================================="
}

################################################################################
# Execute Main Function
################################################################################
main "$@"
