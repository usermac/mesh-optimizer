# Append-Only Stats Log

## Overview

`server/stats.log` is an append-only log file that captures daily snapshots of business metrics. Each line represents a 24-hour trailing window as of midnight UTC.

## Format

```
YYYY-DDD | signups: N (free: N, paid: N) | jobs: N (ok: N, fail: N) | credits: +N -N
```

Example:
```
2025-355 | signups: 15 (free: 12, paid: 3) | jobs: 142 (ok: 138, fail: 4) | credits: +500 -89
2025-356 | signups: 8 (free: 6, paid: 2) | jobs: 167 (ok: 161, fail: 6) | credits: +200 -102
```

- `YYYY-DDD`: Year and day of year (001-366), zero-padded
- `signups`: Total new API keys created in the trailing 24h
  - `free`: Keys created via free credits claim
  - `paid`: Keys created via Stripe purchase
- `jobs`: Total optimization jobs processed
  - `ok`: Successful completions
  - `fail`: Failures (worker error, timeout, no output)
- `credits`: Credit flow
  - `+N`: Credits purchased (payments + initial purchases)
  - `-N`: Credits consumed (excluding refunds)

## How It Works

1. A background task runs continuously, sleeping until midnight UTC
2. At midnight, it queries SQLite for the trailing 24 hours of activity
3. One line is appended to `server/stats.log`
4. The task sleeps until the next midnight

## File Location

```
server/stats.log
```

Same directory as `database.json` and `stats.db`. Inherits the same access controls and backup strategy.

## Security

- **Permissions**: File should be `600` or `640` (owner read/write only)
- **Location**: Outside web root, not publicly accessible
- **Content**: Contains aggregate counts only, no PII or API keys
- **Risk level**: Business intelligence, not security-critical

## Querying the Log

### Manual inspection
```bash
# Last 7 days
tail -7 server/stats.log

# All January entries
grep "2025-0[012][0-9]" server/stats.log

# Days with >10 signups
grep -E "signups: [0-9]{2,}" server/stats.log
```

### AI-assisted analysis

Feed the file (or a slice of it) to an LLM:

> "What's the trend in signups over the last 30 days? Any anomalies?"

> "Compare job success rates between the first and second half of the month."

> "Which day had the highest credit consumption? What happened?"

The LLM handles parsing, aggregation, and pattern recognition. You don't need dashboards or charting libraries.

## Size & Retention

- ~80 bytes per line
- ~30 KB per year
- 10 years = ~300 KB

No rotation needed. Let it grow indefinitely.

For year-boundary clarity, you can optionally split into `stats-2025.log`, `stats-2026.log`, but a single file with year-prefixed entries works equally well.

## Reconstruction

If the log file is lost, it can be reconstructed from SQLite by replaying the `collect_daily_snapshot` query for each historical day. The data source is `credit_transactions` and `job_history` tables in `stats.db`.

## SOP: Daily Verification

Not required. The log is fire-and-forget.

If you want to verify it's working:
```bash
# Check last entry date
tail -1 server/stats.log

# Should match yesterday's day-of-year (if checked after midnight UTC)
```

## SOP: Backup

Include `server/stats.log` in your existing backup rotation alongside:
- `server/database.json` (encrypted API keys/customers)
- `server/stats.db` (SQLite transaction ledger)
- `server/pricing.json` (pricing config)

## Troubleshooting

**No new entries appearing?**
1. Check server logs for "Stats snapshot written" or errors
2. Verify SQLite pool is available (check startup logs)
3. Confirm server has been running past midnight UTC

**Entries have all zeros?**
- Normal for days with no activity
- If unexpected, verify SQLite queries against `credit_transactions` and `job_history` tables

**Missing days?**
- Server was down at midnight UTC on those days
- Can be reconstructed from SQLite if needed
