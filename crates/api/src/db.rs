use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::RngCore;
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
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
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

/// Job status for persistence
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Processing,
    Completed {
        output_size: u64,
        glb_url: String,
        usdz_url: String,
        expires_at: String, // ISO 8601 timestamp
    },
    Failed {
        error: String,
    },
}

/// Stored job record
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct StoredJob {
    pub batch_id: String,
    pub status: JobStatus,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct Database {
    file_path: PathBuf,
    data: Arc<RwLock<DbData>>,
    pool: Option<Pool<Sqlite>>,
    salt: String,
    encryption_key: Option<[u8; 32]>,
}

/// Encrypt data using AES-256-GCM
fn encrypt_data(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    // Generate random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // Prepend nonce to ciphertext (nonce is not secret, just needs to be unique)
    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

/// Decrypt data using AES-256-GCM
fn decrypt_data(key: &[u8; 32], encrypted: &[u8]) -> Result<Vec<u8>> {
    if encrypted.len() < 12 {
        return Err(anyhow::anyhow!("Encrypted data too short"));
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let nonce = Nonce::from_slice(&encrypted[..12]);
    let ciphertext = &encrypted[12..];

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))
}

/// Parse encryption key from environment variable (hex or base64 encoded, must be 32 bytes)
fn parse_encryption_key(key_str: &str) -> Result<[u8; 32]> {
    // Try hex first (64 characters = 32 bytes)
    if key_str.len() == 64 {
        if let Ok(bytes) = hex::decode(key_str) {
            if bytes.len() == 32 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&bytes);
                return Ok(key);
            }
        }
    }

    // Try base64 (44 characters with padding = 32 bytes)
    if let Ok(bytes) = BASE64.decode(key_str) {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }

    Err(anyhow::anyhow!(
        "ENCRYPTION_KEY must be 32 bytes, provided as 64 hex chars or 44 base64 chars"
    ))
}

impl Database {
    pub async fn new(json_path: PathBuf, sqlite_path: PathBuf) -> Self {
        // --- 0. Load Encryption Key ---
        let encryption_key = match env::var("ENCRYPTION_KEY") {
            Ok(key_str) => match parse_encryption_key(&key_str) {
                Ok(key) => {
                    info!("Database encryption enabled");
                    Some(key)
                }
                Err(e) => {
                    error!(
                        "Invalid ENCRYPTION_KEY: {}. Database will NOT be encrypted!",
                        e
                    );
                    None
                }
            },
            Err(_) => {
                warn!("ENCRYPTION_KEY not set. Database file will be stored in PLAINTEXT. Set ENCRYPTION_KEY for production use.");
                None
            }
        };

        // --- 1. JSON Flat File Setup ---
        // Ensure directory exists
        if let Some(parent) = json_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let data = if json_path.exists() {
            Self::load_data_from_file(&json_path, encryption_key.as_ref())
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
                status TEXT NOT NULL,
                source TEXT DEFAULT 'api' NOT NULL
            );

            CREATE TABLE IF NOT EXISTS credit_transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_key TEXT NOT NULL,
                amount INTEGER NOT NULL,
                description TEXT NOT NULL,
                reference_job_hash TEXT,
                created_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS jobs (
                batch_id TEXT PRIMARY KEY,
                status_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            "#;
            if let Err(e) = sqlx::query(schema).execute(p).await {
                error!("Failed to run SQLite migration: {:?}", e);
            }

            // Migration: Add source column if missing (ignores error if exists)
            let _ = sqlx::query(
                "ALTER TABLE job_history ADD COLUMN source TEXT DEFAULT 'api' NOT NULL",
            )
            .execute(p)
            .await;
        }

        // --- 4. Load Salt ---
        let salt = env::var("METRICS_SALT").expect("METRICS_SALT must be set");

        Database {
            file_path: json_path,
            data: Arc::new(RwLock::new(data)),
            pool,
            salt,
            encryption_key,
        }
    }

    /// Load database from file, handling both encrypted and plaintext formats
    fn load_data_from_file(path: &PathBuf, encryption_key: Option<&[u8; 32]>) -> DbData {
        match fs::read(path) {
            Ok(content) => {
                // Try to parse as JSON first (plaintext or legacy format)
                if let Ok(text) = std::str::from_utf8(&content) {
                    if let Ok(data) = serde_json::from_str::<DbData>(text) {
                        if encryption_key.is_some() {
                            info!("Loaded plaintext database - will be encrypted on next save");
                        }
                        return data;
                    }
                }

                // Try to decrypt if we have a key
                if let Some(key) = encryption_key {
                    // Check for base64 prefix marker "ENC:"
                    if content.starts_with(b"ENC:") {
                        let encoded = &content[4..];
                        if let Ok(encrypted) = BASE64.decode(encoded) {
                            match decrypt_data(key, &encrypted) {
                                Ok(decrypted) => {
                                    if let Ok(text) = std::str::from_utf8(&decrypted) {
                                        if let Ok(data) = serde_json::from_str::<DbData>(text) {
                                            info!("Loaded and decrypted database successfully");
                                            return data;
                                        }
                                    }
                                    error!("Decrypted data is not valid JSON");
                                }
                                Err(e) => {
                                    error!("Failed to decrypt database: {}. Wrong key?", e);
                                }
                            }
                        }
                    } else {
                        error!("Database file is not plaintext JSON and not encrypted format");
                    }
                } else {
                    error!("Database appears encrypted but no ENCRYPTION_KEY provided");
                }

                DbData::default()
            }
            Err(e) => {
                error!("Failed to read database file: {:?}", e);
                DbData::default()
            }
        }
    }

