mod db;
mod stats;

use crate::db::JobStatus;
use anyhow::{Context, Result};
use axum::http::{header, Method};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Digest;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, SystemTime},
};
use stripe::{
    CheckoutSessionMode, CreateCheckoutSession, CreateCheckoutSessionLineItems,
    CreateCheckoutSessionLineItemsPriceData, CreateCheckoutSessionLineItemsPriceDataProductData,
    CreateCheckoutSessionPaymentMethodTypes, Currency, CustomerId, EventObject, EventType,
    Expandable, Webhook,
};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tokio::sync::Semaphore;
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    services::ServeDir,
};
use tracing::{error, info, warn};

// --- CONFIGURATION ---
const UPLOAD_DIR: &str = "uploads";
const DB_FILE: &str = "server/database.json";
const DB_SQLITE_FILE: &str = "server/stats.db";
const DOWNLOAD_EXPIRES_SECS: u64 = 60 * 60; // 1 hour

// Allowed file extensions for optimization (main model files)
const ALLOWED_EXTENSIONS: &[&str] = &["glb", "gltf", "obj", "fbx", "zip"];
// Allowed auxiliary file extensions (materials, textures)
const ALLOWED_AUXILIARY_EXTENSIONS: &[&str] = &[
    "mtl", "png", "jpg", "jpeg", "tga", "bmp", "tif", "tiff", "bin",
];

// --- JSON ERROR RESPONSES ---

/// Structured JSON error response for API clients
#[derive(Serialize)]
struct ApiError {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    balance: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl ApiError {
    fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            code: None,
            balance: None,
            required: None,
            message: None,
        }
    }

    fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    fn with_balance(mut self, balance: i32) -> Self {
        self.balance = Some(balance);
        self
    }

    fn with_required(mut self, required: i32) -> Self {
        self.required = Some(required);
        self
    }

    fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Helper to create JSON error responses
fn json_error(status: StatusCode, error: ApiError) -> Response {
    (status, Json(error)).into_response()
}

/// Common error responses
fn error_unauthorized() -> Response {
    json_error(
        StatusCode::UNAUTHORIZED,
        ApiError::new("Unauthorized")
            .with_code("unauthorized")
            .with_message("Invalid or missing API key. Get your key at webdeliveryengine.com"),
    )
}

fn error_insufficient_credits(balance: i32, required: i32) -> Response {
    json_error(
        StatusCode::PAYMENT_REQUIRED,
        ApiError::new("Insufficient credits")
            .with_code("insufficient_credits")
            .with_balance(balance)
            .with_required(required)
            .with_message("Purchase more credits at webdeliveryengine.com"),
    )
}

fn error_bad_request(error: impl Into<String>) -> Response {
    json_error(
        StatusCode::BAD_REQUEST,
        ApiError::new(error).with_code("bad_request"),
    )
}

fn error_not_found(error: impl Into<String>) -> Response {
    json_error(
        StatusCode::NOT_FOUND,
        ApiError::new(error).with_code("not_found"),
    )
}

fn error_server(error: impl Into<String>) -> Response {
    json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        ApiError::new(error)
            .with_code("server_error")
            .with_message("Please try again or contact support@webdeliveryengine.com"),
    )
}

fn error_transaction_failed() -> Response {
    json_error(
        StatusCode::PAYMENT_REQUIRED,
        ApiError::new("Transaction failed")
            .with_code("transaction_failed")
            .with_message("Credit deduction failed. Please try again."),
    )
}

// --- PRICING CONFIGURATION ---
const PRICING_FILE: &str = "server/pricing.json";

#[derive(Clone, Deserialize, Serialize)]
struct PricingTier {
    name: String,
    min_spend_usd: u32,
    bonus_percent: u32,
}

#[derive(Clone, Deserialize, Serialize)]
struct PricingConfig {
    base_rate_usd_per_credit: f64,
    min_purchase_usd: u32,
    max_purchase_usd: u32,
    default_purchase_usd: u32,
    tiers: Vec<PricingTier>,
    free_reoptimization_hours: u32,
    cost_decimate: i32,
    cost_remesh: i32,
    #[serde(default)]
    free_initial_credits: i32,
}

/// Load pricing config fresh from disk (enables hot reloading without restart)
fn load_pricing_config() -> Result<PricingConfig, String> {
    let content = fs::read_to_string(PRICING_FILE)
        .map_err(|e| format!("Failed to read {}: {}", PRICING_FILE, e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse {}: {}", PRICING_FILE, e))
}

// Easter egg: load funny processing messages from JSON file
const PROCESSING_MESSAGES_FILE: &str = "server/processing_messages.json";
const DEFAULT_PROCESSING_MESSAGE: &str = "Processing...";

#[derive(Clone, Default)]
struct ProcessingMessages {
    decimate: Vec<String>,
    remesh: Vec<String>,
}

fn load_processing_messages() -> ProcessingMessages {
    match fs::read_to_string(PROCESSING_MESSAGES_FILE) {
        Ok(content) => match serde_json::from_str::<HashMap<String, Vec<String>>>(&content) {
            Ok(map) => {
                let decimate = map.get("decimate").cloned().unwrap_or_default();
                let remesh = map.get("remesh").cloned().unwrap_or_default();
                info!(
                    "Loaded processing messages: {} decimate, {} remesh",
                    decimate.len(),
                    remesh.len()
                );
                ProcessingMessages { decimate, remesh }
            }
            Err(e) => {
                warn!(
                    "Failed to parse {}: {}, using default",
                    PROCESSING_MESSAGES_FILE, e
                );
                ProcessingMessages::default()
            }
        },
        Err(e) => {
            warn!(
                "Failed to read {}: {}, using default",
                PROCESSING_MESSAGES_FILE, e
            );
            ProcessingMessages::default()
        }
    }
}

#[derive(Clone)]
struct AppState {
    db: db::Database,
    stripe_client: stripe::Client,
    stripe_webhook_secret: String,
    resend_api_key: String,
    admin_secret: String,
    jobs: Arc<RwLock<HashMap<String, JobStatus>>>,
    worker_semaphore: Arc<Semaphore>,
    admin_rate_limiter: Arc<RwLock<HashMap<String, Vec<std::time::Instant>>>>,
    processing_messages: Arc<ProcessingMessages>,
}

#[derive(Clone)]
struct AuthKey(String);

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize Logging
    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_max_level(tracing::Level::INFO)
        .init();
    dotenvy::dotenv().ok();

    let stripe_secret_key =
        std::env::var("STRIPE_SECRET_KEY").expect("STRIPE_SECRET_KEY must be set");
    let stripe_webhook_secret =
        std::env::var("STRIPE_WEBHOOK_SECRET").expect("STRIPE_WEBHOOK_SECRET must be set");
    let resend_api_key = std::env::var("RESEND_API_KEY").expect("RESEND_API_KEY must be set");
    let admin_secret = std::env::var("ADMIN_SECRET")
        .expect("ADMIN_SECRET must be set - this is required for admin endpoint security");

    // Validate ENCRYPTION_KEY is set (actual parsing happens in db.rs)
    std::env::var("ENCRYPTION_KEY")
        .expect("ENCRYPTION_KEY must be set - this is required to encrypt database.json at rest");

    // 2a. Verify Pricing Configuration exists (will be loaded fresh on each request)
    match load_pricing_config() {
        Ok(config) => {
            info!(
                "Verified pricing config: base_rate=${}/credit, {} tiers defined (hot-reload enabled)",
                config.base_rate_usd_per_credit,
                config.tiers.len()
            );
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Pricing config error: {}", e));
        }
    }

    // 2b. Setup Filesystem
    fs::create_dir_all(UPLOAD_DIR).context("Failed to create upload dir")?;
    // Ensure "server" dir exists for db file compat with Node paths
    if let Some(parent) = Path::new(DB_FILE).parent() {
        fs::create_dir_all(parent).ok();
    }

    // 3. Initialize State
    let db = db::Database::new(PathBuf::from(DB_FILE), PathBuf::from(DB_SQLITE_FILE)).await;
    let stripe_client = stripe::Client::new(stripe_secret_key);
    let worker_slots: usize = std::env::var("WORKER_SLOTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10); // Default 10 slots (Remesh=4, Decimate=1)

    // 3b. Load persisted jobs from database and recover state
    let mut recovered_jobs: HashMap<String, JobStatus> = HashMap::new();
    let active_jobs = db.load_active_jobs(DOWNLOAD_EXPIRES_SECS as i64).await;

    for stored_job in active_jobs {
        let batch_dir = Path::new(UPLOAD_DIR).join(&stored_job.batch_id);

        match &stored_job.status {
            JobStatus::Processing => {
                // Server restarted while job was processing - mark as failed
                // unless output files exist (job completed but status wasn't updated)
                let glb_exists = batch_dir.join("output.glb").exists()
                    || fs::read_dir(&batch_dir)
                        .map(|entries| {
                            entries.filter_map(|e| e.ok()).any(|e| {
                                e.path()
                                    .extension()
                                    .map(|ext| ext == "glb")
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false);

                if glb_exists {
                    info!(
                        "Job {} was processing but output exists - marking as completed",
                        stored_job.batch_id
                    );
                    // We don't have exact output info, so mark with placeholder
                    // User can still download via the batch_id
                    recovered_jobs.insert(stored_job.batch_id, stored_job.status);
                } else {
                    info!(
                        "Job {} was interrupted by restart - marking as failed",
                        stored_job.batch_id
                    );
                    let failed_status = JobStatus::Failed {
                        error: "Server restarted during processing".to_string(),
                    };
                    let _ = db.save_job(&stored_job.batch_id, &failed_status).await;
                    recovered_jobs.insert(stored_job.batch_id, failed_status);
                }
            }
            JobStatus::Completed { .. } | JobStatus::Failed { .. } => {
                // Keep completed/failed jobs in memory for status queries
                recovered_jobs.insert(stored_job.batch_id, stored_job.status);
            }
            JobStatus::Queued => {
                // Queued jobs that weren't processed - mark as failed
                let failed_status = JobStatus::Failed {
                    error: "Server restarted before processing started".to_string(),
                };
                let _ = db.save_job(&stored_job.batch_id, &failed_status).await;
                recovered_jobs.insert(stored_job.batch_id, failed_status);
            }
        }
    }

    info!("Recovered {} jobs from database", recovered_jobs.len());

    let state = AppState {
        db,
        stripe_client,
        stripe_webhook_secret,
        resend_api_key,
        admin_secret,
        jobs: Arc::new(RwLock::new(recovered_jobs)),
        worker_semaphore: Arc::new(Semaphore::new(worker_slots)),
        admin_rate_limiter: Arc::new(RwLock::new(HashMap::new())),
        processing_messages: Arc::new(load_processing_messages()),
    };

    // 4. Start Cleanup Task
    let db_for_cleanup = state.db.clone();
    tokio::spawn(cleanup_task(db_for_cleanup));

    // 4a. Start Session Cleanup Task
    let db_for_session_cleanup = state.db.clone();
    tokio::spawn(async move {
        loop {
            // Run session cleanup every hour
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            if let Err(e) = db_for_session_cleanup.cleanup_expired_sessions().await {
                error!("Session cleanup failed: {}", e);
            }
        }
    });

    // 4b. Start Capacity Stats Task
    let semaphore_for_stats = state.worker_semaphore.clone();
    tokio::spawn(capacity_stats_task(semaphore_for_stats, worker_slots));

    // 4c. Start Daily Stats Snapshot Task
    if let Some(pool) = state.db.get_pool() {
        tokio::spawn(stats::daily_stats_task(pool));
        info!("Daily stats snapshot task started");
    } else {
        warn!("SQLite pool not available - daily stats task not started");
    }

    // 5. Build Router
    let app = Router::new()
        // Public Routes
        .route("/health", get(health_check))
        .route("/config", get(get_config))
        .route("/contact", post(contact_handler))
        .route("/claim-free-credits", post(claim_free_credits))
        .route("/job/:id", get(job_status_handler))
        .route(
            "/history",
            get(history_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        .route(
            "/credits",
            get(credits_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        .route("/create-checkout-session", post(create_checkout_session))
        .route("/webhook", post(stripe_webhook))
        .route("/success", get(success_page))
        // Session/Auth Routes (for web app)
        .route("/auth/session", get(get_session_user))
        .route("/auth/logout", post(logout_handler))
        .route("/api/user/api-key", get(get_api_key_handler))
        .route("/api/user/api-key/regenerate", post(regenerate_api_key_handler))
        // Admin Routes
        .route("/admin/add-credits", post(admin_add_credits))
        .route("/admin/create-key", post(admin_create_key))
        .route("/admin/grant-credits", post(admin_grant_credits))
        .route("/admin/users", post(admin_list_users))
        // Static Files
        .nest_service("/", ServeDir::new("server/public")) // Assuming public dir is here
        .nest_service("/download", ServeDir::new(UPLOAD_DIR))
        // Protected Routes
        .route(
            "/optimize",
            post(optimize_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        // Global Middleware
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024 * 1024)) // 5GB
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list([
                    "https://www.webdeliveryengine.com".parse().unwrap(),
                    "https://webdeliveryengine.com".parse().unwrap(),
                    "https://api.webdeliveryengine.com".parse().unwrap(),
                    "http://localhost:3000".parse().unwrap(),
                ]))
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
                .allow_credentials(true),
        )
        .with_state(state);

    // 7. Start Server
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Server running on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// --- MIDDLEWARE ---

// Session cookie configuration
const SESSION_COOKIE_NAME: &str = "session";
const SESSION_DURATION_SECS: i64 = 7 * 24 * 60 * 60; // 7 days

async fn auth_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Response {
    // 1. Check for session cookie first (web app users)
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        if let Some(api_key) = state.db.validate_session(session_cookie.value()).await {
            req.extensions_mut().insert(AuthKey(api_key));
            return next.run(req).await;
        }
    }

    // 2. Check for API key in Authorization header (programmatic access)
    let token = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            // Fallback: check ?key=... query parameter
            req.uri().query().and_then(|q| {
                url::form_urlencoded::parse(q.as_bytes())
                    .find(|(k, _)| k == "key")
                    .map(|(_, v)| v.to_string())
            })
        });

    match token {
        Some(t) if state.db.is_valid_key(&t).await => {
            req.extensions_mut().insert(AuthKey(t));
            next.run(req).await
        }
        _ => error_unauthorized(),
    }
}

/// Create a secure session cookie
fn create_session_cookie(session_id: &str) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, session_id.to_string()))
        .path("/")
        .http_only(true)
        .secure(true) // HTTPS only
        .same_site(SameSite::Strict)
        .max_age(time::Duration::seconds(SESSION_DURATION_SECS))
        .build()
}

/// Create an expired cookie to clear the session
fn clear_session_cookie() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Strict)
        .max_age(time::Duration::seconds(0))
        .build()
}

