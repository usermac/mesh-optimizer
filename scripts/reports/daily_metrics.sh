#!/bin/bash

################################################################################
# Mesh Optimizer - Daily Metrics Email Report
################################################################################
# Generates and sends a daily email report with key business metrics across
# 24-hour, 7-day, and 30-day windows.
#
# Sent at 00:01 EST (05:01 UTC) to capture the full previous day's data.
#
# USAGE:
#   ./daily_metrics.sh
################################################################################

set -euo pipefail

# Configuration
PROJECT_DIR="/root/mesh-optimizer"
DB_PATH="$PROJECT_DIR/server/stats.db"
SEND_HTML_SCRIPT="$PROJECT_DIR/scripts/backup/send_html_report.sh"
LOG_FILE="/var/log/mesh/daily_metrics.log"

# Load environment variables
if [ -f "$PROJECT_DIR/.env" ]; then
    set -a
    source "$PROJECT_DIR/.env"
    set +a
fi

# Ensure log directory exists
mkdir -p "$(dirname "$LOG_FILE")"

# ------------------------------------------------------------------------------
# Generate HTML Report (Python)
# ------------------------------------------------------------------------------
REPORT=$(python3 << 'PYTHON_SCRIPT'
import sqlite3
import os
import sys
from datetime import datetime, timedelta

db_path = '/root/mesh-optimizer/server/stats.db'
BASE_RATE_USD = 0.50

if not os.path.exists(db_path):
    print("Error: Database not found at " + db_path, file=sys.stderr)
    sys.exit(1)

try:
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    cursor = conn.cursor()

    # Time boundaries
    now = datetime.now()
    now_ms = int(now.timestamp() * 1000)

    windows = {
        '24h': {
            'ms': now_ms - (24 * 60 * 60 * 1000),
            'dt': (now - timedelta(hours=24)).strftime('%Y-%m-%d %H:%M:%S')
        },
        '7d': {
            'ms': now_ms - (7 * 24 * 60 * 60 * 1000),
            'dt': (now - timedelta(days=7)).strftime('%Y-%m-%d %H:%M:%S')
        },
        '30d': {
            'ms': now_ms - (30 * 24 * 60 * 60 * 1000),
            'dt': (now - timedelta(days=30)).strftime('%Y-%m-%d %H:%M:%S')
        }
    }

    def get_metrics(cursor, ms_cutoff, dt_cutoff):
        """Query all metrics for a given time window."""

        # Total Users (cumulative - same for all windows)
        cursor.execute("SELECT COUNT(DISTINCT user_key) FROM credit_transactions")
        total_users = cursor.fetchone()[0] or 0

        # New Signups
        cursor.execute("""
            SELECT COUNT(DISTINCT user_key) FROM credit_transactions
            WHERE description IN ('free_initial_credits', 'Initial Purchase')
            AND created_at >= ?
        """, (ms_cutoff,))
        new_signups = cursor.fetchone()[0] or 0

        # Credits Used (negative amounts, excluding refunds)
        cursor.execute("""
            SELECT COALESCE(SUM(ABS(amount)), 0) FROM credit_transactions
            WHERE amount < 0 AND description NOT LIKE '%refund%' AND created_at >= ?
        """, (ms_cutoff,))
        credits_used = cursor.fetchone()[0] or 0

        # Credits Purchased (for revenue calculation)
        cursor.execute("""
            SELECT COALESCE(SUM(amount), 0) FROM credit_transactions
            WHERE amount > 0 AND description IN ('payment', 'Initial Purchase') AND created_at >= ?
        """, (ms_cutoff,))
        credits_purchased = cursor.fetchone()[0] or 0
        revenue_usd = credits_purchased * BASE_RATE_USD

        # Jobs
        cursor.execute("SELECT COUNT(*) FROM job_history WHERE timestamp >= ?", (dt_cutoff,))
        jobs_total = cursor.fetchone()[0] or 0

        cursor.execute("SELECT COUNT(*) FROM job_history WHERE timestamp >= ? AND status = 'success'", (dt_cutoff,))
        jobs_success = cursor.fetchone()[0] or 0

        jobs_failed = jobs_total - jobs_success
        success_rate = (jobs_success / jobs_total * 100) if jobs_total > 0 else 0.0

        return {
            'total_users': total_users,
            'new_signups': new_signups,
            'credits_used': credits_used,
            'revenue_usd': revenue_usd,
            'jobs_total': jobs_total,
            'jobs_failed': jobs_failed,
            'success_rate': success_rate
        }

    # Collect metrics for all windows
    metrics = {}
    for period, bounds in windows.items():
        metrics[period] = get_metrics(cursor, bounds['ms'], bounds['dt'])

    conn.close()

    # Build HTML Report
    report_date = (now - timedelta(days=1)).strftime('%Y-%m-%d')

    html = f'''<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; padding: 20px; background: #f5f5f5; }}
        .container {{ max-width: 600px; margin: 0 auto; background: white; border-radius: 8px; padding: 24px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }}
        h1 {{ color: #333; font-size: 24px; margin-bottom: 8px; }}
        .date {{ color: #666; font-size: 14px; margin-bottom: 24px; }}
        table {{ width: 100%; border-collapse: collapse; margin: 16px 0; }}
        th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #eee; }}
        th {{ background: #f8f9fa; font-weight: 600; color: #333; }}
        td {{ color: #555; }}
        .metric-name {{ font-weight: 500; }}
        .number {{ font-family: 'SF Mono', Monaco, monospace; text-align: right; }}
        .footer {{ margin-top: 24px; padding-top: 16px; border-top: 1px solid #eee; color: #888; font-size: 12px; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Daily Metrics Report</h1>
        <div class="date">{report_date}</div>

        <table>
            <thead>
                <tr>
                    <th>Metric</th>
                    <th style="text-align: right;">24 Hours</th>
                    <th style="text-align: right;">7 Days</th>
                    <th style="text-align: right;">30 Days</th>
                </tr>
            </thead>
            <tbody>
                <tr>
                    <td class="metric-name">New Users</td>
                    <td class="number">{metrics['24h']['new_signups']}</td>
                    <td class="number">{metrics['7d']['new_signups']}</td>
                    <td class="number">{metrics['30d']['new_signups']}</td>
                </tr>
                <tr>
                    <td class="metric-name">Total Users</td>
                    <td class="number">{metrics['24h']['total_users']}</td>
                    <td class="number">{metrics['7d']['total_users']}</td>
                    <td class="number">{metrics['30d']['total_users']}</td>
                </tr>
                <tr>
                    <td class="metric-name">Credits Used</td>
                    <td class="number">{metrics['24h']['credits_used']}</td>
                    <td class="number">{metrics['7d']['credits_used']}</td>
                    <td class="number">{metrics['30d']['credits_used']}</td>
                </tr>
                <tr>
                    <td class="metric-name">Revenue ($)</td>
                    <td class="number">${metrics['24h']['revenue_usd']:.2f}</td>
                    <td class="number">${metrics['7d']['revenue_usd']:.2f}</td>
                    <td class="number">${metrics['30d']['revenue_usd']:.2f}</td>
                </tr>
                <tr>
                    <td class="metric-name">Jobs Processed</td>
                    <td class="number">{metrics['24h']['jobs_total']}</td>
                    <td class="number">{metrics['7d']['jobs_total']}</td>
                    <td class="number">{metrics['30d']['jobs_total']}</td>
                </tr>
                <tr>
                    <td class="metric-name">Jobs Failed</td>
                    <td class="number">{metrics['24h']['jobs_failed']}</td>
                    <td class="number">{metrics['7d']['jobs_failed']}</td>
                    <td class="number">{metrics['30d']['jobs_failed']}</td>
                </tr>
                <tr>
                    <td class="metric-name">Success Rate</td>
                    <td class="number">{metrics['24h']['success_rate']:.1f}%</td>
                    <td class="number">{metrics['7d']['success_rate']:.1f}%</td>
                    <td class="number">{metrics['30d']['success_rate']:.1f}%</td>
                </tr>
            </tbody>
        </table>

        <div class="footer">
            Generated at {now.strftime('%Y-%m-%d %H:%M:%S')} UTC
        </div>
    </div>
</body>
</html>'''

    print(html)

except Exception as e:
    print(f"Error generating report: {e}", file=sys.stderr)
    sys.exit(1)
PYTHON_SCRIPT
)

# ------------------------------------------------------------------------------
# Send HTML Email
# ------------------------------------------------------------------------------

if [[ -n "$REPORT" ]]; then
    SUBJECT="Daily Metrics Report - $(date -d 'yesterday' '+%Y-%m-%d' 2>/dev/null || date -v-1d '+%Y-%m-%d')"

    echo "[$(date)] Generated daily metrics report. Sending..." | tee -a "$LOG_FILE"

    if [[ -x "$SEND_HTML_SCRIPT" ]]; then
        "$SEND_HTML_SCRIPT" "$SUBJECT" "$REPORT" >> "$LOG_FILE" 2>&1
        echo "[$(date)] Email sent successfully." | tee -a "$LOG_FILE"
    else
        echo "[$(date)] ERROR: HTML report script not executable at $SEND_HTML_SCRIPT" | tee -a "$LOG_FILE"
        exit 1
    fi
else
    echo "[$(date)] Failed to generate metrics report." | tee -a "$LOG_FILE"
    exit 1
fi
