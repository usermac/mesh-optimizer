#!/bin/bash

################################################################################
# Mesh Optimizer - Backup System Test
################################################################################
# Tests the full backup workflow:
# 1. Creates a backup
# 2. Verifies backup file was created locally
# 3. Verifies backup was uploaded to Hetzner Storage Box
# 4. Verifies backup contains transaction data
# 5. Tests that data can be extracted and read
#
# Usage:
#   ./test_backup.sh           # Run all tests
#   ./test_backup.sh --quick   # Skip Hetzner upload verification
################################################################################

set -euo pipefail

# Configuration
BACKUP_DIR="/root/backups"
DB_DIR="/root/mesh-optimizer/server"
STATS_DB="$DB_DIR/stats.db"
LOG_FILE="/var/log/mesh/test_backup.log"

# Storage Box Configuration (from environment)
STORAGE_BOX_USER="${STORAGE_BOX_USER:-}"
STORAGE_BOX_HOST="${STORAGE_BOX_HOST:-}"
STORAGE_BOX_PATH="${STORAGE_BOX_PATH:-/backups}"
STORAGE_BOX_PASSWORD="${STORAGE_BOX_PASSWORD:-}"

# Test tracking
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
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
# Test Result Tracking
################################################################################
test_pass() {
    TESTS_RUN=$((TESTS_RUN + 1))
    TESTS_PASSED=$((TESTS_PASSED + 1))
    print_color "$GREEN" "  PASS: $1"
    log "PASS: $1"
}

test_fail() {
    TESTS_RUN=$((TESTS_RUN + 1))
    TESTS_FAILED=$((TESTS_FAILED + 1))
    print_color "$RED" "  FAIL: $1"
    log "FAIL: $1"
}

################################################################################
# Test: Database has transaction data
################################################################################
test_database_has_data() {
    print_color "$BLUE" "\n[Test] Checking database has transaction data..."

    if [[ ! -f "$STATS_DB" ]]; then
        test_fail "stats.db does not exist at $STATS_DB"
        return 1
    fi

    local tx_count=$(sqlite3 "$STATS_DB" "SELECT COUNT(*) FROM credit_transactions;" 2>/dev/null || echo "0")

    if [[ "$tx_count" -gt 0 ]]; then
        test_pass "Database has $tx_count transaction(s)"
        return 0
    else
        test_fail "Database has no transactions (count: $tx_count)"
        return 1
    fi
}

################################################################################
# Test: Backup creates file
################################################################################
test_backup_creates_file() {
    print_color "$BLUE" "\n[Test] Creating new backup..."

    # Get timestamp before backup
    local before_count=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f 2>/dev/null | wc -l)

    # Run backup script
    if ! bash /root/mesh-optimizer/scripts/backup/backup.sh > /tmp/backup_test_output.txt 2>&1; then
        test_fail "Backup script failed to execute"
        cat /tmp/backup_test_output.txt
        return 1
    fi

    # Check new backup was created
    local after_count=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f 2>/dev/null | wc -l)

    if [[ "$after_count" -gt "$before_count" ]]; then
        test_pass "New backup file created (total: $after_count)"
        return 0
    else
        test_fail "No new backup file was created"
        return 1
    fi
}

################################################################################
# Test: Backup file is valid archive
################################################################################
test_backup_is_valid_archive() {
    print_color "$BLUE" "\n[Test] Verifying backup archive integrity..."

    local latest_backup=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r | head -1)

    if [[ -z "$latest_backup" ]]; then
        test_fail "No backup files found"
        return 1
    fi

    if tar -tzf "$latest_backup" > /dev/null 2>&1; then
        test_pass "Archive is valid: $(basename "$latest_backup")"
        return 0
    else
        test_fail "Archive is corrupted: $(basename "$latest_backup")"
        return 1
    fi
}

################################################################################
# Test: Backup contains transaction data
################################################################################
test_backup_contains_transactions() {
    print_color "$BLUE" "\n[Test] Verifying backup contains transaction data..."

    local latest_backup=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r | head -1)

    if [[ -z "$latest_backup" ]]; then
        test_fail "No backup files found"
        return 1
    fi

    # Extract to temp directory
    local temp_dir=$(mktemp -d)
    tar -xzf "$latest_backup" -C "$temp_dir"

    # Find stats.db in extracted backup
    local backup_db=$(find "$temp_dir" -name "stats.db" | head -1)

    if [[ -z "$backup_db" ]]; then
        test_fail "stats.db not found in backup"
        rm -rf "$temp_dir"
        return 1
    fi

    # Check transaction count in backup
    local tx_count=$(sqlite3 "$backup_db" "SELECT COUNT(*) FROM credit_transactions;" 2>/dev/null || echo "0")

    rm -rf "$temp_dir"

    if [[ "$tx_count" -gt 0 ]]; then
        test_pass "Backup contains $tx_count transaction(s)"
        return 0
    else
        test_fail "Backup contains no transactions"
        return 1
    fi
}