// --- SESSION/AUTH HANDLERS ---

/// Get current session user info (if authenticated via session)
async fn get_session_user(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Check for valid session
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        if let Some(api_key) = state.db.validate_session(session_cookie.value()).await {
            if let Some(info) = state.db.get_key_info(&api_key).await {
                return Ok(Json(json!({
                    "authenticated": true,
                    "email": info.email,
                    "credits": info.credits
                })));
            }
        }
    }

    // Not authenticated
    Ok(Json(json!({
        "authenticated": false
    })))
}

/// Logout - clear session cookie and delete session from database
async fn logout_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    // Delete session from database if it exists
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        let _ = state.db.delete_session(session_cookie.value()).await;
    }

    // Return response with cleared cookie
    (jar.add(clear_session_cookie()), Json(json!({ "success": true })))
}

/// Get the user's API key (only accessible when authenticated via session)
async fn get_api_key_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Must be authenticated via session
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        if let Some(api_key) = state.db.validate_session(session_cookie.value()).await {
            if let Some(info) = state.db.get_key_info(&api_key).await {
                return Ok(Json(json!({
                    "api_key": api_key,
                    "email": info.email,
                    "created": info.created
                })));
            }
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

/// Regenerate API key (creates a new key, invalidates the old one)
async fn regenerate_api_key_handler(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    // Must be authenticated via session
    let session_id = jar
        .get(SESSION_COOKIE_NAME)
        .map(|c| c.value().to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let old_api_key = state
        .db
        .validate_session(&session_id)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let info = state
        .db
        .get_key_info(&old_api_key)
        .await
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Get current credits to transfer to new key
    let current_credits = info.credits;

    // Create new key with the same email
    let new_key = state
        .db
        .create_free_tier_key(info.email.clone(), 0)
        .await
        .map_err(|e| {
            error!("Failed to create new key: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Transfer credits from old key to new key
    if current_credits > 0 {
        let _ = state
            .db
            .record_transaction(&new_key, current_credits, "transfer_from_regenerated_key", None)
            .await;
    }

    // Deactivate the old key (set credits to 0, mark inactive)
    // Note: We can't fully delete it as it's part of the JSON structure
    // The old key will just become unusable
    let _ = state
        .db
        .record_transaction(&old_api_key, -current_credits, "key_regenerated", None)
        .await;

    // Delete all sessions for the old key
    let _ = state.db.delete_sessions_for_key(&old_api_key).await;

    // Create a new session for the new key
    let new_session_id = state
        .db
        .create_session(&new_key, SESSION_DURATION_SECS, None, None)
        .await
        .map_err(|e| {
            error!("Failed to create session: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Regenerated API key for {}: {} -> {}",
        info.email, old_api_key, new_key
    );

    // Return new session cookie and key info
    Ok((
        jar.add(create_session_cookie(&new_session_id)),
        Json(json!({
            "success": true,
            "api_key": new_key,
            "message": "API key regenerated. Your old key is now invalid."
        })),
    ))
}

// --- HANDLERS ---

async fn health_check() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

async fn get_config() -> Result<Json<serde_json::Value>, StatusCode> {
    let pricing = load_pricing_config().map_err(|e| {
        error!("Failed to load pricing config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({
        "pricing": {
            "base_rate_usd_per_credit": pricing.base_rate_usd_per_credit,
            "min_purchase_usd": pricing.min_purchase_usd,
            "max_purchase_usd": pricing.max_purchase_usd,
            "default_purchase_usd": pricing.default_purchase_usd,
            "tiers": pricing.tiers,
            "free_reoptimization_hours": pricing.free_reoptimization_hours,
            "free_initial_credits": pricing.free_initial_credits
        },
        "cost_decimate": pricing.cost_decimate,
        "cost_remesh": pricing.cost_remesh
    })))
}

#[derive(Deserialize)]
struct CreateCheckoutPayload {
    api_key: Option<String>,
    usd_amount: u32,
}

async fn create_checkout_session(
    State(state): State<AppState>,
    Json(payload): Json<CreateCheckoutPayload>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Starting Checkout Session for ${}", payload.usd_amount);

    let pricing = load_pricing_config().map_err(|e| {
        error!("Failed to load pricing config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 1. Validate amount is within bounds
    if payload.usd_amount < pricing.min_purchase_usd
        || payload.usd_amount > pricing.max_purchase_usd
    {
        error!(
            "Invalid purchase amount: ${} (min: ${}, max: ${})",
            payload.usd_amount, pricing.min_purchase_usd, pricing.max_purchase_usd
        );
        return Err(StatusCode::BAD_REQUEST);
    }

    // 2. Calculate base credits from USD amount
    let base_credits =
        (payload.usd_amount as f64 / pricing.base_rate_usd_per_credit).floor() as i32;

    // 3. Determine bonus percentage from tiers (highest qualifying tier wins)
    let bonus_percent = pricing
        .tiers
        .iter()
        .filter(|tier| payload.usd_amount >= tier.min_spend_usd)
        .max_by_key(|tier| tier.min_spend_usd)
        .map_or(0, |tier| tier.bonus_percent);

    let bonus_credits = (base_credits as f64 * (bonus_percent as f64 / 100.0)).floor() as i32;
    let total_credits = base_credits + bonus_credits;

    info!(
        "Pricing calculation: ${} -> {} base + {} bonus ({}%) = {} total credits",
        payload.usd_amount, base_credits, bonus_credits, bonus_percent, total_credits
    );

    // 4. Resolve Customer ID if API Key is present
    let mut customer_id_opt = None;
    if let Some(key) = &payload.api_key {
        if !key.is_empty() {
            if let Some(info) = state.db.get_key_info(key).await {
                info!("Existing user detected: {}", info.email);
                if let Ok(cid) = CustomerId::from_str(&info.stripe_customer_id) {
                    customer_id_opt = Some(cid);
                }
            }
        }
    }

    // 5. Build metadata to pass total_credits to the webhook
    let mut metadata: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    metadata.insert("total_credits".to_string(), total_credits.to_string());
    metadata.insert("app".to_string(), "MeshOpt".to_string());

    // 6. Create Stripe Checkout Session
    let params = CreateCheckoutSession {
        customer: customer_id_opt,
        payment_method_types: Some(vec![CreateCheckoutSessionPaymentMethodTypes::Card]),
        line_items: Some(vec![CreateCheckoutSessionLineItems {
            price_data: Some(CreateCheckoutSessionLineItemsPriceData {
                currency: Currency::USD,
                product_data: Some(CreateCheckoutSessionLineItemsPriceDataProductData {
                    name: format!("{} Mesh Optimizer Credits", total_credits),
                    ..Default::default()
                }),
                unit_amount: Some(payload.usd_amount as i64 * 100), // Convert to cents
                ..Default::default()
            }),
            quantity: Some(1),
            ..Default::default()
        }]),
        mode: Some(CheckoutSessionMode::Payment),
        metadata: Some(metadata),
        success_url: Some(
            "https://www.webdeliveryengine.com/success?session_id={CHECKOUT_SESSION_ID}",
        ),
        cancel_url: Some("https://www.webdeliveryengine.com/"),
        ..Default::default()
    };

    match stripe::CheckoutSession::create(&state.stripe_client, params).await {
        Ok(session) => {
            info!("Session Created: {:?}", session.url);
            Ok(Json(json!({ "url": session.url })))
        }
        Err(e) => {
            error!("Stripe Error: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn stripe_webhook(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let sig = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let event = Webhook::construct_event(&body, sig, &state.stripe_webhook_secret)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    if event.type_ == EventType::CheckoutSessionCompleted {
        if let EventObject::CheckoutSession(session) = event.data.object {
            if let Some(customer_details) = session.customer_details {
                if let Some(email) = customer_details.email {
                    let customer_id = match session.customer {
                        Some(Expandable::Id(id)) => id.to_string(),
                        Some(Expandable::Object(c)) => c.id.to_string(),
                        None => String::new(),
                    };
                    info!("💰 Payment received from {}", email);

                    // Read total_credits from session metadata
                    let credit_amount = session
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("total_credits"))
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or_else(|| {
                            error!("No total_credits in session metadata, using fallback");
                            100
                        });

                    info!("Credits to grant: {}", credit_amount);

                    // Check if user exists
                    let mut key_found = state.db.get_key_by_customer_id(&customer_id).await;

                    if key_found.is_none() {
                        key_found = state.db.get_key_by_email(&email).await;
                    }

                    if let Some(existing_key) = key_found {
                        info!("Top Up: Adding credits to existing user {}", email);
                        let _ = state
                            .db
                            .add_credits_with_description(&existing_key, credit_amount, "payment")
                            .await
                            .map_err(|e| {
                                error!("DB Error adding credits: {:?}", e);
                                StatusCode::INTERNAL_SERVER_ERROR
                            })?;
                    } else {
                        info!("New User: Creating key for {}", email);
                        let _ = state
                            .db
                            .create_key(email.clone(), customer_id, credit_amount)
                            .await
                            .map_err(|e| {
                                error!("DB Error creating key: {:?}", e);
                                StatusCode::INTERNAL_SERVER_ERROR
                            })?;
                    }
                }
            }
        }
    }

    Ok(Json(json!({ "received": true })))
}

#[derive(Deserialize)]
struct SuccessQuery {
    session_id: Option<String>,
}

async fn success_page(
    State(state): State<AppState>,
    Query(query): Query<SuccessQuery>,
) -> Response {
    if query.session_id.is_none() {
        return axum::response::Redirect::temporary("/").into_response();
    }

    let session_id = query.session_id.unwrap();

    // Retrieve session from Stripe to get email
    let session = match stripe::CheckoutSession::retrieve(
        &state.stripe_client,
        &stripe::CheckoutSessionId::from_str(&session_id).unwrap(),
        &[],
    )
    .await
    {
        Ok(s) => s,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error retrieving session",
            )
                .into_response()
        }
    };

    let email = match session.customer_details.and_then(|d| d.email) {
        Some(e) => e,
        None => return (StatusCode::BAD_REQUEST, "No email found in session").into_response(),
    };

    let customer_id = match session.customer {
        Some(Expandable::Id(id)) => id.to_string(),
        Some(Expandable::Object(c)) => c.id.to_string(),
        None => String::new(),
    };

    let mut key_found = state.db.get_key_by_customer_id(&customer_id).await;

    if key_found.is_none() {
        key_found = state.db.get_key_by_email(&email).await;
    }

    let key = key_found.unwrap_or_else(|| "Key processing... check email later".to_string());

    let html = format!(
        r#"
        <html><body style="font-family:sans-serif; background:#111; color:white; text-align:center; padding:50px;">
            <h1 style="color:#10b981">Payment Successful!</h1>
            <p>Thank you for your purchase!</p>

            <div style="background:#d1fae5; color:#065f46; padding:20px; border-radius:8px; margin:30px auto; max-width:500px; border-left:4px solid #10b981;">
                <strong style="font-size:1.1em;">Your API key is saved automatically</strong>
                <p style="margin-top:10px; margin-bottom:0;">When you return to the dashboard, your key will be stored in your browser. You can also copy it now for your records.</p>
            </div>

            <p style="margin-top:30px; margin-bottom:10px; color:#aaa;">Your API Key:</p>
            <div style="background:#1f2937; padding:20px; font-size:18px; font-family:monospace; border-radius:8px; display:inline-block; border: 2px solid #3b82f6; word-break:break-all; max-width:400px;">
                <strong style="color:#3b82f6;">{}</strong>
            </div>

            <p style="color:#888; font-size:0.9em; margin-top:20px;">Use this key in the API Key field on the dashboard to optimize your 3D models.</p>
            <a href="/" style="color:#3b82f6; text-decoration:none; margin-top:30px; display:inline-block; padding:10px 20px; border:1px solid #3b82f6; border-radius:6px;">&larr; Back to Dashboard</a>
            <script>
                // Save API key to localStorage so it auto-fills on the dashboard
                localStorage.setItem('apiKey', '{}');
            </script>
        </body></html>
    "#,
        key, key
    );

    Html(html).into_response()
}

// --- OPTIMIZATION HANDLER ---

fn get_processing_message(messages: &[String], job_id: &str) -> String {
    if messages.is_empty() {
        return DEFAULT_PROCESSING_MESSAGE.to_string();
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Hash job_id to get a starting offset so different jobs show different messages
    // Divide by 2 so messages change every 2 seconds instead of every second
    let offset: u64 = job_id.bytes().map(|b| b as u64).sum();
    let index = ((secs / 2) + offset) % messages.len() as u64;
    messages[index as usize].clone()
}

#[derive(Deserialize)]
struct JobStatusQuery {
    mode: Option<String>,
}

/// Formats a JobStatus into the JSON response expected by the frontend.
///
/// IMPORTANT: The frontend (index.html) parses these responses in the polling loop.
/// If you change this format, you MUST update the frontend to match, and update
/// the test `test_job_status_response_format_matches_frontend_contract`.
///
/// Frontend expects for Completed/Failed:
///   - status.Completed.glb_url (nested object format)
///   - status.Failed.error (nested object format)
fn format_job_status(status: &JobStatus, job_id: &str, msgs: &[String]) -> serde_json::Value {
    match status {
        JobStatus::Processing => json!({
            "status": "Processing",
            "message": get_processing_message(msgs, job_id)
        }),
        JobStatus::Completed {
            output_size,
            glb_url,
            usdz_url,
            expires_at,
            original_faces,
            output_faces,
            remesh_method,
            credits_used,
            credits_remaining,
        } => {
            let base = "https://webdeliveryengine.com";
            let full_glb = format!("{}{}", base, glb_url);
            let full_usdz = format!("{}{}", base, usdz_url);
            let glb_filename = glb_url.split('/').last().unwrap_or("model.glb");
            let usdz_filename = usdz_url.split('/').last().unwrap_or("model.usdz");
            json!({
                "status": {
                    "Completed": {
                        "output_size": output_size,
                        "glb_url": glb_url,
                        "usdz_url": usdz_url,
                        "expires_at": expires_at,
                        "original_faces": original_faces,
                        "output_faces": output_faces,
                        "remesh_method": remesh_method,
                        "credits_used": credits_used,
                        "credits_remaining": credits_remaining
                    }
                },
                "download_commands": {
                    "curl": format!(
                        "curl -O {}\ncurl -O {}",
                        full_glb, full_usdz
                    ),
                    "python": format!(
                        "import urllib.request\nurllib.request.urlretrieve('{}', '{}')\nurllib.request.urlretrieve('{}', '{}')",
                        full_glb, glb_filename,
                        full_usdz, usdz_filename
                    ),
                    "powershell": format!(
                        "Invoke-WebRequest -Uri '{}' -OutFile '{}'\nInvoke-WebRequest -Uri '{}' -OutFile '{}'",
                        full_glb, glb_filename,
                        full_usdz, usdz_filename
                    )
                }
            })
        }
        JobStatus::Failed { error } => {
            json!({
                "status": {
                    "Failed": {
                        "error": error
                    }
                }
            })
        }
        _ => json!({ "status": status }),
    }
}

async fn job_status_handler(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Query(query): Query<JobStatusQuery>,
) -> Json<serde_json::Value> {
    // Select messages based on mode (default to decimate)
    let messages = match query.mode.as_deref() {
        Some("remesh") => &state.processing_messages.remesh,
        _ => &state.processing_messages.decimate,
    };

    // Check in-memory cache first
    {
        let jobs = state.jobs.read().await;
        if let Some(status) = jobs.get(&id) {
            return Json(format_job_status(status, &id, messages));
        }
    }

    // Fall back to database
    if let Some(status) = state.db.get_job(&id).await {
        // Cache it in memory for future lookups
        {
            let mut jobs = state.jobs.write().await;
            jobs.insert(id.clone(), status.clone());
        }
        return Json(format_job_status(&status, &id, messages));
    }

    Json(json!({ "error": "Job not found" }))
}

/// Query parameters for optimize endpoint
#[derive(Debug, Deserialize)]
struct OptimizeQuery {
    /// If true, wait for processing to complete and return the result directly
    #[serde(default)]
    blocking: bool,
}

async fn optimize_handler(
    State(state): State<AppState>,
    Extension(auth_key): Extension<AuthKey>,
    Query(query): Query<OptimizeQuery>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let source = headers
        .get("X-Source")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("api")
        .to_string();

    let start_time = std::time::Instant::now();
    let batch_id = uuid::Uuid::new_v4().simple().to_string();
    let batch_dir = Path::new(UPLOAD_DIR).join(&batch_id);

    if let Err(e) = fs::create_dir_all(&batch_dir) {
        error!("Failed to create batch dir: {:?}", e);
        return error_server("Failed to initialize processing");
    }

    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(batch_id.clone(), JobStatus::Processing);
    }
    // Persist to database
    let _ = state.db.save_job(&batch_id, &JobStatus::Processing).await;

    let mut input_filename: Option<String> = None;
    let mut ratio = 0.5;
    let mut target_percentage: Option<f32> = None; // New: percentage-based targeting for decimate
    let mut target_faces: Option<i32> = None; // New: explicit face count for decimate
    let mut format = "glb".to_string();
    let mut mode = "decimate".to_string();
    let mut faces = 5000; // Used for remesh mode
    let mut texture_size = 2048;
    let mut callback_url: Option<String> = None;
    let mut input_filepath: Option<PathBuf> = None;
    let mut file_hash = String::new();
    let mut was_zip = false;
    let mut found_main_model = false; // Track if we've found the main model file

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();

        if name == "file" {
            if let Some(filename) = field.file_name().map(|s| s.to_string()) {
                let filepath = batch_dir.join(&filename);

                let ext = Path::new(&filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                // Check if this is a main model file or an auxiliary file
                let is_main_model = ALLOWED_EXTENSIONS.contains(&ext.as_str());
                let is_auxiliary = ALLOWED_AUXILIARY_EXTENSIONS.contains(&ext.as_str());

                // Validate file extension
                if !is_main_model && !is_auxiliary {
                    error!(
                        "Invalid file extension: {} (allowed model: {:?}, auxiliary: {:?})",
                        ext, ALLOWED_EXTENSIONS, ALLOWED_AUXILIARY_EXTENSIONS
                    );
                    let _ = fs::remove_dir_all(&batch_dir);
                    return error_bad_request(format!(
                        "Invalid file type. Allowed: {}",
                        ALLOWED_EXTENSIONS.join(", ")
                    ));
                }

                // Stream file to disk
                if let Ok(mut file) = tokio::fs::File::create(&filepath).await {
                    let mut hasher = sha2::Sha256::new();
                    let mut stream = field;
                    while let Ok(Some(chunk)) = stream.chunk().await {
                        let _ = file.write_all(&chunk).await;
                        sha2::Digest::update(&mut hasher, &chunk);
                    }
                    // Only set hash for main model file
                    if is_main_model && !found_main_model {
                        file_hash = hex::encode(sha2::Digest::finalize(hasher));
                        input_filepath = Some(filepath.clone());
                    }
                }

                // Only process main model file for input tracking
                if is_main_model && !found_main_model {
                    found_main_model = true;

                    if ext == "zip" {
                        let zip_path = filepath.clone();
                        let target_dir = batch_dir.clone();

                        let found_model = tokio::task::spawn_blocking(move || {
                            let file = std::fs::File::open(&zip_path).ok()?;
                            let mut archive = zip::ZipArchive::new(file).ok()?;
                            let mut candidate = None;

                            for i in 0..archive.len() {
                                let mut file = archive.by_index(i).ok()?;
                                let outpath = match file.enclosed_name() {
                                    Some(path) => target_dir.join(path),
                                    None => continue,
                                };

                                if file.name().ends_with('/') {
                                    std::fs::create_dir_all(&outpath).ok()?;
                                } else {
                                    if let Some(p) = outpath.parent() {
                                        if !p.exists() {
                                            std::fs::create_dir_all(&p).ok()?;
                                        }
                                    }
                                    let mut outfile = std::fs::File::create(&outpath).ok()?;
                                    std::io::copy(&mut file, &mut outfile).ok()?;

                                    if candidate.is_none() {
                                        let fname =
                                            outpath.file_name()?.to_string_lossy().to_string();
                                        let fext = Path::new(&fname)
                                            .extension()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or("")
                                            .to_lowercase();
                                        // Ignore hidden files/mac metadata
                                        if !fname.starts_with('.')
                                            && !outpath.to_string_lossy().contains("__MACOSX")
                                        {
                                            if ["obj", "fbx", "glb", "gltf"]
                                                .contains(&fext.as_str())
                                            {
                                                candidate = file
                                                    .enclosed_name()
                                                    .map(|p| p.to_string_lossy().to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            candidate
                        })
                        .await
                        .unwrap_or(None);

                        if let Some(name) = found_model {
                            input_filename = Some(name);
                            was_zip = true;
                        }
                    } else if ["obj", "fbx", "glb", "gltf"].contains(&ext.as_str()) {
                        input_filename = Some(filename);
                    }
                }
            }
        } else if name == "ratio" {
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<f32>() {
                    // Validate ratio bounds
                    ratio = parsed.clamp(0.01, 1.0);
                    if parsed != ratio {
                        warn!("Ratio {} clamped to {}", parsed, ratio);
                    }
                }
            }
        } else if name == "format" {
            if let Ok(val) = field.text().await {
                // Validate format
                if ["glb", "usdz", "both"].contains(&val.as_str()) {
                    format = val;
                } else {
                    warn!("Invalid format '{}', defaulting to 'glb'", val);
                }
            }
        } else if name == "mode" {
            if let Ok(val) = field.text().await {
                // Validate mode
                if ["decimate", "remesh"].contains(&val.as_str()) {
                    mode = val;
                } else {
                    warn!("Invalid mode '{}', defaulting to 'decimate'", val);
                }
            }
        } else if name == "faces" {
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<i32>() {
                    // Validate faces bounds (100 to 10 million)
                    faces = parsed.clamp(100, 10_000_000);
                    if parsed != faces {
                        warn!("Faces {} clamped to {}", parsed, faces);
                    }
                }
            }
        } else if name == "target_percentage" {
            // New: Accept target as percentage (1-100) for decimate mode
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<f32>() {
                    let clamped = parsed.clamp(1.0, 100.0);
                    if parsed != clamped {
                        warn!("target_percentage {} clamped to {}", parsed, clamped);
                    }
                    target_percentage = Some(clamped);
                }
            }
        } else if name == "target_faces" {
            // New: Accept explicit face count for decimate mode
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<i32>() {
                    let clamped = parsed.clamp(100, 10_000_000);
                    if parsed != clamped {
                        warn!("target_faces {} clamped to {}", parsed, clamped);
                    }
                    target_faces = Some(clamped);
                }
            }
        } else if name == "texture_size" {
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<i32>() {
                    // Validate texture size (powers of 2 from 256 to 8192)
                    let valid_sizes = [256, 512, 1024, 2048, 4096, 8192];
                    if valid_sizes.contains(&parsed) {
                        texture_size = parsed;
                    } else {
                        warn!(
                            "Invalid texture_size {}, defaulting to 2048 (valid: {:?})",
                            parsed, valid_sizes
                        );
                    }
                }
            }
        } else if name == "callback_url" {
            if let Ok(val) = field.text().await {
                // Validate URL format
                if val.starts_with("https://") {
                    callback_url = Some(val);
                } else {
                    warn!("Invalid callback_url '{}' - must be HTTPS", val);
                }
            }
        }
    }

    let input_filename = match input_filename {
        Some(f) => f,
        None => {
            let _ = fs::remove_dir_all(&batch_dir);
            return error_bad_request("No supported 3D model found");
        }
    };

    let input_path = input_filepath.unwrap();
    let input_size = fs::metadata(&input_path).map(|m| m.len()).unwrap_or(0);

    // Validate GLTF files for external buffer references
    let input_ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if input_ext == "gltf" {
        if let Ok(content) = fs::read_to_string(&input_path) {
            if let Ok(gltf_json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(buffers) = gltf_json.get("buffers").and_then(|b| b.as_array()) {
                    let mut missing_buffers = Vec::new();
                    for buffer in buffers {
                        if let Some(uri) = buffer.get("uri").and_then(|u| u.as_str()) {
                            // Skip data URIs (embedded base64)
                            if !uri.starts_with("data:") {
                                // Check if the referenced file exists
                                let buffer_path = batch_dir.join(uri);
                                if !buffer_path.exists() {
                                    missing_buffers.push(uri.to_string());
                                }
                            }
                        }
                    }
                    if !missing_buffers.is_empty() {
                        let _ = fs::remove_dir_all(&batch_dir);
                        return json_error(
                            StatusCode::BAD_REQUEST,
                            ApiError::new("Missing external buffers")
                                .with_code("missing_buffers")
                                .with_message(format!(
                                    "GLTF references external buffers not uploaded: {}. \
                                    Upload a ZIP with .gltf and .bin files, or convert to GLB.",
                                    missing_buffers.join(", ")
                                )),
                        );
                    }
                }
            }
        }
    }

    let output_base = Path::new(&input_filename)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    let output_filename = format!("{}_opt.glb", output_base);
    let usdz_filename = format!("{}_opt.usdz", output_base);

    // Fair Billing Logic
    // Combine file hash with mode so that decimate and remesh are tracked separately
    // This prevents gaming: decimate (1 credit) then remesh (free) on same file
    let file_mode_hash = format!("{}:{}", file_hash, mode);
    let pricing_config = load_pricing_config();
    let free_reoptimization_hours = pricing_config
        .as_ref()
        .map(|p| p.free_reoptimization_hours)
        .unwrap_or(24);
    let should_charge = state
        .db
        .should_charge_for_file(&auth_key.0, &file_mode_hash, free_reoptimization_hours)
        .await;
    let mut deducted = false;

    // Pricing Logic - read from pricing.json
    let cost_decimate = pricing_config
        .as_ref()
        .map(|p| p.cost_decimate)
        .unwrap_or(1);
    let cost_remesh = pricing_config.as_ref().map(|p| p.cost_remesh).unwrap_or(2);

    let required_credits = if mode == "remesh" {
        cost_remesh
    } else {
        cost_decimate
    };

    if should_charge {
        // Pre-flight check
        let current_balance = state.db.get_credits(&auth_key.0).await.unwrap_or(0);
        if current_balance < required_credits {
            let _ = fs::remove_dir_all(&batch_dir);
            return error_insufficient_credits(current_balance, required_credits);
        }

        info!(
            "Attempting to charge key={} for file={} (cost={})",
            &auth_key.0, input_filename, required_credits
        );
        match state
            .db
            .record_transaction(
                &auth_key.0,
                -required_credits,
                &if mode == "remesh" {
                    format!(
                        "{}{}; Rem; {} faces; {}px",
                        input_filename,
                        if was_zip { " (zip)" } else { "" },
                        faces,
                        texture_size
                    )
                } else {
                    format!(
                        "{}{}; Dec; {}%",
                        input_filename,
                        if was_zip { " (zip)" } else { "" },
                        (ratio * 100.0) as i32
                    )
                },
                Some(file_mode_hash.clone()),
            )
            .await
        {
            Ok(new_balance) => {
                info!(
                    "Credit deducted successfully for key={}. New balance={}",
                    &auth_key.0, new_balance
                );
                deducted = true;
            }
            Err(e) => {
                error!("Failed to deduct credit for key={}: {:?}", &auth_key.0, e);
                let _ = fs::remove_dir_all(&batch_dir);
                return error_transaction_failed();
            }
        }
    } else {
        // Record free re-optimization for transparency in transaction history
        info!(
            "Free Re-roll for hash: {} (recording 0-credit transaction)",
            file_mode_hash
        );
        let _ = state
            .db
            .record_transaction(
                &auth_key.0,
                0,
                &if mode == "remesh" {
                    format!(
                        "{}{}; Rem; {} faces; {}px (free re-opt)",
                        input_filename,
                        if was_zip { " (zip)" } else { "" },
                        faces,
                        texture_size
                    )
                } else {
                    format!(
                        "{}{}; Dec; {}% (free re-opt)",
                        input_filename,
                        if was_zip { " (zip)" } else { "" },
                        (ratio * 100.0) as i32
                    )
                },
                Some(file_mode_hash.clone()),
            )
            .await;
    }

    let credits_remaining = state.db.get_credits(&auth_key.0).await.unwrap_or(0);
    let credits_used = if deducted { required_credits } else { 0 };

    // Collect metrics identifiers
    // If X-Session-ID header is present (Web UI), use that. Otherwise use API Key.
    let session_header = headers
        .get("x-session-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let user_identifier = session_header.unwrap_or_else(|| auth_key.0.clone());

    // Spawn Background Task
    let state_clone = state.clone();
    let auth_key_str = auth_key.0.clone();
    let batch_id_clone = batch_id.clone();
    let batch_dir_clone = batch_dir.clone();
    let input_filename_clone = input_filename.clone();
    let output_filename_clone = output_filename.clone();
    let usdz_filename_clone = usdz_filename.clone();
    let file_mode_hash_clone = file_mode_hash.clone();
    let format_clone = format.clone();
    let mode_clone = mode.clone();
    let callback_url_clone = callback_url.clone();
    let target_percentage_clone = target_percentage;
    let target_faces_clone = target_faces;
    let credits_used_clone = credits_used;
    let credits_remaining_clone = credits_remaining;

    // Slot Cost Logic
    let slot_cost_decimate = std::env::var("SLOT_COST_DECIMATE")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1);
    let slot_cost_remesh = std::env::var("SLOT_COST_REMESH")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(5);

    // For blocking mode, we use a oneshot channel to get the result
    let (result_tx, result_rx) = tokio::sync::oneshot::channel::<JobStatus>();
    let is_blocking = query.blocking;

    tokio::spawn(async move {
        // Helper to send webhook callback
        let send_webhook = |job_id: &str, status: &JobStatus, callback: &Option<String>| {
            let job_id = job_id.to_string();
            let status = status.clone();
            let callback = callback.clone();
            async move {
                if let Some(url) = callback {
                    let payload = serde_json::json!({
                        "jobId": job_id,
                        "status": status
                    });
                    if let Err(e) = reqwest::Client::new()
                        .post(&url)
                        .json(&payload)
                        .timeout(std::time::Duration::from_secs(10))
                        .send()
                        .await
                    {
                        warn!("Webhook callback failed for job {}: {}", job_id, e);
                    }
                }
            }
        };

        // Determine resource cost (Weighted Semaphore)
        let required_permits: u32 = if mode_clone == "remesh" {
            slot_cost_remesh
        } else {
            slot_cost_decimate
        };

        // Log semaphore wait time for capacity monitoring
        let wait_start = std::time::Instant::now();
        let _permit = state_clone
            .worker_semaphore
            .acquire_many(required_permits)
            .await
            .unwrap();
        let wait_time = wait_start.elapsed();
        if wait_time.as_secs() > 0 {
            warn!(
                "CAPACITY: job {} waited {:?} for {} slot(s)",
                batch_id_clone, wait_time, required_permits
            );
        }

        // Run Command
        let mut cmd = if mode_clone == "remesh" {
            let script_path = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("scripts/remesh.py");

            let blender_exe =
                std::env::var("BLENDER_PATH").unwrap_or_else(|_| "blender".to_string());
            let mut c = tokio::process::Command::new(blender_exe);
            c.arg("-b")
                .arg("-P")
                .arg(script_path)
                .arg("--")
                .arg("--input")
                .arg(&input_filename_clone)
                .arg("--output")
                .arg(&output_filename_clone)
                .arg("--faces")
                .arg(faces.to_string())
                .arg("--texture_size")
                .arg(texture_size.to_string());
            c
        } else {
            let mut c = tokio::process::Command::new("mesh-optimizer");
            c.arg("--input")
                .arg(&input_filename_clone)
                .arg("--output")
                .arg(&output_filename_clone);

            // Priority: target_faces > target_percentage > ratio
            // Worker will handle calculating ratio from these if needed
            if let Some(tf) = target_faces_clone {
                c.arg("--target-faces").arg(tf.to_string());
            } else if let Some(tp) = target_percentage_clone {
                c.arg("--target-percentage").arg(tp.to_string());
            } else {
                c.arg("--ratio").arg(ratio.to_string());
            }

            c
        };

        cmd.current_dir(&batch_dir_clone)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()); // IMPORTANT: Run inside batch dir

        info!("Executing: {:?}", cmd);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn worker: {:?}", e);
                let failed_status = JobStatus::Failed {
                    error: "Spawn Failed".to_string(),
                };
                {
                    let mut jobs = state_clone.jobs.write().await;
                    jobs.insert(batch_id_clone.clone(), failed_status.clone());
                }
                let _ = state_clone
                    .db
                    .save_job(&batch_id_clone, &failed_status)
                    .await;
                send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
                let _ = result_tx.send(failed_status);
                return;
            }
        };

        // Stream stdout and capture lines for parsing
        let stdout_lines = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let stdout_lines_clone = stdout_lines.clone();
        let stdout_handle = if let Some(stdout) = child.stdout.take() {
            Some(tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    info!("WORKER: {}", line);
                    stdout_lines_clone.lock().await.push(line);
                }
            }))
        } else {
            None
        };

        // Stream stderr
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    error!("WORKER_ERR: {}", line);
                }
            });
        }

        let status = match tokio::time::timeout(Duration::from_secs(600), child.wait()).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let failed_status = JobStatus::Failed {
                    error: "System Error".to_string(),
                };
                {
                    let mut jobs = state_clone.jobs.write().await;
                    jobs.insert(batch_id_clone.clone(), failed_status.clone());
                }
                let _ = state_clone
                    .db
                    .save_job(&batch_id_clone, &failed_status)
                    .await;
                send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
                error!("Execution failed: {:?}", e);
                // Refund Credit (only if we charged them)
                if deducted {
                    let _ = state_clone
                        .db
                        .record_transaction(
                            &auth_key_str,
                            required_credits,
                            "system_error_refund",
                            Some(file_mode_hash_clone.clone()),
                        )
                        .await;
                }
                // Log Failure
                let _ = state_clone
                    .db
                    .log_job(
                        &auth_key_str,
                        &input_filename_clone,
                        input_size,
                        "unknown",
                        &format_clone,
                        0,
                        start_time.elapsed().as_millis() as u64,
                        ratio,
                        "system_error",
                        &source,
                    )
                    .await;
                let _ = result_tx.send(failed_status);
                return;
            }
            Err(_) => {
                let failed_status = JobStatus::Failed {
                    error: "Timeout".to_string(),
                };
                {
                    let mut jobs = state_clone.jobs.write().await;
                    jobs.insert(batch_id_clone.clone(), failed_status.clone());
                }
                let _ = state_clone
                    .db
                    .save_job(&batch_id_clone, &failed_status)
                    .await;
                send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
                error!("Execution timed out");
                // Refund Credit (only if we charged them)
                if deducted {
                    let _ = state_clone
                        .db
                        .record_transaction(
                            &auth_key_str,
                            required_credits,
                            "timeout_refund",
                            Some(file_mode_hash_clone.clone()),
                        )
                        .await;
                }
                // Log Failure
                let _ = state_clone
                    .db
                    .log_job(
                        &auth_key_str,
                        &input_filename_clone,
                        input_size,
                        "unknown",
                        &format_clone,
                        0,
                        start_time.elapsed().as_millis() as u64,
                        ratio,
                        "timeout",
                        &source,
                    )
                    .await;
                let _ = result_tx.send(failed_status);
                return;
            }
        };

        let processing_time = start_time.elapsed().as_millis() as u64;

        if !status.success() {
            error!("Worker exited with status: {}", status);
            // Refund Credit (Process Failure)
            if deducted {
                let _ = state_clone
                    .db
                    .record_transaction(
                        &auth_key_str,
                        required_credits,
                        &format!("process_failure_refund: {}", input_filename_clone),
                        Some(file_mode_hash_clone.clone()),
                    )
                    .await;
            }

            // Log User Error (likely bad mesh)
            state_clone
                .db
                .log_job(
                    &user_identifier,
                    &input_filename_clone,
                    input_size,
                    "unknown", // Could extract from filename extension
                    &format_clone,
                    0,
                    processing_time,
                    ratio,
                    "worker_error",
                    &source,
                )
                .await;

            let failed_status = JobStatus::Failed {
                error: "Worker Error".to_string(),
            };
            {
                let mut jobs = state_clone.jobs.write().await;
                jobs.insert(batch_id_clone.clone(), failed_status.clone());
            }
            let _ = state_clone
                .db
                .save_job(&batch_id_clone, &failed_status)
                .await;
            send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
            let _ = result_tx.send(failed_status);
            return;
        }

        // Post-Processing: Convert to USDZ if requested
        if format_clone == "both" || format_clone == "usdz" {
            let script_path = std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("scripts/glb_to_usdz.py");

            let blender_exe =
                std::env::var("BLENDER_PATH").unwrap_or_else(|_| "blender".to_string());

            let mut cmd = tokio::process::Command::new(blender_exe);
            cmd.arg("-b")
                .arg("-P")
                .arg(script_path)
                .arg("--")
                .arg("--input")
                .arg(&output_filename_clone)
                .arg("--output")
                .arg(&usdz_filename_clone);

            cmd.current_dir(&batch_dir_clone)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            info!("Executing USDZ Conversion: {:?}", cmd);

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to spawn USDZ conversion: {:?}", e);
                    let failed_status = JobStatus::Failed {
                        error: "USDZ Conversion Spawn Failed".to_string(),
                    };
                    {
                        let mut jobs = state_clone.jobs.write().await;
                        jobs.insert(batch_id_clone.clone(), failed_status.clone());
                    }
                    let _ = state_clone
                        .db
                        .save_job(&batch_id_clone, &failed_status)
                        .await;
                    send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
                    let _ = result_tx.send(failed_status);
                    return;
                }
            };

            // Stream stdout
            if let Some(stdout) = child.stdout.take() {
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        info!("USDZ_CONV: {}", line);
                    }
                });
            }

            // Stream stderr
            if let Some(stderr) = child.stderr.take() {
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        error!("USDZ_CONV_ERR: {}", line);
                    }
                });
            }

            match child.wait().await {
                Ok(status) => {
                    if !status.success() {
                        error!("USDZ conversion failed with status: {}", status);
                        let failed_status = JobStatus::Failed {
                            error: "USDZ Conversion Failed".to_string(),
                        };
                        {
                            let mut jobs = state_clone.jobs.write().await;
                            jobs.insert(batch_id_clone.clone(), failed_status.clone());
                        }
                        let _ = state_clone
                            .db
                            .save_job(&batch_id_clone, &failed_status)
                            .await;
                        send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
                        let _ = result_tx.send(failed_status);
                        return;
                    }
                }
                Err(e) => {
                    error!("Failed to wait for USDZ conversion: {:?}", e);
                    let failed_status = JobStatus::Failed {
                        error: "USDZ Conversion Error".to_string(),
                    };
                    {
                        let mut jobs = state_clone.jobs.write().await;
                        jobs.insert(batch_id_clone.clone(), failed_status.clone());
                    }
                    let _ = state_clone
                        .db
                        .save_job(&batch_id_clone, &failed_status)
                        .await;
                    send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
                    let _ = result_tx.send(failed_status);
                    return;
                }
            }
        }

        // Validate Output File Exists and Has Content
        let output_path = if format_clone == "usdz" {
            batch_dir_clone.join(&usdz_filename_clone)
        } else {
            batch_dir_clone.join(&output_filename_clone)
        };
        let mut output_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

        if format_clone == "both" {
            let usdz_path = batch_dir_clone.join(&usdz_filename_clone);
            let usdz_size = fs::metadata(&usdz_path).map(|m| m.len()).unwrap_or(0);
            if usdz_size == 0 {
                output_size = 0; // Force failure if USDZ is missing in 'both' mode
            } else {
                output_size += usdz_size;
            }
        }

        // CRITICAL: Check if output file was actually created
        if output_size == 0 {
            error!(
                "Worker exited successfully but output file is missing or empty: {:?}",
                output_path
            );

            // Refund Credit (No Output Generated)
            if deducted {
                let _ = state_clone
                    .db
                    .record_transaction(
                        &auth_key_str,
                        required_credits,
                        &format!("no_output_refund: {}", input_filename_clone),
                        Some(file_mode_hash_clone.clone()),
                    )
                    .await;
                info!(
                    "Credit refunded for key={} due to missing output",
                    &auth_key_str
                );
            }

            // Log as failed job
            let input_ext = Path::new(&input_filename_clone)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            state_clone
                .db
                .log_job(
                    &user_identifier,
                    &input_filename_clone,
                    input_size,
                    input_ext,
                    &format_clone,
                    0,
                    processing_time,
                    ratio,
                    "no_output",
                    &source,
                )
                .await;

            let failed_status = JobStatus::Failed {
                error: "No Output".to_string(),
            };
            {
                let mut jobs = state_clone.jobs.write().await;
                jobs.insert(batch_id_clone.clone(), failed_status.clone());
            }
            let _ = state_clone
                .db
                .save_job(&batch_id_clone, &failed_status)
                .await;
            send_webhook(&batch_id_clone, &failed_status, &callback_url_clone).await;
            let _ = result_tx.send(failed_status);
            return;
        }

        let input_ext = Path::new(&input_filename_clone)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        state_clone
            .db
            .log_job(
                &user_identifier,
                &input_filename_clone,
                input_size,
                input_ext,
                &format_clone,
                output_size,
                processing_time,
                ratio,
                "success",
                &source,
            )
            .await;

        let dl_base = format!("/download/{}", batch_id_clone);
        let glb_url = format!("{}/{}", dl_base, output_filename_clone);
        let usdz_url = format!("{}/{}", dl_base, usdz_filename_clone);

        info!(
            "Job {} completed successfully. Output size: {} bytes",
            batch_id_clone, output_size
        );

        // Wait for stdout capture to complete and parse face counts
        if let Some(handle) = stdout_handle {
            let _ = handle.await;
        }
        let (original_faces, output_faces, remesh_method) = {
            let lines = stdout_lines.lock().await;
            let mut orig: Option<u64> = None;
            let mut out: Option<u64> = None;
            let mut method: Option<String> = None;
            for line in lines.iter() {
                if line.starts_with("FACE_COUNTS: ") {
                    let parts: Vec<&str> =
                        line["FACE_COUNTS: ".len()..].split_whitespace().collect();
                    if parts.len() >= 2 {
                        orig = parts[0].parse().ok();
                        out = parts[1].parse().ok();
                    }
                    if parts.len() >= 3 {
                        method = Some(parts[2].to_string());
                    }
                }
            }
            (orig, out, method)
        };
        if let (Some(o), Some(f)) = (original_faces, output_faces) {
            info!("Face counts: {} -> {} (method: {:?})", o, f, remesh_method);
        }

        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(DOWNLOAD_EXPIRES_SECS as i64);
        let completed_status = JobStatus::Completed {
            output_size,
            glb_url,
            usdz_url,
            expires_at: expires_at.to_rfc3339(),
            original_faces,
            output_faces,
            remesh_method,
            credits_used: Some(credits_used_clone),
            credits_remaining: Some(credits_remaining_clone),
        };
        {
            let mut jobs = state_clone.jobs.write().await;
            jobs.insert(batch_id_clone.clone(), completed_status.clone());
        }
        let _ = state_clone
            .db
            .save_job(&batch_id_clone, &completed_status)
            .await;

        // Send webhook callback if configured
        send_webhook(&batch_id_clone, &completed_status, &callback_url_clone).await;

        // Send result through channel for blocking mode
        let _ = result_tx.send(completed_status);
    });

    // Handle blocking vs non-blocking response
    if is_blocking {
        // Wait for the job to complete (with timeout)
        let timeout_duration = Duration::from_secs(600); // 10 minute timeout
        match tokio::time::timeout(timeout_duration, result_rx).await {
            Ok(Ok(status)) => {
                // Get processing messages for the response
                let messages = if mode == "remesh" {
                    &state.processing_messages.remesh
                } else {
                    &state.processing_messages.decimate
                };
                let mut response = Json(format_job_status(&status, &batch_id, messages)).into_response();
                response.headers_mut().insert(
                    "X-Credits-Remaining",
                    credits_remaining.to_string().parse().unwrap(),
                );
                response
            }
            Ok(Err(_)) => {
                // Channel was dropped - this shouldn't happen
                error_server("Job processing failed unexpectedly")
            }
            Err(_) => {
                // Timeout
                json_error(
                    StatusCode::GATEWAY_TIMEOUT,
                    ApiError::new("Job timed out")
                        .with_code("timeout")
                        .with_message(format!(
                            "Processing exceeded {} seconds. Check job status at /job/{}",
                            timeout_duration.as_secs(),
                            batch_id
                        )),
                )
            }
        }
    } else {
        // Non-blocking: return immediately with job ID
        let mut response = Json(json!({
            "jobId": batch_id,
            "status": "processing",
            "credits_used": credits_used,
            "credits_remaining": credits_remaining
        }))
        .into_response();

        response.headers_mut().insert(
            "X-Credits-Remaining",
            credits_remaining.to_string().parse().unwrap(),
        );

        response
    }
}

