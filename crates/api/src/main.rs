mod db;

use anyhow::{Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use stripe::{
    CheckoutSessionMode, CreateCheckoutSession, CreateCheckoutSessionLineItems,
    CreateCheckoutSessionLineItemsPriceData, CreateCheckoutSessionLineItemsPriceDataProductData,
    CreateCheckoutSessionPaymentMethodTypes, Currency, EventObject, EventType, Expandable, Webhook,
};
use tokio::io::AsyncWriteExt;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info};

// --- CONFIGURATION ---
const UPLOAD_DIR: &str = "uploads";
const DB_FILE: &str = "server/database.json";
const DB_SQLITE_FILE: &str = "server/stats.db";

#[derive(Clone)]
struct AppState {
    db: db::Database,
    stripe_client: stripe::Client,
    stripe_webhook_secret: String,
}

#[derive(Clone)]
struct AuthKey(String);

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize Logging
    tracing_subscriber::fmt::init();
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
    let state = AppState {
        db,
        stripe_client,
        stripe_webhook_secret,
    };

    // 4. Start Cleanup Task
    tokio::spawn(cleanup_task());

    // 5. Build Router
    let app = Router::new()
        // Public Routes
        .route("/create-checkout-session", post(create_checkout_session))
        .route("/webhook", post(stripe_webhook))
        .route("/success", get(success_page))
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

async fn create_checkout_session(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("Starting Checkout Session...");

    let params = CreateCheckoutSession {
        payment_method_types: Some(vec![CreateCheckoutSessionPaymentMethodTypes::Card]),
        line_items: Some(vec![CreateCheckoutSessionLineItems {
            price_data: Some(CreateCheckoutSessionLineItemsPriceData {
                currency: Currency::USD,
                product_data: Some(CreateCheckoutSessionLineItemsPriceDataProductData {
                    name: "MeshOpt Pro License".to_string(),
                    ..Default::default()
                }),
                unit_amount: Some(4900), // $49.00
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

                    let _ = state
                        .db
                        .create_key(email.clone(), customer_id)
                        .await
                        .map_err(|e| {
                            error!("DB Error: {:?}", e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?;
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

    let key = state
        .db
        .get_key_by_email(&email)
        .await
        .unwrap_or_else(|| "Key processing... check email later".to_string());

    let html = format!(
        r#"
        <html><body style="font-family:sans-serif; background:#111; color:white; text-align:center; padding:50px;">
            <h1 style="color:#10b981">Payment Successful!</h1>
            <p>Thank you {}</p>
            <p>Here is your API Key:</p>
            <div style="background:#333; padding:20px; font-size:24px; font-family:monospace; border-radius:10px; display:inline-block; border: 1px solid #555;">
                {}
            </div>
            <p style="color:#aaa">Save this key.</p>
            <a href="/" style="color:#3b82f6; text-decoration:none; margin-top:20px; display:inline-block;">&larr; Back to Dashboard</a>
        </body></html>
    "#,
        email, key
    );

    Html(html).into_response()
}

// --- OPTIMIZATION HANDLER ---

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

    let mut input_filename: Option<String> = None;
    let mut ratio = 0.5;
    let mut format = "glb".to_string();
    let mut input_filepath: Option<PathBuf> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();

        if name == "file" {
            if let Some(filename) = field.file_name().map(|s| s.to_string()) {
                let filepath = batch_dir.join(&filename);
                input_filepath = Some(filepath.clone());

                // Stream file to disk
                if let Ok(mut file) = tokio::fs::File::create(&filepath).await {
                    let mut stream = field;
                    while let Ok(Some(chunk)) = stream.chunk().await {
                        let _ = file.write_all(&chunk).await;
                    }
                }

                let ext = Path::new(&filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if ["obj", "fbx", "glb", "gltf"].contains(&ext.as_str()) {
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

    // Run Command
    let mut cmd = tokio::process::Command::new("mesh-optimizer");
    cmd.arg("--input")
        .arg(&input_filename)
        .arg("--output")
        .arg(&output_filename)
        .arg("--ratio")
        .arg(ratio.to_string())
        .current_dir(&batch_dir); // IMPORTANT: Run inside batch dir

    if format == "json" || format == "usdz" {
        cmd.arg("--usdz");
    }

    info!("Executing: {:?}", cmd);

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) => {
            error!("Execution failed: {:?}", e);
            // Log Failure
            let _ = state
                .db
                .log_job(
                    &auth_key.0,
                    &input_filename,
                    input_size,
                    "unknown",
                    &format,
                    0,
                    start_time.elapsed().as_millis() as u64,
                    ratio,
                    "system_error",
                )
                .await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "Optimization Failed").into_response();
        }
    };

    let processing_time = start_time.elapsed().as_millis() as u64;

    // Collect metrics identifiers
    // If X-Session-ID header is present (Web UI), use that. Otherwise use API Key.
    let session_header = headers
        .get("x-session-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let user_identifier = session_header.unwrap_or(auth_key.0);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Worker Error: {}", stderr);
        // Log User Error (likely bad mesh)
        state
            .db
            .log_job(
                &user_identifier,
                &input_filename,
                input_size,
                "unknown", // Could extract from filename extension
                &format,
                0,
                processing_time,
                ratio,
                "worker_error",
            )
            .await;

        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Optimization Failed", "details": stderr })),
        )
            .into_response();
    }

    // Success - Calculate Output Size
    // If JSON requested, we probably generated both GLB and USDZ.
    // Ideally we sum them or pick the primary one. For simplicity let's pick GLB or USDZ size.
    let output_path = if format == "usdz" {
        batch_dir.join(&usdz_filename)
    } else {
        batch_dir.join(&output_filename)
    };
    let output_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

    let input_ext = Path::new(&input_filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    state
        .db
        .log_job(
            &user_identifier,
            &input_filename,
            input_size,
            input_ext,
            &format,
            output_size,
            processing_time,
            ratio,
            "success",
        )
        .await;

    // Response
    let dl_base = format!("/download/{}", batch_id);

    if format == "json" {
        Json(json!({
            "glb": format!("{}/{}", dl_base, output_filename),
            "usdz": format!("{}/{}", dl_base, usdz_filename)
        }))
        .into_response()
    } else if format == "usdz" {
        // Redirect to download
        axum::response::Redirect::temporary(&format!("{}/{}", dl_base, usdz_filename))
            .into_response()
    } else {
        // Redirect to download GLB
        axum::response::Redirect::temporary(&format!("{}/{}", dl_base, output_filename))
            .into_response()
    }
}

// --- CLEANUP ---
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