################################################################################
# Test: Backup checksums match
################################################################################
test_backup_checksums() {
    print_color "$BLUE" "\n[Test] Verifying backup checksums..."

    local latest_backup=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r | head -1)

    if [[ -z "$latest_backup" ]]; then
        test_fail "No backup files found"
        return 1
    fi

    # Extract to temp directory
    local temp_dir=$(mktemp -d)
    tar -xzf "$latest_backup" -C "$temp_dir"

    # Find checksums file
    local checksums_file=$(find "$temp_dir" -name "checksums.txt" | head -1)

    if [[ -z "$checksums_file" ]]; then
        test_fail "checksums.txt not found in backup"
        rm -rf "$temp_dir"
        return 1
    fi

    # Verify checksums
    local backup_dir=$(dirname "$checksums_file")
    cd "$backup_dir"

    if sha256sum -c checksums.txt > /dev/null 2>&1 || shasum -a 256 -c checksums.txt > /dev/null 2>&1; then
        test_pass "All checksums verified"
        cd - > /dev/null
        rm -rf "$temp_dir"
        return 0
    else
        test_fail "Checksum verification failed"
        cd - > /dev/null
        rm -rf "$temp_dir"
        return 1
    fi
}

################################################################################
# Test: Backup uploaded to Hetzner Storage Box
################################################################################
test_hetzner_upload() {
    print_color "$BLUE" "\n[Test] Verifying backup exists on Hetzner Storage Box..."

    if [[ -z "$STORAGE_BOX_USER" ]] || [[ -z "$STORAGE_BOX_HOST" ]]; then
        print_color "$YELLOW" "  SKIP: Hetzner Storage Box not configured"
        return 0
    fi

    # Get latest local backup name
    local latest_backup=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r | head -1)
    local backup_name=$(basename "$latest_backup")

    # Check if it exists on Storage Box
    local ssh_cmd="ssh -p 23 -o BatchMode=yes -o ConnectTimeout=10"
    if [[ -n "$STORAGE_BOX_PASSWORD" ]] && command -v sshpass >/dev/null 2>&1; then
        ssh_cmd="sshpass -p '${STORAGE_BOX_PASSWORD}' ssh -p 23 -o ConnectTimeout=10"
    fi

    if eval "$ssh_cmd ${STORAGE_BOX_USER}@${STORAGE_BOX_HOST} 'ls ${STORAGE_BOX_PATH}/${backup_name}'" > /dev/null 2>&1; then
        test_pass "Backup exists on Hetzner: $backup_name"
        return 0
    else
        test_fail "Backup not found on Hetzner: $backup_name"
        return 1
    fi
}

################################################################################
# Test: Transaction count matches between live DB and backup
################################################################################
test_transaction_count_matches() {
    print_color "$BLUE" "\n[Test] Comparing transaction counts (live vs backup)..."

    # Get live count
    local live_count=$(sqlite3 "$STATS_DB" "SELECT COUNT(*) FROM credit_transactions;" 2>/dev/null || echo "0")

    # Get backup count
    local latest_backup=$(find "$BACKUP_DIR" -name "mesh-backup-*.tar.gz" -type f | sort -r | head -1)
    local temp_dir=$(mktemp -d)
    tar -xzf "$latest_backup" -C "$temp_dir"
    local backup_db=$(find "$temp_dir" -name "stats.db" | head -1)
    local backup_count=$(sqlite3 "$backup_db" "SELECT COUNT(*) FROM credit_transactions;" 2>/dev/null || echo "0")
    rm -rf "$temp_dir"

    if [[ "$live_count" -eq "$backup_count" ]]; then
        test_pass "Transaction counts match: $live_count"
        return 0
    else
        test_fail "Transaction counts differ: live=$live_count, backup=$backup_count"
        return 1
    fi
}

################################################################################
# Main
################################################################################
main() {
    mkdir -p "$(dirname "$LOG_FILE")"

    print_color "$BLUE" "\n╔════════════════════════════════════════════════════════════╗"
    print_color "$BLUE" "║       Mesh Optimizer - Backup System Test Suite            ║"
    print_color "$BLUE" "╚════════════════════════════════════════════════════════════╝"

    log "=========================================="
    log "Starting backup system tests..."

    # Load environment
    if [[ -f "/root/mesh-optimizer/.env" ]]; then
        set -a
        source /root/mesh-optimizer/.env
        set +a
    fi

    local skip_hetzner=false
    if [[ "${1:-}" == "--quick" ]]; then
        skip_hetzner=true
        print_color "$YELLOW" "\nRunning in quick mode (skipping Hetzner tests)"
    fi

    # Run tests
    test_database_has_data || true
    test_backup_creates_file || true
    test_backup_is_valid_archive || true
    test_backup_contains_transactions || true
    test_backup_checksums || true
    test_transaction_count_matches || true

    if [[ "$skip_hetzner" == false ]]; then
        test_hetzner_upload || true
    fi

    # Summary
    print_color "$BLUE" "\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    print_color "$BLUE" "Test Summary:"
    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Total:  $TESTS_RUN"
    print_color "$GREEN" "  Passed: $TESTS_PASSED"
    if [[ $TESTS_FAILED -gt 0 ]]; then
        print_color "$RED" "  Failed: $TESTS_FAILED"
    else
        echo "  Failed: $TESTS_FAILED"
    fi
    print_color "$BLUE" "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [[ $TESTS_FAILED -eq 0 ]]; then
        print_color "$GREEN" "\n All tests passed!"
        log "All tests passed ($TESTS_PASSED/$TESTS_RUN)"
        exit 0
    else
        print_color "$RED" "\n Some tests failed!"
        log "Tests failed ($TESTS_FAILED/$TESTS_RUN failed)"
        exit 1
    fi
}

main "$@"
