mod db;

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

    // 4b. Start Capacity Stats Task
    let semaphore_for_stats = state.worker_semaphore.clone();
    tokio::spawn(capacity_stats_task(semaphore_for_stats, worker_slots));

    // 5. Build Router
    let app = Router::new()
        // Public Routes
        .route("/config", get(get_config))
        .route("/contact", post(contact_handler))
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
        // Admin Routes
        .route("/admin/add-credits", post(admin_add_credits))
        .route("/admin/create-key", post(admin_create_key))
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

async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .or_else(|| {
            req.uri().query().and_then(|q| {
                url::form_urlencoded::parse(q.as_bytes())
                    .find(|(k, _)| k == "key")
                    .map(|(_, v)| v.to_string())
            })
        });

    match token {
        Some(t) if state.db.is_valid_key(&t).await => {
            req.extensions_mut().insert(AuthKey(t));
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// --- HANDLERS ---

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
            "free_reoptimization_hours": pricing.free_reoptimization_hours
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
                            .add_credits(&existing_key, credit_amount)
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
        </body></html>
    "#,
        key
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

    // Helper to format status response, with easter egg for Processing
    let format_status = |status: &JobStatus, job_id: &str, msgs: &[String]| -> serde_json::Value {
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
            } => {
                let base = "https://webdeliveryengine.com";
                let full_glb = format!("{}{}", base, glb_url);
                let full_usdz = format!("{}{}", base, usdz_url);
                let glb_filename = glb_url.split('/').last().unwrap_or("model.glb");
                let usdz_filename = usdz_url.split('/').last().unwrap_or("model.usdz");
                json!({
                    "status": "Completed",
                    "output_size": output_size,
                    "glb_url": full_glb,
                    "usdz_url": full_usdz,
                    "expires_at": expires_at,
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
            _ => json!({ "status": status }),
        }
    };

    // Check in-memory cache first
    {
        let jobs = state.jobs.read().await;
        if let Some(status) = jobs.get(&id) {
            return Json(format_status(status, &id, messages));
        }
    }

    // Fall back to database
    if let Some(status) = state.db.get_job(&id).await {
        // Cache it in memory for future lookups
        {
            let mut jobs = state.jobs.write().await;
            jobs.insert(id.clone(), status.clone());
        }
        return Json(format_status(&status, &id, messages));
    }

    Json(json!({ "error": "Job not found" }))
}

async fn optimize_handler(
    State(state): State<AppState>,
    Extension(auth_key): Extension<AuthKey>,
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
        return (StatusCode::INTERNAL_SERVER_ERROR, "Server Error").into_response();
    }

    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(batch_id.clone(), JobStatus::Processing);
    }
    // Persist to database
    let _ = state.db.save_job(&batch_id, &JobStatus::Processing).await;

    let mut input_filename: Option<String> = None;
    let mut ratio = 0.5;
    let mut format = "glb".to_string();
    let mut mode = "decimate".to_string();
    let mut faces = 5000;
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
                    return (
                        StatusCode::BAD_REQUEST,
                        format!(
                            "Invalid file type. Allowed: {}",
                            ALLOWED_EXTENSIONS.join(", ")
                        ),
                    )
                        .into_response();
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
            return (StatusCode::BAD_REQUEST, "No supported 3D model found").into_response();
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
                        return (
                            StatusCode::BAD_REQUEST,
                            format!(
                                "This GLTF file references external buffers that were not uploaded: {}. \
                                Please either: (1) Upload a ZIP containing the .gltf and all .bin files, \
                                or (2) Convert to GLB format which embeds all data in a single file.",
                                missing_buffers.join(", ")
                            ),
                        )
                            .into_response();
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
            return (
                StatusCode::PAYMENT_REQUIRED,
                format!(
                    "Insufficient Credits. Need {}, Have {}",
                    required_credits, current_balance
                ),
            )
                .into_response();
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
                        "{}{}; Rem; {}k faces; {}px",
                        input_filename,
                        if was_zip { " (zip)" } else { "" },
                        faces / 1000,
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
                return (StatusCode::PAYMENT_REQUIRED, "Transaction Failed.").into_response();
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
                        "{}{}; Rem; {}k faces; {}px (free re-opt)",
                        input_filename,
                        if was_zip { " (zip)" } else { "" },
                        faces / 1000,
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

    // Slot Cost Logic
    let slot_cost_decimate = std::env::var("SLOT_COST_DECIMATE")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1);
    let slot_cost_remesh = std::env::var("SLOT_COST_REMESH")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(5);

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
                .arg(&output_filename_clone)
                .arg("--ratio")
                .arg(ratio.to_string());

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
                return;
            }
        };

        // Stream stdout
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    info!("WORKER: {}", line);
                }
            });
        }

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

        let expires_at =
            chrono::Utc::now() + chrono::Duration::seconds(DOWNLOAD_EXPIRES_SECS as i64);
        let completed_status = JobStatus::Completed {
            output_size,
            glb_url,
            usdz_url,
            expires_at: expires_at.to_rfc3339(),
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
    });

    let mut response = Json(json!({
        "jobId": batch_id,
        "status": "processing"
    }))
    .into_response();

    response.headers_mut().insert(
        "X-Credits-Remaining",
        credits_remaining.to_string().parse().unwrap(),
    );

    response
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
            Ok(Json(json!({
                "success": true,
                "key": new_key,
                "email": payload.email,
                "initial_credits": payload.initial_credits
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

async fn credits_handler(
    State(state): State<AppState>,
    Extension(auth_key): Extension<AuthKey>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Credits request for key={}", &auth_key.0);
    match state.db.get_credits(&auth_key.0).await {
        Some(credits) => {
            info!("Retrieved credits for key={}: {}", &auth_key.0, credits);
            Ok(Json(json!({ "credits": credits })))
        }
        None => {
            error!("Key not found: {}", &auth_key.0);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

async fn history_handler(
    State(state): State<AppState>,
    Extension(auth_key): Extension<AuthKey>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("History request for key={}", &auth_key.0);
    match state.db.get_history(&auth_key.0, 50).await {
        Ok(transactions) => {
            info!(
                "Successfully retrieved {} transactions for key={}",
                transactions.len(),
                &auth_key.0
            );
            Ok(Json(serde_json::to_value(transactions).unwrap()))
        }
        Err(e) => {
            error!("Failed to get history for key={}: {:?}", &auth_key.0, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
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
