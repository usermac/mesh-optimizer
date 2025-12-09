use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct KeyInfo {
    pub email: String,
    #[serde(rename = "stripeCustomerId")]
    pub stripe_customer_id: String,
    pub created: u64, // Milliseconds since epoch
    pub active: bool,
    #[serde(default)]
    pub credits: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transaction {
    pub id: i64,
    pub user_key: String,
    pub amount: i32,
    pub description: String,
    pub reference_job_hash: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CustomerInfo {
    pub email: String,
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DbData {
    pub keys: HashMap<String, KeyInfo>,
    pub customers: HashMap<String, CustomerInfo>,
}

#[derive(Clone)]
pub struct Database {
    file_path: PathBuf,
    data: Arc<RwLock<DbData>>,
    pool: Option<Pool<Sqlite>>,
    salt: String,
}

impl Database {
    pub async fn new(json_path: PathBuf, sqlite_path: PathBuf) -> Self {
        // --- 1. JSON Flat File Setup ---
        // Ensure directory exists
        if let Some(parent) = json_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let data = if json_path.exists() {
            match fs::read_to_string(&json_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(e) => {
                    error!("Failed to read JSON DB: {:?}", e);
                    DbData::default()
                }
            }
        } else {
            DbData::default()
        };

        // --- 2. SQLite Setup ---
        // Ensure directory exists for SQLite db
        if let Some(parent) = sqlite_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        // Create file if not exists (sqlx requires it for file-based URLs sometimes, or handles it via create_if_missing)
        let db_url = format!("sqlite://{}", sqlite_path.to_string_lossy());

        let pool = match SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(
                SqliteConnectOptions::from_str(&db_url)
                    .unwrap_or_else(|_| SqliteConnectOptions::new().filename(&sqlite_path))
                    .create_if_missing(true),
            )
            .await
        {
            Ok(p) => {
                info!("Connected to SQLite at {:?}", sqlite_path);
                Some(p)
            }
            Err(e) => {
                error!("Failed to connect to SQLite: {:?}", e);
                None
            }
        };

        // --- 3. Schema Migration ---
        if let Some(ref p) = pool {
            let schema = r#"
            CREATE TABLE IF NOT EXISTS job_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                user_hash TEXT NOT NULL,
                file_fingerprint TEXT NOT NULL,
                input_format TEXT NOT NULL,
                output_format TEXT NOT NULL,
                input_size_bytes INTEGER NOT NULL,
                output_size_bytes INTEGER NOT NULL,
                processing_time_ms INTEGER NOT NULL,
                quality_ratio REAL NOT NULL,
                status TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS credit_transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_key TEXT NOT NULL,
                amount INTEGER NOT NULL,
                description TEXT NOT NULL,
                reference_job_hash TEXT,
                created_at INTEGER NOT NULL
            );
            "#;
            if let Err(e) = sqlx::query(schema).execute(p).await {
                error!("Failed to run SQLite migration: {:?}", e);
            }
        }

        // --- 4. Load Salt ---
        let salt = env::var("METRICS_SALT").unwrap_or_else(|_| "default-insecure-salt".to_string());
        if salt == "default-insecure-salt" {
            error!("WARNING: METRICS_SALT not set. Using default.");
        }

        Database {
            file_path: json_path,
            data: Arc::new(RwLock::new(data)),
            pool,
            salt,
        }
    }

    /// Persist the current state to disk (JSON)
    async fn persist(&self) -> Result<()> {
        let data = self.data.read().await;
        let json = serde_json::to_string_pretty(&*data)?;
        tokio::fs::write(&self.file_path, json).await?;
        Ok(())
    }

    pub async fn is_valid_key(&self, key: &str) -> bool {
        let data = self.data.read().await;
        if let Some(info) = data.keys.get(key) {
            info.active
        } else {
            false
        }
    }

    pub async fn create_key(
        &self,
        email: String,
        stripe_customer_id: String,
        initial_credits: i32,
    ) -> Result<String> {
        let new_key = format!("sk_{}", Uuid::new_v4().simple());

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        {
            let mut data = self.data.write().await;

            data.keys.insert(
                new_key.clone(),
                KeyInfo {
                    email: email.clone(),
                    stripe_customer_id: stripe_customer_id.clone(),
                    created: now,
                    active: true,
                    credits: initial_credits,
                },
            );

            data.customers.insert(
                stripe_customer_id,
                CustomerInfo {
                    email,
                    key: new_key.clone(),
                },
            );
        } // Drop write lock

        self.persist().await?;

        Ok(new_key)
    }

    pub async fn get_key_by_email(&self, email: &str) -> Option<String> {
        let data = self.data.read().await;
        data.keys
            .iter()
            .find(|(_, info)| info.email == email)
            .map(|(k, _)| k.clone())
    }

    pub async fn add_credits(&self, key: &str, amount: i32) -> Result<i32> {
        // Just a wrapper around the ledger now
        self.record_transaction(key, amount, "admin_add", None)
            .await
    }

    // Updated to use Ledger
    pub async fn record_transaction(
        &self,
        key: &str,
        amount: i32,
        description: &str,
        job_hash: Option<String>,
    ) -> Result<i32> {
        let new_balance;

        // 1. Update In-Memory Balance (Source of Truth for speed)
        {
            let mut data = self.data.write().await;
            if let Some(info) = data.keys.get_mut(key) {
                info.credits += amount;
                new_balance = info.credits;
            } else {
                return Err(anyhow::anyhow!("Key not found"));
            }
        }
        // Persist JSON immediately
        self.persist().await?;

        // 2. Log to SQLite Ledger
        if let Some(pool) = &self.pool {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;

            let query = r#"
            INSERT INTO credit_transactions (user_key, amount, description, reference_job_hash, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#;

            match sqlx::query(query)
                .bind(key)
                .bind(amount)
                .bind(description)
                .bind(job_hash)
                .bind(now)
                .execute(pool)
                .await
            {
                Ok(_) => {
                    info!(
                        "Transaction recorded: key={}, amount={}, description={}",
                        key, amount, description
                    );
                }
                Err(e) => {
                    error!("Failed to record transaction to SQLite: {:?}. key={}, amount={}, description={}", e, key, amount, description);
                }
            }
        }

        Ok(new_balance)
    }

    // Check if we should charge for this file (Fairness Logic)
    pub async fn should_charge_for_file(&self, key: &str, file_hash: &str) -> bool {
        if let Some(pool) = &self.pool {
            // Check for transactions in last 24 hours for this hash
            let one_day_ago = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                - (24 * 60 * 60 * 1000);

            let query = r#"
            SELECT COUNT(*) FROM credit_transactions
            WHERE user_key = ?
            AND reference_job_hash = ?
            AND amount < 0
            AND created_at > ?
            "#;

            let count: (i64,) = sqlx::query_as(query)
                .bind(key)
                .bind(file_hash)
                .bind(one_day_ago)
                .fetch_one(pool)
                .await
                .unwrap_or((0,));

            // If count > 0, they already paid. Don't charge.
            return count.0 == 0;
        }
        // Default to charging if DB is down (safety)
        true
    }

    pub async fn get_credits(&self, key: &str) -> Option<i32> {
        let data = self.data.read().await;
        data.keys.get(key).map(|info| info.credits)
    }

    pub async fn get_history(&self, key: &str, limit: i32) -> Result<Vec<Transaction>> {
        if let Some(pool) = &self.pool {
            let query = r#"
            SELECT id, user_key, amount, description, reference_job_hash, created_at
            FROM credit_transactions
            WHERE user_key = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#;

            match sqlx::query_as(query)
                .bind(key)
                .bind(limit)
                .fetch_all(pool)
                .await
            {
                Ok(history) => {
                    info!("Retrieved {} transactions for key={}", history.len(), key);
                    Ok(history)
                }
                Err(e) => {
                    error!("Failed to fetch history from SQLite: {:?}. key={}", e, key);
                    Err(anyhow::anyhow!("SQLite query failed: {}", e))
                }
            }
        } else {
            error!(
                "SQLite pool is None when trying to get history for key={}",
                key
            );
            Ok(vec![])
        }
    }

    // --- Metrics / Logging ---

    fn hash_string(&self, input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.salt);
        hasher.update(input);
        hex::encode(hasher.finalize())
    }

    pub async fn log_job(
        &self,
        user_identifier: &str, // API Key or Session ID
        filename: &str,
        file_size_bytes: u64,
        input_format: &str,
        output_format: &str,
        output_size_bytes: u64,
        processing_time_ms: u64,
        quality_ratio: f32,
        status: &str,
    ) {
        if let Some(pool) = &self.pool {
            // Pseudonymize User
            let user_hash = self.hash_string(user_identifier);

            // Fingerprint Content (Salt + Filename + Size) ensures same file uploaded twice gets same ID,
            // but is unrelated to the actual filename reversibly.
            let raw_fingerprint = format!("{}{}", filename, file_size_bytes);
            let file_fingerprint = self.hash_string(&raw_fingerprint);

            let query = r#"
            INSERT INTO job_history
            (user_hash, file_fingerprint, input_format, output_format, input_size_bytes, output_size_bytes, processing_time_ms, quality_ratio, status)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#;

            let res = sqlx::query(query)
                .bind(user_hash)
                .bind(file_fingerprint)
                .bind(input_format)
                .bind(output_format)
                .bind(file_size_bytes as i64)
                .bind(output_size_bytes as i64)
                .bind(processing_time_ms as i64)
                .bind(quality_ratio)
                .bind(status)
                .execute(pool)
                .await;

            if let Err(e) = res {
                error!("Failed to log job metrics: {:?}", e);
            }
        }
    }
}
