#!/bin/bash

################################################################################
# Mesh Optimizer - Backup System Setup Script
################################################################################
# This script sets up the automated backup system.
#
# What it does:
# 1. Creates necessary directories
# 2. Sets up cron jobs for automated backups
# 3. Configures environment variables
# 4. Tests the backup system
# 5. Sends a test email
#
# Usage:
#   ./setup.sh
################################################################################

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

################################################################################
# Print colored message
################################################################################
print_color() {
    local color=$1
    shift
    echo -e "${color}$@${NC}"
}

################################################################################
# Print header
################################################################################
print_header() {
    print_color "$BLUE" "\n╔════════════════════════════════════════════════════════════╗"
    print_color "$BLUE" "║       Mesh Optimizer - Backup System Setup                ║"
    print_color "$BLUE" "╚════════════════════════════════════════════════════════════╝\n"
}

################################################################################
# Check if running as root
################################################################################
check_root() {
    if [[ $EUID -ne 0 ]]; then
        print_color "$RED" "❌ This script must be run as root"
        echo "   Please run: sudo $0"
        exit 1
    fi
}

################################################################################
# Check environment variables
################################################################################
check_environment() {
    print_color "$BLUE" "\n📋 Checking environment configuration..."

    local env_file="/root/mesh-optimizer/.env"

    if [[ ! -f "$env_file" ]]; then
        print_color "$RED" "❌ .env file not found at $env_file"
        exit 1
    fi

    # Load environment variables
    set -a
    source "$env_file"
    set +a

    local missing_vars=()

    # Check Storage Box configuration
    if [[ -z "${STORAGE_BOX_USER:-}" ]]; then
        missing_vars+=("STORAGE_BOX_USER")
    fi
    if [[ -z "${STORAGE_BOX_HOST:-}" ]]; then
        missing_vars+=("STORAGE_BOX_HOST")
    fi

    # Check email configuration
    if [[ -z "${RESEND_API_KEY:-}" ]]; then
        missing_vars+=("RESEND_API_KEY")
    fi
    if [[ -z "${BACKUP_EMAIL:-}" ]]; then
        missing_vars+=("BACKUP_EMAIL")
    fi

    if [[ ${#missing_vars[@]} -gt 0 ]]; then
        print_color "$RED" "❌ Missing required environment variables in .env:"
        for var in "${missing_vars[@]}"; do
            echo "   - $var"
        done
        echo ""
        print_color "$YELLOW" "📝 Please add these to /root/mesh-optimizer/.env:"
        echo ""
        echo "# Storage Box Configuration"
        echo "STORAGE_BOX_USER=u518013"
        echo "STORAGE_BOX_HOST=u518013.your-storagebox.de"
        echo "STORAGE_BOX_PATH=/backups"
        echo ""
        echo "# Email Configuration"
        echo "RESEND_API_KEY=re_your_key_here"
        echo "BACKUP_EMAIL=your-email@example.com"
        exit 1
    fi

    print_color "$GREEN" "✅ Environment variables configured"
}

################################################################################
# Create directories
################################################################################
create_directories() {
    print_color "$BLUE" "\n📁 Creating backup directories..."

    mkdir -p /root/backups
    mkdir -p /root/backups/pre-restore
    mkdir -p /var/log/mesh

    chmod 755 /root/backups
    chmod 755 /var/log/mesh

    print_color "$GREEN" "✅ Directories created"
}

################################################################################
# Make scripts executable
################################################################################
setup_scripts() {
    print_color "$BLUE" "\n🔧 Setting up backup scripts..."

    local script_dir="/root/mesh-optimizer/scripts/backup"

    chmod +x "$script_dir/backup.sh"
    chmod +x "$script_dir/restore.sh"
    chmod +x "$script_dir/verify_backup.sh"
    chmod +x "$script_dir/health_check.sh"
    chmod +x "$script_dir/send_report.sh"
    chmod +x "$script_dir/send_html_report.sh"
    chmod +x "$script_dir/listmonk-backup.sh"
    chmod +x "$script_dir/listmonk-restore.sh"
    chmod +x "/root/mesh-optimizer/scripts/reports/daily_stats.sh"
    chmod +x "/root/mesh-optimizer/scripts/reports/daily_metrics.sh"
    chmod +x "/root/mesh-optimizer/scripts/reports/blender_health_check.sh"

    # Create listmonk backup directory
    mkdir -p /root/backups/listmonk
    mkdir -p /root/backups/listmonk/pre-restore
    mkdir -p /root/backups/listmonk/user-exports

    print_color "$GREEN" "✅ Scripts configured"
}

################################################################################
# Test Storage Box connection
################################################################################
test_storage_box() {
    print_color "$BLUE" "\n🔌 Testing Storage Box connection..."

    # Load environment
    set -a
    source /root/mesh-optimizer/.env
    set +a

    if timeout 10 ssh -p 23 -o ConnectTimeout=5 -o StrictHostKeyChecking=no \
        "${STORAGE_BOX_USER}@${STORAGE_BOX_HOST}" "echo 'Connection test successful'" 2>/dev/null; then
        print_color "$GREEN" "✅ Storage Box connection successful"
        return 0
    else
        print_color "$RED" "❌ Failed to connect to Storage Box"
        print_color "$YELLOW" "⚠️  Please check:"
        echo "   - Storage Box credentials in .env"
        echo "   - SSH key is added to Storage Box (should be done already)"
        echo "   - Network connectivity"
        return 1
    fi
}

################################################################################
# Setup cron jobs
################################################################################
setup_cron() {
    print_color "$BLUE" "\n⏰ Setting up automated backup schedule..."

    local backup_script="/root/mesh-optimizer/scripts/backup/backup.sh"
    local listmonk_backup_script="/root/mesh-optimizer/scripts/backup/listmonk-backup.sh"
    local verify_script="/root/mesh-optimizer/scripts/backup/verify_backup.sh"
    local health_script="/root/mesh-optimizer/scripts/backup/health_check.sh"
    local stats_script="/root/mesh-optimizer/scripts/reports/daily_stats.sh"
    local metrics_script="/root/mesh-optimizer/scripts/reports/daily_metrics.sh"
    local blender_check_script="/root/mesh-optimizer/scripts/reports/blender_health_check.sh"

    # Remove existing cron jobs for mesh backup (if any) - safer approach
    if crontab -l >/dev/null 2>&1; then
        crontab -l 2>/dev/null | grep -v "mesh-optimizer/scripts" | crontab -
    fi

    # Add new cron jobs with PROPER environment loading
    (crontab -l 2>/dev/null || echo ""; cat <<EOF

# Mesh Optimizer Backup System
# Backup every 6 hours (at 00:00, 06:00, 12:00, 18:00)
0 */6 * * * /bin/bash -c 'set -a; source /root/mesh-optimizer/.env; set +a; bash $backup_script' >> /var/log/mesh/backup.log 2>&1

# Listmonk Backup daily at 3 AM
0 3 * * * /bin/bash -c 'set -a; source /root/mesh-optimizer/.env; set +a; bash $listmonk_backup_script' >> /var/log/mesh/listmonk-backup.log 2>&1

# Verify backups weekly (every Sunday at 2 AM)
0 2 * * 0 /bin/bash -c 'set -a; source /root/mesh-optimizer/.env; set +a; bash $verify_script' >> /var/log/mesh/verify.log 2>&1

# Health Check (Hourly)
0 * * * * /bin/bash -c 'set -a; source /root/mesh-optimizer/.env; set +a; bash $health_script' >> /var/log/mesh/health_check.log 2>&1

# Daily Stats Report (Daily at 00:00 UTC)
0 0 * * * /bin/bash -c 'set -a; source /root/mesh-optimizer/.env; set +a; bash $stats_script' >> /var/log/mesh/daily_stats.log 2>&1

# Daily Metrics Email Report (00:01 EST = 05:01 UTC)
1 5 * * * /bin/bash -c 'set -a; source /root/mesh-optimizer/.env; set +a; bash $metrics_script' >> /var/log/mesh/daily_metrics.log 2>&1

# Blender Health Watchdog (Every 30 mins)
*/30 * * * * /bin/bash -c 'set -a; source /root/mesh-optimizer/.env; set +a; bash $blender_check_script' >> /var/log/mesh/blender_monitor.log 2>&1

# Upload Cleanup (Every 15 mins)
*/15 * * * * find /root/mesh-optimizer/uploads -type f -mmin +15 -delete 2>&1 | logger -t upload-cleanup

EOF
    ) | crontab -

    print_color "$GREEN" "✅ Cron jobs configured:"
    echo "   - Mesh Backup: Every 6 hours"
    echo "   - Listmonk Backup: Daily (3:00 AM)"
    echo "   - Verification: Weekly (Sunday 2:00 AM)"
    echo "   - Health Check: Hourly"
    echo "   - Daily Stats: Daily (00:00 UTC)"
    echo "   - Metrics Email: Daily (05:01 UTC / 00:01 EST)"
    echo "   - Blender Watchdog: Every 30 mins"
    echo "   - Upload Cleanup: Every 15 mins"
}

################################################################################
# Run test backup
################################################################################
run_test_backup() {
    print_color "$BLUE" "\n🧪 Running test backup..."

    local backup_script="/root/mesh-optimizer/scripts/backup/backup.sh"

    # Load environment
    export $(cat /root/mesh-optimizer/.env | grep -v '^#' | xargs)

    if bash "$backup_script"; then
        print_color "$GREEN" "✅ Test backup completed successfully"
        return 0
    else
        print_color "$RED" "❌ Test backup failed"
        print_color "$YELLOW" "⚠️  Check logs at: /var/log/mesh/backup.log"
        return 1
    fi
}

################################################################################
# Display next steps
################################################################################
display_next_steps() {
    print_color "$GREEN" "\n╔════════════════════════════════════════════════════════════╗"
    print_color "$GREEN" "║       ✅ Backup System Setup Complete!                     ║"
    print_color "$GREEN" "╚════════════════════════════════════════════════════════════╝\n"

    print_color "$BLUE" "📚 Quick Reference:\n"

    echo "Manual backup (mesh-optimizer):"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/backup.sh"
    echo ""

    echo "Manual backup (listmonk):"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/listmonk-backup.sh"
    echo ""

    echo "List available backups:"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/restore.sh"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh"
    echo ""

    echo "Restore a backup:"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/restore.sh 20250108_120000"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh 20250108_120000"
    echo ""

    echo "Export single Listmonk user:"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/listmonk-restore.sh user email@example.com"
    echo ""

    echo "Verify backups:"
    print_color "$YELLOW" "  bash /root/mesh-optimizer/scripts/backup/verify_backup.sh"
    echo ""

    echo "View backup logs:"
    print_color "$YELLOW" "  tail -f /var/log/mesh/backup.log"
    print_color "$YELLOW" "  tail -f /var/log/mesh/listmonk-backup.log"
    echo ""

    echo "View cron schedule:"
    print_color "$YELLOW" "  crontab -l"
    echo ""

    print_color "$BLUE" "📧 Check your email (${BACKUP_EMAIL}) for backup notification!"
    echo ""

    print_color "$BLUE" "📦 Backups are stored in:"
    echo "   - Local: /root/backups (7 days retention)"
    echo "   - Storage Box: ${STORAGE_BOX_HOST}:${STORAGE_BOX_PATH} (30 days retention)"
    echo ""

    print_color "$GREEN" "🎉 Your database is now protected!\n"
}

################################################################################
# Main
################################################################################
main() {
    print_header

    check_root
    check_environment
    create_directories
    setup_scripts

    if ! test_storage_box; then
        print_color "$YELLOW" "\n⚠️  Storage Box connection failed, but setup will continue."
        print_color "$YELLOW" "   Backups will still work locally. Fix Storage Box config later."
        read -p "Continue anyway? (y/n): " continue
        if [[ "$continue" != "y" ]]; then
            print_color "$RED" "Setup cancelled."
            exit 1
        fi
    fi

    setup_cron

    if ! run_test_backup; then
        print_color "$YELLOW" "\n⚠️  Test backup failed. Check the error above."
        read -p "Mark setup as complete anyway? (y/n): " continue
        if [[ "$continue" != "y" ]]; then
            print_color "$RED" "Setup incomplete. Fix errors and run again."
            exit 1
        fi
    fi

    display_next_steps
}

################################################################################
# Execute Main Function
################################################################################
main "$@"