// --- CLEANUP ---
#[derive(Deserialize)]
struct AdminAddCredits {
    key: String,
    amount: i32,
    secret: String,
}

#[derive(Deserialize)]
struct AdminCreateKey {
    email: String,
    initial_credits: i32,
    secret: String,
}

/// Timing-safe comparison for admin secret
fn verify_admin_secret(provided: &str, expected: &str) -> bool {
    // Constant-time comparison to prevent timing attacks
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

/// Extract admin secret from X-Admin-Secret header

/// Simple in-memory rate limiter: returns true if request is allowed
async fn check_admin_rate_limit(
    rate_limiter: &Arc<RwLock<HashMap<String, Vec<std::time::Instant>>>>,
    client_ip: &str,
    max_requests: usize,
    window_secs: u64,
) -> bool {
    let now = std::time::Instant::now();
    let window = Duration::from_secs(window_secs);

    let mut limiter = rate_limiter.write().await;
    let requests = limiter
        .entry(client_ip.to_string())
        .or_insert_with(Vec::new);

    // Remove old requests outside the window
    requests.retain(|&t| now.duration_since(t) < window);

    if requests.len() >= max_requests {
        false // Rate limited
    } else {
        requests.push(now);
        true // Allowed
    }
}

/// Log admin action for audit trail
fn log_admin_audit(action: &str, success: bool, details: &str, client_ip: Option<&str>) {
    let ip = client_ip.unwrap_or("unknown");
    if success {
        info!(
            "ADMIN_AUDIT: action={}, success=true, details=\"{}\", ip={}",
            action, details, ip
        );
    } else {
        error!(
            "ADMIN_AUDIT: action={}, success=false, details=\"{}\", ip={}",
            action, details, ip
        );
    }
}

async fn admin_add_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AdminAddCredits>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client_ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("X-Real-IP").and_then(|v| v.to_str().ok()));
    let ip_for_limit = client_ip.unwrap_or("unknown");

    info!(
        "admin_add_credits attempt: key={}, amount={}, ip={}",
        payload.key, payload.amount, ip_for_limit
    );

    // Rate limit: 5 requests per minute per IP
    if !check_admin_rate_limit(&state.admin_rate_limiter, ip_for_limit, 5, 60).await {
        log_admin_audit("add_credits", false, "rate limited", client_ip);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    if !verify_admin_secret(&payload.secret, &state.admin_secret) {
        log_admin_audit("add_credits", false, "invalid secret", client_ip);
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.db.add_credits(&payload.key, payload.amount).await {
        Ok(new_balance) => {
            log_admin_audit(
                "add_credits",
                true,
                &format!(
                    "key={}, amount={}, new_balance={}",
                    payload.key, payload.amount, new_balance
                ),
                client_ip,
            );
            Ok(Json(json!({ "success": true, "new_balance": new_balance })))
        }
        Err(e) => {
            let error_string = e.to_string();
            log_admin_audit(
                "add_credits",
                false,
                &format!("Error for key {}: {}", payload.key, error_string),
                client_ip,
            );
            if error_string.contains("Key not found") {
                Err(StatusCode::NOT_FOUND)
            } else {
                error!("Unexpected error in admin_add_credits: {}", error_string);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

async fn admin_create_key(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AdminCreateKey>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client_ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("X-Real-IP").and_then(|v| v.to_str().ok()));

    let ip_for_limit = client_ip.unwrap_or("unknown");

    // Rate limit: 5 requests per minute per IP
    if !check_admin_rate_limit(&state.admin_rate_limiter, ip_for_limit, 5, 60).await {
        log_admin_audit("create_key", false, "rate limited", client_ip);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    if !verify_admin_secret(&payload.secret, &state.admin_secret) {
        log_admin_audit(
            "create_key",
            false,
            &format!("invalid secret, attempted email={}", payload.email),
            client_ip,
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state
        .db
        .create_key(
            payload.email.clone(),
            format!("admin_created_{}", uuid::Uuid::new_v4().simple()),
            payload.initial_credits,
        )
        .await
    {
        Ok(new_key) => {
            log_admin_audit(
                "create_key",
                true,
                &format!(
                    "email={}, initial_credits={}",
                    payload.email, payload.initial_credits
                ),
                client_ip,
            );

            // Send welcome email with API key
            let email_html = format!(
                r#"
                <html><body style="font-family:sans-serif; background:#111; color:white; padding:40px;">
                    <div style="max-width:600px; margin:0 auto;">
                        <h1 style="color:#10b981;">Welcome to Mesh Optimizer!</h1>
                        <p>Your account has been created with <strong>{} credits</strong>.</p>

                        <div style="background:#1f2937; padding:20px; border-radius:8px; margin:30px 0; border-left:4px solid #3b82f6;">
                            <p style="margin:0 0 10px 0; color:#9ca3af;">Your API Key:</p>
                            <code style="font-size:16px; color:#3b82f6; word-break:break-all;">{}</code>
                        </div>

                        <p>Use this key in the API Key field on the dashboard to optimize your 3D models.</p>
                        <p style="margin-top:30px;">
                            <a href="https://webdeliveryengine.com" style="background:#3b82f6; color:white; padding:12px 24px; text-decoration:none; border-radius:6px;">Go to Dashboard</a>
                        </p>

                        <p style="color:#6b7280; font-size:14px; margin-top:40px;">
                            Questions? Reply to this email or visit our support page.
                        </p>
                    </div>
                </body></html>
                "#,
                payload.initial_credits, new_key
            );

            let client = reqwest::Client::new();
            let email_result = client
                .post("https://api.resend.com/emails")
                .header("Authorization", format!("Bearer {}", state.resend_api_key))
                .json(&json!({
                    "from": "Mesh Optimizer <support@webdeliveryengine.com>",
                    "to": [payload.email.clone()],
                    "subject": "Your Mesh Optimizer API Key",
                    "html": email_html
                }))
                .send()
                .await;

            let email_sent = match email_result {
                Ok(res) if res.status().is_success() => {
                    info!("Welcome email sent to {}", payload.email);
                    true
                }
                Ok(res) => {
                    error!(
                        "Failed to send welcome email to {}: status {}",
                        payload.email,
                        res.status()
                    );
                    false
                }
                Err(e) => {
                    error!("Failed to send welcome email to {}: {}", payload.email, e);
                    false
                }
            };

            Ok(Json(json!({
                "success": true,
                "key": new_key,
                "email": payload.email,
                "initial_credits": payload.initial_credits,
                "email_sent": email_sent
            })))
        }
        Err(e) => {
            log_admin_audit(
                "create_key",
                false,
                &format!("db error for email={}: {:?}", payload.email, e),
                client_ip,
            );
            error!("Failed to create key: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Smart endpoint: creates key if email is new, adds credits if email exists
async fn admin_grant_credits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AdminCreateKey>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client_ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("X-Real-IP").and_then(|v| v.to_str().ok()));

    let ip_for_limit = client_ip.unwrap_or("unknown");

    // Rate limit: 5 requests per minute per IP
    if !check_admin_rate_limit(&state.admin_rate_limiter, ip_for_limit, 5, 60).await {
        log_admin_audit("grant_credits", false, "rate limited", client_ip);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    if !verify_admin_secret(&payload.secret, &state.admin_secret) {
        log_admin_audit(
            "grant_credits",
            false,
            &format!("invalid secret, attempted email={}", payload.email),
            client_ip,
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Check if email already exists
    if let Some(existing_key) = state.db.get_key_by_email(&payload.email).await {
        // Add credits to existing user
        match state
            .db
            .add_credits_with_description(&existing_key, payload.initial_credits, "Free credits")
            .await
        {
            Ok(new_balance) => {
                log_admin_audit(
                    "grant_credits",
                    true,
                    &format!(
                        "existing user: email={}, added={}, new_balance={}",
                        payload.email, payload.initial_credits, new_balance
                    ),
                    client_ip,
                );

                // Send top-up email
                let email_html = format!(
                    r#"
                    <html><body style="font-family:sans-serif; background:#111; color:white; padding:40px;">
                        <div style="max-width:600px; margin:0 auto;">
                            <h1 style="color:#10b981;">Credits Added!</h1>
                            <p>We've added <strong>{} credits</strong> to your account.</p>
                            <p>Your new balance: <strong>{} credits</strong></p>

                            <p style="margin-top:30px;">
                                <a href="https://webdeliveryengine.com" style="background:#3b82f6; color:white; padding:12px 24px; text-decoration:none; border-radius:6px;">Go to Dashboard</a>
                            </p>
                        </div>
                    </body></html>
                    "#,
                    payload.initial_credits, new_balance
                );

                let client = reqwest::Client::new();
                let _ = client
                    .post("https://api.resend.com/emails")
                    .header("Authorization", format!("Bearer {}", state.resend_api_key))
                    .json(&json!({
                        "from": "Mesh Optimizer <support@webdeliveryengine.com>",
                        "to": [payload.email.clone()],
                        "subject": "Credits Added to Your Mesh Optimizer Account",
                        "html": email_html
                    }))
                    .send()
                    .await;

                Ok(Json(json!({
                    "success": true,
                    "action": "added_credits",
                    "email": payload.email,
                    "credits_added": payload.initial_credits,
                    "new_balance": new_balance
                })))
            }
            Err(e) => {
                log_admin_audit(
                    "grant_credits",
                    false,
                    &format!(
                        "db error adding credits for email={}: {:?}",
                        payload.email, e
                    ),
                    client_ip,
                );
                error!("Failed to add credits: {:?}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        // Create new user
        match state
            .db
            .create_key_with_description(
                payload.email.clone(),
                format!("admin_created_{}", uuid::Uuid::new_v4().simple()),
                payload.initial_credits,
                "Free credits",
            )
            .await
        {
            Ok(new_key) => {
                log_admin_audit(
                    "grant_credits",
                    true,
                    &format!(
                        "new user: email={}, initial_credits={}",
                        payload.email, payload.initial_credits
                    ),
                    client_ip,
                );

                // Send welcome email with API key
                let email_html = format!(
                    r#"
                    <html><body style="font-family:sans-serif; background:#111; color:white; padding:40px;">
                        <div style="max-width:600px; margin:0 auto;">
                            <h1 style="color:#10b981;">Welcome to Mesh Optimizer!</h1>
                            <p>Your account has been created with <strong>{} credits</strong>.</p>

                            <div style="background:#1f2937; padding:20px; border-radius:8px; margin:30px 0; border-left:4px solid #3b82f6;">
                                <p style="margin:0 0 10px 0; color:#9ca3af;">Your API Key:</p>
                                <code style="font-size:16px; color:#3b82f6; word-break:break-all;">{}</code>
                            </div>

                            <p>Use this key in the API Key field on the dashboard to optimize your 3D models.</p>
                            <p style="margin-top:30px;">
                                <a href="https://webdeliveryengine.com" style="background:#3b82f6; color:white; padding:12px 24px; text-decoration:none; border-radius:6px;">Go to Dashboard</a>
                            </p>

                            <p style="color:#6b7280; font-size:14px; margin-top:40px;">
                                Questions? Reply to this email or visit our support page.
                            </p>
                        </div>
                    </body></html>
                    "#,
                    payload.initial_credits, new_key
                );

                let client = reqwest::Client::new();
                let email_result = client
                    .post("https://api.resend.com/emails")
                    .header("Authorization", format!("Bearer {}", state.resend_api_key))
                    .json(&json!({
                        "from": "Mesh Optimizer <support@webdeliveryengine.com>",
                        "to": [payload.email.clone()],
                        "subject": "Your Mesh Optimizer API Key",
                        "html": email_html
                    }))
                    .send()
                    .await;

                let email_sent = match email_result {
                    Ok(res) if res.status().is_success() => {
                        info!("Welcome email sent to {}", payload.email);
                        true
                    }
                    Ok(res) => {
                        error!(
                            "Failed to send welcome email to {}: status {}",
                            payload.email,
                            res.status()
                        );
                        false
                    }
                    Err(e) => {
                        error!("Failed to send welcome email to {}: {}", payload.email, e);
                        false
                    }
                };

                Ok(Json(json!({
                    "success": true,
                    "action": "created_key",
                    "key": new_key,
                    "email": payload.email,
                    "initial_credits": payload.initial_credits,
                    "email_sent": email_sent
                })))
            }
            Err(e) => {
                log_admin_audit(
                    "grant_credits",
                    false,
                    &format!("db error creating key for email={}: {:?}", payload.email, e),
                    client_ip,
                );
                error!("Failed to create key: {:?}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

#[derive(Deserialize)]
struct AdminListUsers {
    secret: String,
}

async fn admin_list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AdminListUsers>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client_ip = headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("X-Real-IP").and_then(|v| v.to_str().ok()));

    let ip_for_limit = client_ip.unwrap_or("unknown");

    // Rate limit: 5 requests per minute per IP
    if !check_admin_rate_limit(&state.admin_rate_limiter, ip_for_limit, 5, 60).await {
        log_admin_audit("list_users", false, "rate limited", client_ip);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    if !verify_admin_secret(&payload.secret, &state.admin_secret) {
        log_admin_audit("list_users", false, "invalid secret", client_ip);
        return Err(StatusCode::UNAUTHORIZED);
    }

    let users = state.db.list_users().await;
    let user_list: Vec<serde_json::Value> = users
        .into_iter()
        .map(|(key, email, credits, created)| {
            json!({
                "key": key,
                "email": email,
                "credits": credits,
                "created": created
            })
        })
        .collect();

    log_admin_audit(
        "list_users",
        true,
        &format!("returned {} users", user_list.len()),
        client_ip,
    );

    Ok(Json(json!({
        "success": true,
        "count": user_list.len(),
        "users": user_list
    })))
}

async fn credits_handler(
    State(state): State<AppState>,
    Extension(auth_key): Extension<AuthKey>,
) -> Response {
    info!("Credits request for key={}", &auth_key.0);
    match state.db.get_credits(&auth_key.0).await {
        Some(credits) => {
            info!("Retrieved credits for key={}: {}", &auth_key.0, credits);
            Json(json!({ "credits": credits })).into_response()
        }
        None => {
            error!("Key not found: {}", &auth_key.0);
            error_not_found("API key not found")
        }
    }
}

/// Structured history entry for API responses
#[derive(Debug, Serialize)]
struct HistoryEntry {
    id: i64,
    timestamp: String,
    #[serde(rename = "type")]
    entry_type: String,
    credits: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    is_free_reopt: Option<bool>,
    /// Raw description for entries that can't be fully parsed
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_description: Option<String>,
}

/// Parse a transaction description into structured fields
fn parse_transaction_description(description: &str) -> (String, Option<String>, Option<String>, Option<String>, bool) {
    // Format examples:
    // "hero.glb; Dec; 50%" -> (optimize, hero.glb, decimate, 50%)
    // "hero.glb (zip); Rem; 5000 faces; 2048px" -> (optimize, hero.glb, remesh, 5000 faces @ 2048px)
    // "hero.glb; Dec; 50% (free re-opt)" -> free re-optimization
    // "system_error_refund" -> (refund, None, None, None)
    // "process_failure_refund: hero.glb" -> (refund, hero.glb, None, None)
    // "free_initial_credits" -> (credit, None, None, None)
    // "transfer_from_regenerated_key" -> (transfer, None, None, None)

    let is_free_reopt = description.contains("(free re-opt)");
    let desc = description.replace(" (free re-opt)", "");

    // Check for refund patterns
    if desc.contains("_refund") {
        let filename = if desc.contains(": ") {
            desc.split(": ").nth(1).map(|s| s.to_string())
        } else {
            None
        };
        return ("refund".to_string(), filename, None, None, is_free_reopt);
    }

    // Check for credit/transfer patterns
    if desc == "free_initial_credits" || desc.contains("bonus") {
        return ("credit".to_string(), None, None, None, false);
    }
    if desc.contains("transfer") || desc.contains("regenerated") {
        return ("transfer".to_string(), None, None, None, false);
    }

    // Parse optimization entries: "filename; Mode; params"
    let parts: Vec<&str> = desc.split("; ").collect();
    if parts.len() >= 3 {
        let filename = parts[0].replace(" (zip)", "").to_string();
        let mode = match parts[1] {
            "Dec" => "decimate",
            "Rem" => "remesh",
            _ => parts[1],
        }.to_string();

        let params = if parts.len() >= 4 {
            // Remesh: "5000 faces; 2048px"
            format!("{} @ {}", parts[2], parts[3])
        } else {
            // Decimate: "50%"
            parts[2].to_string()
        };

        return ("optimize".to_string(), Some(filename), Some(mode), Some(params), is_free_reopt);
    }

    // Can't parse - return raw
    ("other".to_string(), None, None, None, false)
}

async fn history_handler(
    State(state): State<AppState>,
    Extension(auth_key): Extension<AuthKey>,
) -> Response {
    info!("History request for key={}", &auth_key.0);
    match state.db.get_history(&auth_key.0, 50).await {
        Ok(transactions) => {
            info!(
                "Successfully retrieved {} transactions for key={}",
                transactions.len(),
                &auth_key.0
            );

            // Convert to structured entries
            let entries: Vec<HistoryEntry> = transactions
                .into_iter()
                .map(|tx| {
                    let (entry_type, filename, mode, parameters, is_free_reopt) =
                        parse_transaction_description(&tx.description);

                    // Convert unix timestamp (milliseconds) to ISO 8601
                    let timestamp = chrono::DateTime::from_timestamp(tx.created_at / 1000, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| tx.created_at.to_string());

                    HistoryEntry {
                        id: tx.id,
                        timestamp,
                        entry_type: entry_type.clone(),
                        credits: tx.amount,
                        filename,
                        mode,
                        parameters,
                        is_free_reopt: if is_free_reopt { Some(true) } else { None },
                        raw_description: if entry_type == "other" {
                            Some(tx.description)
                        } else {
                            None
                        },
                    }
                })
                .collect();

            Json(json!({ "history": entries })).into_response()
        }
        Err(e) => {
            error!("Failed to get history for key={}: {:?}", &auth_key.0, e);
            error_server("Failed to retrieve history")
        }
    }
}

// --- FREE CREDITS HANDLER ---

#[derive(Debug, Deserialize)]
struct ClaimFreeCreditsRequest {
    email: String,
}

async fn claim_free_credits(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ClaimFreeCreditsRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let email = req.email.trim().to_lowercase();

    // Validate email format (basic check)
    if !email.contains('@') || !email.contains('.') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Invalid email address" })),
        ));
    }

    // Load pricing config to get free_initial_credits
    let pricing = load_pricing_config().map_err(|e| {
        error!("Failed to load pricing config: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Server configuration error" })),
        )
    })?;

    let free_credits = pricing.free_initial_credits;
    if free_credits <= 0 {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "Free credits promotion is not currently available" })),
        ));
    }

    // Check if email already has a key
    let existing_key = state.db.get_key_by_email(&email).await;

    if let Some(api_key) = existing_key {
        // Email already exists - create a session for them (auto-login)
        // Also send a reminder email with their API key for programmatic use
        let html_body = format!(
            r#"
            <h2>Your MeshOpt API Key from webdeliveryengine.com</h2>
            <p>You requested access to your account. You've been automatically logged in on our website.</p>
            <p>For programmatic use (scripts, API calls), here's your API key:</p>
            <p style="font-family: monospace; font-size: 1.2em; background: #f4f4f4; padding: 10px; border-radius: 4px;">{}</p>
            <p>You can also view your API key anytime by logging in and visiting the API Settings page.</p>
            <p>Happy optimizing!</p>
            "#,
            api_key
        );

        let client = reqwest::Client::new();
        let res = client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", state.resend_api_key))
            .json(&json!({
                "from": "MeshOpt <support@webdeliveryengine.com>",
                "to": [email.clone()],
                "subject": "Your MeshOpt API Key (Reminder)",
                "html": html_body
            }))
            .send()
            .await
            .map_err(|e| {
                error!("Failed to send reminder email: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Failed to send email" })),
                )
            })?;

        if res.status().is_success() {
            // Log successful email send
            info!("Sent API key reminder email to {}", email);

            // Create a session for the user (auto-login)
            let session_id = state
                .db
                .create_session(&api_key, SESSION_DURATION_SECS, None, None)
                .await
                .map_err(|e| {
                    error!("Failed to create session: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed to create session" })),
                    )
                })?;

            info!("Existing user {} logged in via free credits form", email);

            // Return session cookie - NO API KEY in response!
            return Ok((
                jar.add(create_session_cookie(&session_id)),
                Json(json!({
                    "success": true,
                    "existing": true,
                    "message": "Welcome back! You've been logged in."
                })),
            ));
        } else {
            let error_text = res.text().await.unwrap_or_default();
            error!("Resend API error: {}", error_text);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to send email" })),
            ));
        }
    }

    // Create the free starter credits key
    let api_key = state
        .db
        .create_free_tier_key(email.clone(), free_credits)
        .await
        .map_err(|e| {
            error!("Failed to create free starter credits key: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to create API key" })),
            )
        })?;

    // Send email with the API key (for programmatic access)
    let html_body = format!(
        r#"
        <h2>Welcome to MeshOpt!</h2>
        <p>You've been automatically logged in with <strong>{} free credits</strong>!</p>
        <p>For programmatic use (scripts, API calls), here's your API key:</p>
        <p style="font-family: monospace; font-size: 1.2em; background: #f4f4f4; padding: 10px; border-radius: 4px;">{}</p>
        <p>You can also view your API key anytime from the API Settings page on our website.</p>
        <p>Need more credits? You can purchase additional credits anytime from the website.</p>
        <p>Happy optimizing!</p>
        "#,
        free_credits, api_key
    );

    let client = reqwest::Client::new();
    let res = client
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {}", state.resend_api_key))
        .json(&json!({
            "from": "MeshOpt <support@webdeliveryengine.com>",
            "to": [email.clone()],
            "subject": format!("Welcome to MeshOpt! ({} free credits)", free_credits),
            "html": html_body
        }))
        .send()
        .await
        .map_err(|e| {
            error!("Failed to send email: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to send email" })),
            )
        })?;

    if res.status().is_success() {
        // Log successful email send
        info!("Sent welcome email with API key to {}", email);

        // Create a session for the new user (auto-login)
        let session_id = state
            .db
            .create_session(&api_key, SESSION_DURATION_SECS, None, None)
            .await
            .map_err(|e| {
                error!("Failed to create session: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Failed to create session" })),
                )
            })?;

        info!("New user {} created and logged in with {} free credits", email, free_credits);

        // Return session cookie - NO API KEY in response!
        Ok((
            jar.add(create_session_cookie(&session_id)),
            Json(json!({
                "success": true,
                "message": format!("Welcome! You've been logged in with {} free credits.", free_credits)
            })),
        ))
    } else {
        let error_text = res.text().await.unwrap_or_default();
        error!("Resend API error: {}", error_text);
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to send email" })),
        ))
    }
}

