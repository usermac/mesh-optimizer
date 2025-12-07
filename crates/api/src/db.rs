use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct KeyInfo {
    pub email: String,
    #[serde(rename = "stripeCustomerId")]
    pub stripe_customer_id: String,
    pub created: u64, // Milliseconds since epoch
    pub active: bool,
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
}

impl Database {
    pub fn new(file_path: PathBuf) -> Self {
        // Ensure directory exists
        if let Some(parent) = file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let data = if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => DbData::default(),
            }
        } else {
            DbData::default()
        };

        Database {
            file_path,
            data: Arc::new(RwLock::new(data)),
        }
    }

    /// Persist the current state to disk
    async fn persist(&self) -> Result<()> {
        let data = self.data.read().await;
        let json = serde_json::to_string_pretty(&*data)?;
        fs::write(&self.file_path, json)?;
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

    pub async fn create_key(&self, email: String, stripe_customer_id: String) -> Result<String> {
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
}