    /// Persist the current state to disk (encrypted if key is set)
    async fn persist(&self) -> Result<()> {
        let data = self.data.read().await;
        let json = serde_json::to_string_pretty(&*data)?;

        let file_content = if let Some(ref key) = self.encryption_key {
            let encrypted = encrypt_data(key, json.as_bytes())?;
            let encoded = BASE64.encode(&encrypted);
            format!("ENC:{}", encoded).into_bytes()
        } else {
            json.into_bytes()
        };

        tokio::fs::write(&self.file_path, file_content).await?;
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

    pub async fn get_key_info(&self, key: &str) -> Option<KeyInfo> {
        let data = self.data.read().await;
        data.keys.get(key).cloned()
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
                    credits: 0,
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

        self.record_transaction(&new_key, initial_credits, "Initial Purchase", None)
            .await?;

        Ok(new_key)
    }

    pub async fn get_key_by_email(&self, email: &str) -> Option<String> {
        let data = self.data.read().await;
        data.keys
            .iter()
            .find(|(_, info)| info.email == email)
            .map(|(k, _)| k.clone())
    }

    pub async fn get_key_by_customer_id(&self, customer_id: &str) -> Option<String> {
        let data = self.data.read().await;
        data.customers.get(customer_id).map(|info| info.key.clone())
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
    pub async fn should_charge_for_file(
        &self,
        key: &str,
        file_hash: &str,
        free_reoptimization_hours: u32,
    ) -> bool {
        if let Some(pool) = &self.pool {
            // Check for transactions within the free re-optimization window
            let cutoff = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                - (free_reoptimization_hours as i64 * 60 * 60 * 1000);

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
                .bind(cutoff)
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

    // --- Job Persistence ---

    /// Save or update a job status
    pub async fn save_job(&self, batch_id: &str, status: &JobStatus) -> Result<()> {
        if let Some(pool) = &self.pool {
            let now = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            let status_json = serde_json::to_string(status)?;

            let query = r#"
            INSERT INTO jobs (batch_id, status_json, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(batch_id) DO UPDATE SET
                status_json = excluded.status_json,
                updated_at = excluded.updated_at
            "#;

            sqlx::query(query)
                .bind(batch_id)
                .bind(&status_json)
                .bind(now)
                .bind(now)
                .execute(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to save job: {}", e))?;

            info!("Job {} saved with status: {:?}", batch_id, status);
        }
        Ok(())
    }

    /// Load a single job by batch_id
    pub async fn get_job(&self, batch_id: &str) -> Option<JobStatus> {
        if let Some(pool) = &self.pool {
            let query = "SELECT status_json FROM jobs WHERE batch_id = ?";

            if let Ok(row) = sqlx::query_as::<_, (String,)>(query)
                .bind(batch_id)
                .fetch_one(pool)
                .await
            {
                if let Ok(status) = serde_json::from_str::<JobStatus>(&row.0) {
                    return Some(status);
                }
            }
        }
        None
    }

    /// Load all non-expired jobs (for startup recovery)
    pub async fn load_active_jobs(&self, max_age_secs: i64) -> Vec<StoredJob> {
        let mut jobs = Vec::new();

        if let Some(pool) = &self.pool {
            let cutoff = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                - max_age_secs;

            let query = r#"
            SELECT batch_id, status_json, created_at
            FROM jobs
            WHERE created_at > ?
            ORDER BY created_at DESC
            "#;

            if let Ok(rows) = sqlx::query_as::<_, (String, String, i64)>(query)
                .bind(cutoff)
                .fetch_all(pool)
                .await
            {
                for (batch_id, status_json, created_at) in rows {
                    if let Ok(status) = serde_json::from_str::<JobStatus>(&status_json) {
                        jobs.push(StoredJob {
                            batch_id,
                            status,
                            created_at,
                        });
                    }
                }
            }
        }

        info!("Loaded {} active jobs from database", jobs.len());
        jobs
    }

    /// Delete old jobs from the database
    pub async fn cleanup_old_jobs(&self, max_age_secs: i64) -> Result<u64> {
        if let Some(pool) = &self.pool {
            let cutoff = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                - max_age_secs;

            let query = "DELETE FROM jobs WHERE created_at < ?";

            let result = sqlx::query(query)
                .bind(cutoff)
                .execute(pool)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to cleanup jobs: {}", e))?;

            let deleted = result.rows_affected();
            if deleted > 0 {
                info!("Cleaned up {} old job records", deleted);
            }
            return Ok(deleted);
        }
        Ok(0)
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
        source: &str,
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
            (user_hash, file_fingerprint, input_format, output_format, input_size_bytes, output_size_bytes, processing_time_ms, quality_ratio, status, source)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                .bind(source)
                .execute(pool)
                .await;

            if let Err(e) = res {
                error!("Failed to log job metrics: {:?}", e);
            }
        }
    }
}
