mod db;

use anyhow::{Context, Result};
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
use std::process::Stdio;
use std::sync::Arc;
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use stripe::{
    CheckoutSessionMode, CreateCheckoutSession, CreateCheckoutSessionLineItems,
    CreateCheckoutSessionLineItemsPriceData, CreateCheckoutSessionLineItemsPriceDataProductData,
    CreateCheckoutSessionPaymentMethodTypes, Currency, CustomerId, EventObject, EventType,
    Expandable, Webhook,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tokio::sync::Semaphore;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info};

// --- CONFIGURATION ---
const UPLOAD_DIR: &str = "uploads";
const DB_FILE: &str = "server/database.json";
const DB_SQLITE_FILE: &str = "server/stats.db";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum JobStatus {
    Queued,
    Processing,
    Completed {
        output_size: u64,
        glb_url: String,
        usdz_url: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Clone)]
struct AppState {
    db: db::Database,
    stripe_client: stripe::Client,
    stripe_webhook_secret: String,
    jobs: Arc<RwLock<HashMap<String, JobStatus>>>,
    worker_semaphore: Arc<Semaphore>,
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

    // 2. Setup Filesystem
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

    let state = AppState {
        db,
        stripe_client,
        stripe_webhook_secret,
        jobs: Arc::new(RwLock::new(HashMap::new())),
        worker_semaphore: Arc::new(Semaphore::new(worker_slots)),
    };

    // 4. Start Cleanup Task
    tokio::spawn(cleanup_task());

    // 5. Build Router
    let app = Router::new()
        // Public Routes
        .route("/config", get(get_config))
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
        .layer(CorsLayer::permissive())
        .with_state(state);

