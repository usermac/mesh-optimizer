#!/bin/bash

################################################################################
# Blender Health Watchdog
################################################################################
# Purpose:
#   Monitors for "Zombie" Blender processes (running too long) and OOM kills.
#   Only alerts via email if issues are detected (silent otherwise).
#
# Usage:
#   Add to crontab to run every 30 minutes:
#   */30 * * * * /root/mesh-optimizer/scripts/reports/blender_health_check.sh
################################################################################

set -euo pipefail

# Configuration
# Adjust paths if your deployment structure differs
PROJECT_ROOT="/root/mesh-optimizer"
SEND_REPORT_SCRIPT="$PROJECT_ROOT/scripts/backup/send_report.sh"

# Thresholds
MAX_RUNTIME_SEC=1800  # 30 Minutes
ALERT_SUBJECT="⚠️ Blender Health Alert: Production Server"

# State Tracking
HAS_ISSUES=false
ZOMBIE_REPORT=""
OOM_REPORT=""
BLENDER_REPORT=""

# ------------------------------------------------------------------------------
# 0. Verify Blender Installation
# ------------------------------------------------------------------------------
# Check if Blender is installed and accessible
if ! command -v blender &> /dev/null; then
    HAS_ISSUES=true
    BLENDER_REPORT="${BLENDER_REPORT}
- ❌ CRITICAL: Blender executable not found in PATH
  Action: Install Blender or add to PATH

  Installation options:
  - Ubuntu/Debian: apt-get install blender
  - Download: https://www.blender.org/download/
  - Verify PATH includes Blender location"
fi

# Test Blender can start (quick version check)
if command -v blender &> /dev/null; then
    if ! timeout 5 blender --version &> /dev/null 2>&1; then
        HAS_ISSUES=true
        BLENDER_REPORT="${BLENDER_REPORT}
- ❌ WARNING: Blender cannot execute (--version check failed)
  Action: Check Blender installation and permissions

  Troubleshooting:
  - Run manually: blender --version
  - Check permissions: ls -lh \$(which blender)
  - Verify dependencies: ldd \$(which blender)"
    fi
fi

# ------------------------------------------------------------------------------
# 1. Check for Zombie Processes
# ------------------------------------------------------------------------------
# ps flags:
# -e: Select all processes
# -o: Output specific columns (pid, elapsed seconds, command)
# --no-headers: cleaner output for parsing
# We assume 'blender' is the command name.

while read -r pid etimes cmd; do
    # Check if command contains "blender" (case insensitive just in case)
    if [[ "$cmd" =~ [bB]lender ]]; then
        if [ "$etimes" -gt "$MAX_RUNTIME_SEC" ]; then
            HAS_ISSUES=true
            RUNTIME_MIN=$((etimes / 60))
            # Append to report
            ZOMBIE_REPORT="${ZOMBIE_REPORT}
- PID $pid has been running for ${RUNTIME_MIN}m
  Command: $cmd"
        fi
    fi
done < <(ps -eo pid,etimes,args --no-headers)

# ------------------------------------------------------------------------------
# 2. Check for Recent OOM Kills
# ------------------------------------------------------------------------------
# Scan kernel ring buffer for OOM events involving blender.
# We limit to the last 500 lines to avoid spamming about old events on every run.
# 'grep -i' ensures we catch "Out of memory" or "Killed process".

OOM_LOGS=$(dmesg | tail -n 500 | grep -i "blender" | grep -iE "killed process|out of memory" || true)

if [[ -n "$OOM_LOGS" ]]; then
    HAS_ISSUES=true
    OOM_REPORT="${OOM_REPORT}
System Kernel reports Out-Of-Memory kills involving Blender in recent logs:

$OOM_LOGS"
fi

# ------------------------------------------------------------------------------
# 3. Trigger Alert if Needed
# ------------------------------------------------------------------------------

if [ "$HAS_ISSUES" = true ]; then
    # Construct the email body
    BODY="The Blender Watchdog has detected issues on the production server."

    if [[ -n "$BLENDER_REPORT" ]]; then
        BODY="${BODY}

### 🔧 Blender Installation Issues
Critical problems detected with Blender installation or accessibility.
${BLENDER_REPORT}"
    fi

    if [[ -n "$ZOMBIE_REPORT" ]]; then
        BODY="${BODY}

### 🧟 Stuck Processes (Running > $((MAX_RUNTIME_SEC / 60))m)
These processes may be frozen and consuming CPU/RAM. Consider killing them manually.
${ZOMBIE_REPORT}"
    fi

    if [[ -n "$OOM_REPORT" ]]; then
        BODY="${BODY}

### 💀 Out of Memory Events
The kernel killed Blender processes to save memory. This usually happens when input files are too large for RAM.
${OOM_REPORT}"
    fi

    BODY="${BODY}

--------------------------------------------------
Timestamp: $(date)
Server: $(hostname)"

    # Validate sender script exists
    if [ -x "$SEND_REPORT_SCRIPT" ]; then
        echo "⚠️ Issues detected. Sending report..."
        "$SEND_REPORT_SCRIPT" "$ALERT_SUBJECT" "$BODY"
    else
        echo "❌ Error: Reporting script not executable or found at $SEND_REPORT_SCRIPT" >&2
        echo "--- Report Content ---"
        echo "$BODY"
        exit 1
    fi
else
    # Silent success
    echo "✅ Blender health check passed. No issues found."
fi
