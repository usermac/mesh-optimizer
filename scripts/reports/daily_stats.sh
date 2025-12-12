#!/bin/bash

################################################################################
# Mesh Optimizer - Daily Usage Statistics Report
################################################################################
# Generates a daily email report with KPIs, usage patterns, and technical stats.
# Designed to be run daily via cron (e.g., at 00:00 UTC).
#
# USAGE:
#   ./daily_stats.sh
################################################################################

set -euo pipefail

# Configuration
PROJECT_DIR="/root/mesh-optimizer"
DB_PATH="$PROJECT_DIR/server/stats.db"
SEND_REPORT_SCRIPT="$PROJECT_DIR/scripts/backup/send_report.sh"
LOG_FILE="/var/log/mesh/daily_stats.log"

# Load environment variables if needed
if [ -f "$PROJECT_DIR/.env" ]; then
    set -a
    source "$PROJECT_DIR/.env"
    set +a
fi

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")"

# ------------------------------------------------------------------------------
# Generate Report (Python)
# ------------------------------------------------------------------------------
REPORT=$(python3 -c "
import sqlite3
import os
import sys
import time
from datetime import datetime, timedelta

db_path = '$DB_PATH'

if not os.path.exists(db_path):
    print(\"Error: Database not found at \" + db_path)
    sys.exit(1)

try:
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    cursor = conn.cursor()

    # Time Calculations
    now = datetime.now()
    yesterday = now - timedelta(days=1)

    # Format for job_history (DATETIME string)
    ts_string = yesterday.strftime('%Y-%m-%d %H:%M:%S')

    # Format for credit_transactions (INTEGER millis)
    ts_millis = int(yesterday.timestamp() * 1000)

    # ---------------------------------------------------------
    # 1. The Pulse (KPIs)
    # ---------------------------------------------------------

    # Jobs Stats
    cursor.execute(\"\"\"
        SELECT
            COUNT(*) as total,
            SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END) as success_count,
            SUM(input_size_bytes) as total_input,
            SUM(output_size_bytes) as total_output
        FROM job_history
        WHERE timestamp > ?
    \"\"\", (ts_string,))
    job_stats = cursor.fetchone()

    total_jobs = job_stats['total'] or 0
    success_count = job_stats['success_count'] or 0
    total_input = job_stats['total_input'] or 0
    total_output = job_stats['total_output'] or 0

    success_rate = (success_count / total_jobs * 100) if total_jobs > 0 else 0.0

    # Credits Spent (Sum of negative amounts)
    cursor.execute(\"\"\"
        SELECT SUM(ABS(amount)) as spent
        FROM credit_transactions
        WHERE created_at > ? AND amount < 0
    \"\"\", (ts_millis,))
    credit_row = cursor.fetchone()
    credits_spent = credit_row['spent'] or 0

    # Web vs API Split
    # Check if source column exists first
    try:
        cursor.execute(\"SELECT source, COUNT(*) as cnt FROM job_history WHERE timestamp > ? GROUP BY source\", (ts_string,))
        source_rows = cursor.fetchall()
    except sqlite3.OperationalError:
        # Fallback if column missing
        source_rows = []

    web_count = 0
    api_count = 0
    for row in source_rows:
        s = row['source'].lower() if row['source'] else 'unknown'
        if s == 'web':
            web_count += row['cnt']
        else:
            api_count += row['cnt'] # lump unknown/api together

    # ---------------------------------------------------------
    # 2. Usage Patterns (Time of Day)
    # ---------------------------------------------------------

    # Group by Hour (00-23)
    cursor.execute(\"\"\"
        SELECT strftime('%H', timestamp) as hour, COUNT(*) as cnt
        FROM job_history
        WHERE timestamp > ?
        GROUP BY 1
        ORDER BY 2 DESC
    \"\"\", (ts_string,))
    hourly_rows = cursor.fetchall()

    peak_hour = \"N/A\"
    peak_count = 0
    if hourly_rows:
        peak_hour = f\"{hourly_rows[0]['hour']}:00\"
        peak_count = hourly_rows[0]['cnt']

    # Find quietest hour (simplistic approach: just taking the last row if we have 24 rows,
    # but strictly speaking we might have hours with 0 jobs which won't show up.
    # For a simple report, taking the lowest returned count is fine.)
    quiet_hour = \"N/A\"
    quiet_count = 0
    if hourly_rows:
        quiet_hour = f\"{hourly_rows[-1]['hour']}:00\"
        quiet_count = hourly_rows[-1]['cnt']

    # ---------------------------------------------------------
    # 3. Technical Stats
    # ---------------------------------------------------------

    # Formats
    cursor.execute(\"\"\"
        SELECT input_format, COUNT(*) as cnt
        FROM job_history
        WHERE timestamp > ?
        GROUP BY input_format
        ORDER BY 2 DESC
        LIMIT 1
    \"\"\", (ts_string,))
    format_row = cursor.fetchone()
    top_format = format_row['input_format'] if format_row else \"N/A\"

    # Data Volume formatting
    def format_bytes(size):
        power = 2**10
        n = 0
        power_labels = {0 : '', 1: 'K', 2: 'M', 3: 'G', 4: 'T'}
        while size > power:
            size /= power
            n += 1
        return f\"{size:.1f} {power_labels[n]}B\"

    total_data_str = format_bytes(total_input)

    # Avg Compression
    # (1 - (output / input)) * 100
    avg_compression = 0.0
    if total_input > 0:
        avg_compression = (1.0 - (total_output / total_input)) * 100.0

    conn.close()

    # ---------------------------------------------------------
    # Build Report Output
    # ---------------------------------------------------------
    print(f\"📊 Daily Mesh Optimizer Report ({now.strftime('%Y-%m-%d')})\")
    print(\"\")
    print(\"--- 📈 The Pulse (Last 24h) ---\")
    print(f\"Total Jobs:       {total_jobs}\")
    print(f\"Success Rate:     {success_rate:.1f}%\")
    print(f\"Credits Spent:    {credits_spent}\")
    if total_jobs > 0:
        web_pct = (web_count / total_jobs) * 100
        api_pct = (api_count / total_jobs) * 100
        print(f\"Web / API Split:  {web_pct:.0f}% Web / {api_pct:.0f}% API\")
    else:
        print(f\"Web / API Split:  N/A\")

    print(\"\")
    print(\"--- ⏱️ Usage Patterns ---\")
    print(f\"Peak Hour:        {peak_hour} ({peak_count} jobs)\")
    print(f\"Quiet Hour:       {quiet_hour} ({quiet_count} jobs)\")

    print(\"\")
    print(\"--- 💾 Technical Stats ---\")
    print(f\"Data Processed:   {total_data_str}\")
    print(f\"Avg Compression:  {avg_compression:.1f}% reduction\")
    print(f\"Top Format:       {top_format}\")

except Exception as e:
    print(f\"Error generating report: {e}\")
    sys.exit(1)
")

# ------------------------------------------------------------------------------
# Send Email
# ------------------------------------------------------------------------------

if [[ -n "$REPORT" ]]; then
    # Extract the first line as the subject
    SUBJECT=$(echo "$REPORT" | head -n 1)

    echo "[$(date)] Generated daily report. Sending..." | tee -a "$LOG_FILE"

    if [[ -x "$SEND_REPORT_SCRIPT" ]]; then
        "$SEND_REPORT_SCRIPT" "$SUBJECT" "$REPORT" >> "$LOG_FILE" 2>&1
    else
        echo "ERROR: Send report script not executable at $SEND_REPORT_SCRIPT" | tee -a "$LOG_FILE"
    fi
else
    echo "[$(date)] Failed to generate report." >> "$LOG_FILE"
fi
