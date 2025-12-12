#!/bin/bash

################################################################################
# Mesh Optimizer - Application Health Check
################################################################################
# This script monitors the SQLite database for signs of application failure.
# It is designed to be run via cron (e.g., hourly).
#
# CHECKS PERFORMED:
# 1. High Failure Rate: Alerts if >50% of jobs failed in the last hour.
#    (Requires minimum 5 jobs to trigger to avoid noise)
#
# USAGE:
#   ./health_check.sh
################################################################################

set -euo pipefail

# Configuration
PROJECT_DIR="/root/mesh-optimizer"
DB_PATH="$PROJECT_DIR/server/stats.db"
SEND_REPORT_SCRIPT="$PROJECT_DIR/scripts/backup/send_report.sh"
LOG_FILE="/var/log/mesh/health_check.log"

# Load environment variables if needed
if [ -f "$PROJECT_DIR/.env" ]; then
    set -a
    source "$PROJECT_DIR/.env"
    set +a
fi

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")"

# ------------------------------------------------------------------------------
# Run Health Analysis (Python)
# ------------------------------------------------------------------------------
# We use Python to interact with SQLite and calculate statistics reliably.
# The script outputs an alert message ONLY if thresholds are exceeded.
# ------------------------------------------------------------------------------

REPORT=$(python3 -c "
import sqlite3
import os
import sys
from datetime import datetime, timedelta

db_path = '$DB_PATH'
hours_back = 1
min_jobs = 5
fail_threshold = 0.50 # 50% failure rate triggers alarm

if not os.path.exists(db_path):
    # Database doesn't exist yet, nothing to check
    sys.exit(0)

try:
    conn = sqlite3.connect(db_path)
    cursor = conn.cursor()

    # Query job status distribution for the last X hours
    # SQLite 'datetime' function handles the calculation
    # Check if 'source' column exists (migration safety)
    try:
        cursor.execute(\"SELECT source FROM job_history LIMIT 1\")
        has_source = True
    except sqlite3.OperationalError:
        has_source = False

    if has_source:
        cursor.execute(\"\"\"
            SELECT status, source, COUNT(*)
            FROM job_history
            WHERE timestamp > datetime('now', ?)
            GROUP BY status, source
        \"\"\", (f'-{hours_back} hour',))
    else:
        cursor.execute(\"\"\"
            SELECT status, 'unknown', COUNT(*)
            FROM job_history
            WHERE timestamp > datetime('now', ?)
            GROUP BY status
        \"\"\", (f'-{hours_back} hour',))

    rows = cursor.fetchall()
    conn.close()

    stats = {}
    source_stats = {}
    total_jobs = 0

    for status, source, count in rows:
        stats[status] = stats.get(status, 0) + count

        # Track failures by source
        if status != 'success':
            source_stats[source] = source_stats.get(source, 0) + count

        total_jobs += count

    # If not enough data, skip check to avoid false alarms
    if total_jobs < min_jobs:
        sys.exit(0)

    # Calculate Failure Rate
    # We consider 'success' as the only good state.
    # 'timeout', 'worker_error', 'system_error', 'failed' are bad.
    success_count = stats.get('success', 0)
    fail_count = total_jobs - success_count
    fail_rate = fail_count / total_jobs

    if fail_rate >= fail_threshold:
        print(f\"🚨 HEALTH ALERT: High Job Failure Rate Detected\")
        print(f\"Time Period: Last {hours_back} hour(s)\")
        print(f\"Failure Rate: {fail_rate*100:.1f}%\")
        print(f\"Total Jobs: {total_jobs}\")
        print(f\"Successful: {success_count}\")
        print(f\"Failed: {fail_count}\")
        print(\"\nDetailed Status Breakdown:\")
        for status, count in stats.items():
            print(f\"  - {status}: {count}\")

        if source_stats:
            print(\"\nFailures by Source:\")
            for source, count in source_stats.items():
                print(f\"  - {source}: {count}\")

        print(\"\n---\")
        print(\"RECOMMENDED ACTIONS:\")
        print(\"1. Check server logs: tail -n 50 /var/log/syslog\")
        print(\"2. Verify Blender worker status\")
        print(\"3. Check disk space (df -h)\")

except Exception as e:
    print(f\"Error performing health check: {e}\")
    sys.exit(1)
")

# ------------------------------------------------------------------------------
# Handle Report
# ------------------------------------------------------------------------------

if [[ -n "$REPORT" ]]; then
    echo "[$(date)] Issues detected. Sending alert." | tee -a "$LOG_FILE"

    # Extract the first line as the subject
    SUBJECT=$(echo "$REPORT" | head -n 1)

    if [[ -x "$SEND_REPORT_SCRIPT" ]]; then
        "$SEND_REPORT_SCRIPT" "$SUBJECT" "$REPORT" >> "$LOG_FILE" 2>&1
    else
        echo "ERROR: Send report script not executable at $SEND_REPORT_SCRIPT" | tee -a "$LOG_FILE"
    fi
else
    # Silence is golden
    echo "[$(date)] Health check passed. No issues." >> "$LOG_FILE"
fi