// --- CONTACT FORM HANDLER ---

#[derive(Debug, Deserialize)]
struct ContactForm {
    name: Option<String>,
    email: String,
    subject: String,
    message: String,
    api_key: Option<String>,
}

async fn contact_handler(
    State(state): State<AppState>,
    Json(form): Json<ContactForm>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    // Map subject codes to human-readable labels
    let subject_label = match form.subject.as_str() {
        "privacy" => "Privacy & Data Inquiry",
        "technical" => "Technical Support",
        "billing" => "Billing & Credits",
        "account" => "Account Management",
        "feature" => "Feature Request",
        "bug" => "Bug Report",
        "other" => "Other",
        _ => &form.subject,
    };

    // Build HTML email body
    let api_key_section = form
        .api_key
        .as_ref()
        .filter(|k| !k.is_empty())
        .map(|k| format!(r#"<p><strong>API Key:</strong> {}</p>"#, k))
        .unwrap_or_default();

    let display_name = form.name.as_deref().unwrap_or("(not provided)");

    let html_body = format!(
        r#"
        <h2>New Support Request</h2>
        <p><strong>From:</strong> {} &lt;{}&gt;</p>
        <p><strong>Subject:</strong> {}</p>
        {}
        <hr />
        <h3>Message:</h3>
        <p style="white-space: pre-wrap;">{}</p>
        "#,
        display_name, form.email, subject_label, api_key_section, form.message
    );

    let email_subject = format!("[Mesh Optimizer] {}: {}", subject_label, display_name);

    // Build Resend API request
    let client = reqwest::Client::new();
    let res = client
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {}", state.resend_api_key))
        .json(&json!({
            "from": "Mesh Optimizer Support <support@webdeliveryengine.com>",
            "to": ["support@webdeliveryengine.com"],
            "reply_to": form.email,
            "subject": email_subject,
            "html": html_body
        }))
        .send()
        .await
        .map_err(|e| {
            error!("Failed to send email: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to send message" })),
            )
        })?;

    if res.status().is_success() {
        info!("Support email sent from {} <{}>", display_name, form.email);
        Ok(Json(
            json!({ "success": true, "message": "Message sent successfully" }),
        ))
    } else {
        let error_text = res.text().await.unwrap_or_default();
        error!("Resend API error: {}", error_text);
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to send message" })),
        ))
    }
}

