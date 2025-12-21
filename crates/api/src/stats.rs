use anyhow::Result;
use chrono::{Datelike, Utc};
use sqlx::{Pool, Sqlite};
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tracing::{error, info};

const STATS_LOG_FILE: &str = "server/stats.log";

/// Snapshot of daily metrics captured at end of day
#[derive(Debug)]
pub struct DailySnapshot {
    pub year: i32,
    pub day_of_year: u32,
    pub signups_24h: i64,
    pub signups_free_24h: i64,
    pub signups_paid_24h: i64,
    pub api_calls_24h: i64,
    pub jobs_24h: i64,
    pub jobs_success_24h: i64,
    pub jobs_failed_24h: i64,
    pub credits_purchased_24h: i64,
    pub credits_used_24h: i64,
}

impl DailySnapshot {
    /// Format as a single log line
    pub fn to_log_line(&self) -> String {
        format!(
            "{}-{:03} | signups: {} (free: {}, paid: {}) | jobs: {} (ok: {}, fail: {}) | credits: +{} -{}\n",
            self.year,
            self.day_of_year,
            self.signups_24h,
            self.signups_free_24h,
            self.signups_paid_24h,
            self.jobs_24h,
            self.jobs_success_24h,
            self.jobs_failed_24h,
            self.credits_purchased_24h,
            self.credits_used_24h,
        )
    }
}

/// Query SQLite for the last 24 hours of activity
pub async fn collect_daily_snapshot(pool: &Pool<Sqlite>) -> Result<DailySnapshot> {
    let now = Utc::now();
    let year = now.year();
    let day_of_year = now.ordinal();

    // 24 hours ago in milliseconds (credit_transactions uses ms)
    let cutoff_ms = (now.timestamp() - 86400) * 1000;

    // 24 hours ago as datetime string (job_history uses DATETIME)
    let cutoff_datetime = (now - chrono::Duration::hours(24))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    // Count signups (first transaction per user with specific descriptions)
    let signups_free: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT user_key) FROM credit_transactions
         WHERE description = 'free_initial_credits' AND created_at > ?",
    )
    .bind(cutoff_ms)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    let signups_paid: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT user_key) FROM credit_transactions
         WHERE description = 'Initial Purchase' AND created_at > ?",
    )
    .bind(cutoff_ms)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    // Count jobs from job_history
    let jobs_total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM job_history WHERE timestamp > ?")
        .bind(&cutoff_datetime)
        .fetch_one(pool)
        .await
        .unwrap_or((0,));

    let jobs_success: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM job_history WHERE timestamp > ? AND status = 'success'",
    )
    .bind(&cutoff_datetime)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    let jobs_failed: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM job_history WHERE timestamp > ? AND status != 'success'",
    )
    .bind(&cutoff_datetime)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    // Credits purchased (positive transactions from payment)
    let credits_purchased: (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(amount), 0) FROM credit_transactions
         WHERE amount > 0 AND description IN ('payment', 'Initial Purchase') AND created_at > ?",
    )
    .bind(cutoff_ms)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    // Credits used (negative transactions, excluding refunds)
    let credits_used: (i64,) = sqlx::query_as(
        "SELECT COALESCE(ABS(SUM(amount)), 0) FROM credit_transactions
         WHERE amount < 0 AND description NOT LIKE '%refund%' AND created_at > ?",
    )
    .bind(cutoff_ms)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

    Ok(DailySnapshot {
        year,
        day_of_year,
        signups_24h: signups_free.0 + signups_paid.0,
        signups_free_24h: signups_free.0,
        signups_paid_24h: signups_paid.0,
        api_calls_24h: jobs_total.0, // Using jobs as proxy for API calls
        jobs_24h: jobs_total.0,
        jobs_success_24h: jobs_success.0,
        jobs_failed_24h: jobs_failed.0,
        credits_purchased_24h: credits_purchased.0,
        credits_used_24h: credits_used.0,
    })
}

/// Append a snapshot to the stats log file
pub async fn append_snapshot(snapshot: &DailySnapshot) -> Result<()> {
    let log_path = PathBuf::from(STATS_LOG_FILE);

    // Ensure directory exists
    if let Some(parent) = log_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .await?;

    file.write_all(snapshot.to_log_line().as_bytes()).await?;
    file.flush().await?;

    info!(
        "Stats snapshot written: {}-{:03}",
        snapshot.year, snapshot.day_of_year
    );

    Ok(())
}

/// Background task that runs daily at midnight UTC
pub async fn daily_stats_task(pool: Pool<Sqlite>) {
    loop {
        // Calculate time until next midnight UTC
        let now = Utc::now();
        let tomorrow = (now + chrono::Duration::days(1))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let tomorrow_utc = tomorrow.and_utc();
        let duration_until_midnight = (tomorrow_utc - now)
            .to_std()
            .unwrap_or(std::time::Duration::from_secs(3600));

        info!(
            "Stats task sleeping until midnight UTC ({} seconds)",
            duration_until_midnight.as_secs()
        );

        tokio::time::sleep(duration_until_midnight).await;

        // Collect and write snapshot
        match collect_daily_snapshot(&pool).await {
            Ok(snapshot) => {
                if let Err(e) = append_snapshot(&snapshot).await {
                    error!("Failed to write stats snapshot: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to collect stats snapshot: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_log_line_format() {
        let snapshot = DailySnapshot {
            year: 2025,
            day_of_year: 355,
            signups_24h: 15,
            signups_free_24h: 12,
            signups_paid_24h: 3,
            api_calls_24h: 142,
            jobs_24h: 142,
            jobs_success_24h: 138,
            jobs_failed_24h: 4,
            credits_purchased_24h: 500,
            credits_used_24h: 89,
        };

        let line = snapshot.to_log_line();

        // Verify format
        assert!(line.starts_with("2025-355"));
        assert!(line.contains("signups: 15"));
        assert!(line.contains("free: 12"));
        assert!(line.contains("paid: 3"));
        assert!(line.contains("jobs: 142"));
        assert!(line.contains("ok: 138"));
        assert!(line.contains("fail: 4"));
        assert!(line.contains("+500"));
        assert!(line.contains("-89"));
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn test_day_of_year_formatting() {
        // Test leading zeros for day of year
        let snapshot = DailySnapshot {
            year: 2025,
            day_of_year: 5,
            signups_24h: 0,
            signups_free_24h: 0,
            signups_paid_24h: 0,
            api_calls_24h: 0,
            jobs_24h: 0,
            jobs_success_24h: 0,
            jobs_failed_24h: 0,
            credits_purchased_24h: 0,
            credits_used_24h: 0,
        };

        let line = snapshot.to_log_line();
        assert!(line.starts_with("2025-005")); // Padded to 3 digits
    }
}