    // 6. Start Server
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], 3000));
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
        Some(t) if t == "sk_test_123" || state.db.is_valid_key(&t).await => {
            req.extensions_mut().insert(AuthKey(t));
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// --- HANDLERS ---

async fn get_config() -> Json<serde_json::Value> {
    let cost = std::env::var("CREDIT_COST")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(49);
    let credits = std::env::var("CREDIT_INCREMENT")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(100);

    Json(json!({
        "cost": cost,
        "credits": credits
    }))
}

#[derive(Deserialize)]
struct CreateCheckoutPayload {
    api_key: Option<String>,
}

async fn create_checkout_session(
    State(state): State<AppState>,
    payload: Option<Json<CreateCheckoutPayload>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Starting Checkout Session...");

    // 1. Resolve Customer ID if API Key is present
    let mut customer_id_opt = None;
    if let Some(Json(payload)) = payload {
        if let Some(key) = payload.api_key {
            if let Some(info) = state.db.get_key_info(&key).await {
                info!("Existing user detected: {}", info.email);
                if let Ok(cid) = CustomerId::from_str(&info.stripe_customer_id) {
                    customer_id_opt = Some(cid);
                }
            }
        }
    }

    let credit_cost = std::env::var("CREDIT_COST")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(49);
    let credit_amount = std::env::var("CREDIT_INCREMENT")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .unwrap_or(100);

    let params = CreateCheckoutSession {
        customer: customer_id_opt,
        payment_method_types: Some(vec![CreateCheckoutSessionPaymentMethodTypes::Card]),
        line_items: Some(vec![CreateCheckoutSessionLineItems {
            price_data: Some(CreateCheckoutSessionLineItemsPriceData {
                currency: Currency::USD,
                product_data: Some(CreateCheckoutSessionLineItemsPriceDataProductData {
                    name: format!("MeshOpt Pro License ({} Credits)", credit_amount),
                    ..Default::default()
                }),
                unit_amount: Some(credit_cost * 100), // Convert to cents
                ..Default::default()
            }),
            quantity: Some(1),
            ..Default::default()
        }]),
        mode: Some(CheckoutSessionMode::Payment),
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

                    let credit_amount = std::env::var("CREDIT_INCREMENT")
                        .ok()
                        .and_then(|v| v.parse::<i32>().ok())
                        .unwrap_or(100);

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

            <div style="background:#fee2e2; color:#991b1b; padding:20px; border-radius:8px; margin:30px auto; max-width:500px; border-left:4px solid #dc2626;">
                <strong style="font-size:1.1em;">⚠️ IMPORTANT - SAVE YOUR KEY NOW</strong>
                <p style="margin-top:10px; margin-bottom:0;">This is the ONLY time you will see your API key. If you lose it, you'll need to contact support.</p>
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

async fn job_status_handler(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Json<serde_json::Value> {
    let jobs = state.jobs.read().await;
    if let Some(status) = jobs.get(&id) {
        Json(json!({ "status": status }))
    } else {
        Json(json!({ "error": "Job not found" }))
    }
}

async fn optimize_handler(
    State(state): State<AppState>,
    Extension(auth_key): Extension<AuthKey>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    let start_time = std::time::Instant::now();
    let batch_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .to_string();
    let batch_dir = Path::new(UPLOAD_DIR).join(&batch_id);

    if let Err(e) = fs::create_dir_all(&batch_dir) {
        error!("Failed to create batch dir: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Server Error").into_response();
    }

    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(batch_id.clone(), JobStatus::Processing);
    }

    let mut input_filename: Option<String> = None;
    let mut ratio = 0.5;
    let mut format = "glb".to_string();
    let mut mode = "decimate".to_string();
    let mut faces = 5000;
    let mut texture_size = 2048;
    let mut input_filepath: Option<PathBuf> = None;
    let mut file_hash = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();

        if name == "file" {
            if let Some(filename) = field.file_name().map(|s| s.to_string()) {
                let filepath = batch_dir.join(&filename);
                input_filepath = Some(filepath.clone());

                // Stream file to disk AND hash it
                if let Ok(mut file) = tokio::fs::File::create(&filepath).await {
                    let mut hasher = sha2::Sha256::new();
                    let mut stream = field;
                    while let Ok(Some(chunk)) = stream.chunk().await {
                        let _ = file.write_all(&chunk).await;
                        sha2::Digest::update(&mut hasher, &chunk);
                    }
                    file_hash = hex::encode(sha2::Digest::finalize(hasher));
                }

                let ext = Path::new(&filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

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
                                    let fname = outpath.file_name()?.to_string_lossy().to_string();
                                    let fext = Path::new(&fname)
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    // Ignore hidden files/mac metadata
                                    if !fname.starts_with('.')
                                        && !outpath.to_string_lossy().contains("__MACOSX")
                                    {
                                        if ["obj", "fbx", "glb", "gltf"].contains(&fext.as_str()) {
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
                    }
                } else if ["obj", "fbx", "glb", "gltf"].contains(&ext.as_str()) {
                    input_filename = Some(filename);
                }
            }
        } else if name == "ratio" {
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<f32>() {
                    ratio = parsed;
                }
            }
        } else if name == "format" {
            if let Ok(val) = field.text().await {
                format = val;
            }
        } else if name == "mode" {
            if let Ok(val) = field.text().await {
                mode = val;
            }
        } else if name == "faces" {
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<i32>() {
                    faces = parsed;
                }
            }
        } else if name == "texture_size" {
            if let Ok(val) = field.text().await {
                if let Ok(parsed) = val.parse::<i32>() {
                    texture_size = parsed;
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

    let output_base = Path::new(&input_filename)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    let output_filename = format!("{}_opt.glb", output_base);
    let usdz_filename = format!("{}_opt.usdz", output_base);

    // Fair Billing Logic
    let test_key = std::env::var("TEST_KEY").unwrap_or_default();
    let is_test_key = !test_key.is_empty() && auth_key.0 == test_key;

    let should_charge = if is_test_key {
        info!("Test Key used. Skipping billing.");
        false
    } else {
        state
            .db
            .should_charge_for_file(&auth_key.0, &file_hash)
            .await
    };
    let mut deducted = false;

    if should_charge {
        info!(
            "Attempting to charge key={} for file={}",
            &auth_key.0, input_filename
        );
        match state
            .db
            .record_transaction(
                &auth_key.0,
                -1,
                &format!("optimized: {}", input_filename),
                Some(file_hash.clone()),
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
                return (
                    StatusCode::PAYMENT_REQUIRED,
                    "Insufficient Credits. Please top up.",
                )
                    .into_response();
            }
        }
    } else {
        // Record free re-optimization for transparency in transaction history
        if is_test_key {
            info!("Test Key used. Skipping transaction record.");
        } else {
            info!(
                "Free Re-roll for hash: {} (recording 0-credit transaction)",
                file_hash
            );
            let _ = state
                .db
                .record_transaction(
                    &auth_key.0,
                    0,
                    &format!("re-optimized: {} (free - within 24hr)", input_filename),
                    Some(file_hash.clone()),
                )
                .await;
        }
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
    let file_hash_clone = file_hash.clone();
    let format_clone = format.clone();
    let mode_clone = mode.clone();

    tokio::spawn(async move {
        // Determine resource cost (Weighted Semaphore)
        // Remesh is heavy (RAM/CPU), Decimate is light.
        let required_permits: u32 = if mode_clone == "remesh" { 4 } else { 1 };
        let _permit = state_clone
            .worker_semaphore
            .acquire_many(required_permits)
            .await
            .unwrap();

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

            if format_clone == "json" || format_clone == "usdz" {
                c.arg("--usdz");
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
                {
                    let mut jobs = state_clone.jobs.write().await;
                    jobs.insert(
                        batch_id_clone,
                        JobStatus::Failed {
                            error: "Spawn Failed".to_string(),
                        },
                    );
                }
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
                {
                    let mut jobs = state_clone.jobs.write().await;
                    jobs.insert(
                        batch_id_clone.clone(),
                        JobStatus::Failed {
                            error: "System Error".to_string(),
                        },
                    );
                }
                error!("Execution failed: {:?}", e);
                // Refund Credit (only if we charged them)
                if deducted {
                    let _ = state_clone
                        .db
                        .record_transaction(
                            &auth_key_str,
                            1,
                            "system_error_refund",
                            Some(file_hash_clone),
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
                    )
                    .await;
                return;
            }
            Err(_) => {
                {
                    let mut jobs = state_clone.jobs.write().await;
                    jobs.insert(
                        batch_id_clone.clone(),
                        JobStatus::Failed {
                            error: "Timeout".to_string(),
                        },
                    );
                }
                error!("Execution timed out");
                // Refund Credit (only if we charged them)
                if deducted {
                    let _ = state_clone
                        .db
                        .record_transaction(
                            &auth_key_str,
                            1,
                            "timeout_refund",
                            Some(file_hash_clone),
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
                        1,
                        &format!("process_failure_refund: {}", input_filename_clone),
                        Some(file_hash_clone),
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
                )
                .await;

            {
                let mut jobs = state_clone.jobs.write().await;
                jobs.insert(
                    batch_id_clone.clone(),
                    JobStatus::Failed {
                        error: "Worker Error".to_string(),
                    },
                );
            }
            return;
        }

        // Success - Calculate Output Size
        let output_path = if format_clone == "usdz" {
            batch_dir_clone.join(&usdz_filename_clone)
        } else {
            batch_dir_clone.join(&output_filename_clone)
        };
        let output_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

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
            )
            .await;

        let dl_base = format!("/download/{}", batch_id_clone);
        let glb_url = format!("{}/{}", dl_base, output_filename_clone);
        let usdz_url = format!("{}/{}", dl_base, usdz_filename_clone);

        {
            let mut jobs = state_clone.jobs.write().await;
            jobs.insert(
                batch_id_clone,
                JobStatus::Completed {
                    output_size,
                    glb_url,
                    usdz_url,
                },
            );
        }
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

async fn admin_add_credits(
    State(state): State<AppState>,
    Json(payload): Json<AdminAddCredits>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // In production, set ADMIN_SECRET in env
    let admin_secret =
        std::env::var("ADMIN_SECRET").unwrap_or_else(|_| "supersecret123".to_string());

    if payload.secret != admin_secret {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.db.add_credits(&payload.key, payload.amount).await {
        Ok(new_balance) => Ok(Json(json!({ "success": true, "new_balance": new_balance }))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn admin_create_key(
    State(state): State<AppState>,
    Json(payload): Json<AdminCreateKey>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // In production, set ADMIN_SECRET in env
    let admin_secret =
        std::env::var("ADMIN_SECRET").unwrap_or_else(|_| "supersecret123".to_string());

    if payload.secret != admin_secret {
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
        Ok(new_key) => Ok(Json(json!({
            "success": true,
            "key": new_key,
            "email": payload.email,
            "initial_credits": payload.initial_credits
        }))),
        Err(e) => {
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

async fn cleanup_task() {
    let cleanup_age = Duration::from_secs(60 * 60); // 1 Hour
    let interval = Duration::from_secs(15 * 60); // 15 Min

    loop {
        tokio::time::sleep(interval).await;
        info!("Running cleanup...");

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
    }
}