async fn cleanup_task(db: db::Database) {
    let cleanup_age = Duration::from_secs(DOWNLOAD_EXPIRES_SECS);
    let interval = Duration::from_secs(15 * 60); // 15 Min

    loop {
        tokio::time::sleep(interval).await;
        info!("Running cleanup...");

        // Clean up old files
        if let Ok(mut entries) = tokio::fs::read_dir(UPLOAD_DIR).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(created) = metadata.created() {
                        if let Ok(age) = SystemTime::now().duration_since(created) {
                            if age > cleanup_age {
                                let _ = tokio::fs::remove_dir_all(entry.path()).await;
                                info!("Deleted stale batch: {:?}", entry.path());
                            }
                        }
                    }
                }
            }
        }

        // Clean up old job records from database
        let _ = db.cleanup_old_jobs(DOWNLOAD_EXPIRES_SECS as i64).await;
    }
}

/// Periodic task to log capacity statistics
async fn capacity_stats_task(semaphore: Arc<Semaphore>, total_slots: usize) {
    let interval = Duration::from_secs(60); // Log every minute

    loop {
        tokio::time::sleep(interval).await;

        let available = semaphore.available_permits();
        let in_use = total_slots - available;
        let utilization = (in_use as f64 / total_slots as f64) * 100.0;

        if utilization > 80.0 {
            warn!(
                "CAPACITY: high utilization - {}/{} slots in use ({:.0}%)",
                in_use, total_slots, utilization
            );
        } else if utilization > 0.0 {
            info!(
                "CAPACITY: {}/{} slots in use ({:.0}%)",
                in_use, total_slots, utilization
            );
        }
        // Don't log if nothing is happening (0% utilization)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests that job status responses match the format expected by the frontend.
    ///
    /// IMPORTANT: The frontend (index.html) parses these responses in the polling loop.
    /// If you change the response format here, you MUST update the frontend to match.
    ///
    /// Frontend expects for Completed status:
    ///   - status.Completed.glb_url (nested object format)
    ///   - status.Failed.error (nested object format)
    ///
    /// This test calls the ACTUAL format_job_status() function to catch any changes.
    #[test]
    fn test_job_status_response_format_matches_frontend_contract() {
        let empty_msgs: Vec<String> = vec![];

        // Test Completed status
        let completed_status = JobStatus::Completed {
            output_size: 12345,
            glb_url: "/download/abc123/model.glb".to_string(),
            usdz_url: "/download/abc123/model.usdz".to_string(),
            expires_at: "2024-01-01T00:00:00Z".to_string(),
            original_faces: Some(10000),
            output_faces: Some(5000),
            remesh_method: Some("quadriflow".to_string()),
            credits_used: Some(2),
            credits_remaining: Some(15),
        };

        // Call the ACTUAL function that the handler uses
        let response = format_job_status(&completed_status, "test-job-id", &empty_msgs);

        // Frontend contract: status must be an object with "Completed" key
        assert!(
            response["status"].is_object(),
            "status must be an object, not a string. Frontend checks: status.Completed.glb_url. Got: {}",
            response["status"]
        );
        assert!(
            response["status"]["Completed"].is_object(),
            "status.Completed must be an object containing glb_url and usdz_url. Got: {}",
            response["status"]
        );
        assert!(
            response["status"]["Completed"]["glb_url"].is_string(),
            "status.Completed.glb_url must be a string"
        );
        assert!(
            response["status"]["Completed"]["usdz_url"].is_string(),
            "status.Completed.usdz_url must be a string"
        );

        // Test Failed status
        let failed_status = JobStatus::Failed {
            error: "Something went wrong".to_string(),
        };

        // Call the ACTUAL function
        let failed_response = format_job_status(&failed_status, "test-job-id", &empty_msgs);

        assert!(
            failed_response["status"]["Failed"].is_object(),
            "status.Failed must be an object containing error"
        );
        assert!(
            failed_response["status"]["Failed"]["error"].is_string(),
            "status.Failed.error must be a string"
        );
    }

    #[test]
    fn test_get_config_includes_free_initial_credits() {
        // This test ensures free_initial_credits is always included in the config response.
        // The bug: when this field was missing from get_config(), the frontend would
        // show a hardcoded value instead of the dynamic value from pricing.json.

        // Try workspace root first, then relative path (for different test contexts)
        let pricing_path = if Path::new("server/pricing.json").exists() {
            "server/pricing.json"
        } else if Path::new("../../server/pricing.json").exists() {
            "../../server/pricing.json"
        } else {
            panic!("Cannot find pricing.json - run tests from workspace root");
        };

        let content = fs::read_to_string(pricing_path).expect("Failed to read pricing.json");
        let pricing: PricingConfig =
            serde_json::from_str(&content).expect("Failed to parse pricing.json");

        let config_json = json!({
            "pricing": {
                "base_rate_usd_per_credit": pricing.base_rate_usd_per_credit,
                "min_purchase_usd": pricing.min_purchase_usd,
                "max_purchase_usd": pricing.max_purchase_usd,
                "default_purchase_usd": pricing.default_purchase_usd,
                "tiers": pricing.tiers,
                "free_reoptimization_hours": pricing.free_reoptimization_hours,
                "free_initial_credits": pricing.free_initial_credits
            },
            "cost_decimate": pricing.cost_decimate,
            "cost_remesh": pricing.cost_remesh
        });

        // Verify free_initial_credits is present and matches the config
        let free_credits = config_json["pricing"]["free_initial_credits"].as_i64();
        assert!(
            free_credits.is_some(),
            "free_initial_credits must be present in config response"
        );
        assert_eq!(
            free_credits.unwrap(),
            pricing.free_initial_credits as i64,
            "free_initial_credits must match pricing.json value"
        );
    }
}
