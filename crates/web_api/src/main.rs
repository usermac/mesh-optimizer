use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use shared_db::get_db_pool;
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "web_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Connect to database
    // In a real scenario, ensure the DB file exists or run migrations
    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:../mesh.db".to_string());

    // We might need to create the file if it doesn't exist for this example to run immediately,
    // but shared_db::get_db_pool expects a valid connection string.
    // For now, we assume the DB exists or will be created by the user/scripts.
    let pool = match get_db_pool(&database_url).await {
        Ok(p) => {
            tracing::info!("Connected to database at {}", database_url);
            p
        }
        Err(e) => {
            tracing::warn!(
                "Failed to connect to database: {}. Starting without DB for demo.",
                e
            );
            // This is just to let the server start if DB is missing during initial setup
            // In production, you'd likely want to panic here.
            SqlitePool::connect_lazy("sqlite::memory:").unwrap()
        }
    };

    // Build application with routes
    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .with_state(pool);

    // Run the server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn root() -> Json<Value> {
    Json(json!({ "message": "Welcome to the Mesh Optimizer API" }))
}

async fn health(State(_pool): State<SqlitePool>) -> Json<Value> {
    // In the future, check DB connectivity here
    Json(json!({ "status": "ok" }))
}
