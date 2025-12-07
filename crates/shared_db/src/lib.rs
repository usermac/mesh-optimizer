use sqlx::sqlite::SqlitePool;
use anyhow::Result;

/// Creates a connection pool to the SQLite database.
pub async fn get_db_pool(database_url: &str) -> Result<SqlitePool> {
    let pool = SqlitePool::connect(database_url).await?;
    Ok(pool)
}

pub mod models {
    use serde::{Deserialize, Serialize};
    use sqlx::FromRow;

    #[derive(Debug, Serialize, Deserialize, FromRow)]
    pub struct User {
        pub id: i64,
        pub email: String,
        pub api_key: Option<String>,
        pub created_at: i64,
    }

    #[derive(Debug, Serialize, Deserialize, FromRow)]
    pub struct Transaction {
        pub id: i64,
        pub user_id: i64,
        pub amount_cents: i64,
        pub description: String,
        pub created_at: i64,
    }
}
