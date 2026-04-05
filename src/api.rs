mod config;
mod db;
mod jwt;
mod qr;
mod utils;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use chrono::Utc;
use config::Config;
use db::Database;
use jwt::Claims;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpListener;
use utils::{extract_album_id, parse_ttl};
use uuid::Uuid;

struct AppState {
    config: Config,
    db: Database,
    secret: String,
}

#[derive(Deserialize)]
struct GenerateRequest {
    album_id: String,
    #[serde(default)]
    ttl: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Serialize)]
struct GenerateResponse {
    id: i64,
    url: String,
    qr_code_png_base64: String,
    album_id: String,
    expires_at: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct ListResponse {
    links: Vec<LinkInfo>,
}

#[derive(Serialize)]
struct LinkInfo {
    id: i64,
    label: Option<String>,
    album_id: String,
    url: String,
    expires_at: Option<String>,
    status: String,
}

#[derive(Deserialize)]
struct RevokeRequest {
    id: i64,
}

#[derive(Serialize)]
struct RevokeResponse {
    success: bool,
    message: String,
}

#[derive(Deserialize)]
struct ExtendRequest {
    id: i64,
    ttl: String,
}

#[derive(Serialize)]
struct ExtendResponse {
    id: i64,
    url: String,
    qr_code_png_base64: String,
    expires_at: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let db = Database::new(&config.db_path())?;
    let secret = std::env::var("IMSHARE_SECRET")
        .context("IMSHARE_SECRET environment variable not set")?;

    let state = Arc::new(AppState { config, db, secret });

    let app = Router::new()
        .route("/imshare-api/generate", post(handle_generate))
        .route("/imshare-api/list", get(handle_list))
        .route("/imshare-api/revoke", post(handle_revoke))
        .route("/imshare-api/extend", post(handle_extend))
        .route("/imshare-api/health", get(health_check))
        .with_state(state);

    let port = std::env::var("IMSHARE_API_PORT")
        .unwrap_or_else(|_| "3002".to_string())
        .parse::<u16>()?;

    let addr = format!("0.0.0.0:{}", port);
    println!("imshare-api listening on {}", addr);

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn handle_generate(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GenerateRequest>,
) -> Response {
    match generate_link(state, req).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn generate_link(
    state: Arc<AppState>,
    req: GenerateRequest,
) -> Result<GenerateResponse> {
    // Extract album ID (supports both UUIDs and full URLs)
    let album_id = extract_album_id(&req.album_id)?;

    // Parse TTL
    let ttl_str = req.ttl.as_deref().unwrap_or(&state.config.default_ttl);
    let duration = parse_ttl(ttl_str)?;

    // Calculate expiration
    let expires_at = duration.map(|d| Utc::now() + d);
    let exp_timestamp = expires_at.map(|dt| dt.timestamp());

    // Generate JWT
    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        album_id: album_id.clone(),
        exp: exp_timestamp,
        jti: jti.clone(),
    };

    let token = jwt::sign_jwt(&claims, &state.secret)?;

    // Build URL
    let url = format!(
        "https://{}/share/{}?token={}",
        state.config.public_domain, album_id, token
    );

    // Generate QR code as PNG
    let qr_png = qr::generate_qr_code_png(&url)?;
    let qr_code_png_base64 = base64::prelude::BASE64_STANDARD.encode(&qr_png);

    // Store in database
    let id = state.db.insert_link(&album_id, req.label.as_deref(), &url, &jti, expires_at)?;

    Ok(GenerateResponse {
        id,
        url,
        qr_code_png_base64,
        album_id,
        expires_at: expires_at.map(|dt| dt.to_rfc3339()),
    })
}

async fn handle_list(State(state): State<Arc<AppState>>) -> Response {
    match list_links(state).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn list_links(state: Arc<AppState>) -> Result<ListResponse> {
    let links = state.db.list_links()?;

    let link_infos: Vec<LinkInfo> = links
        .into_iter()
        .map(|link| LinkInfo {
            id: link.id,
            label: link.label,
            album_id: link.album_id.clone(),
            url: link.url.clone(),
            expires_at: link.expires_at.map(|dt| dt.to_rfc3339()),
            status: utils::get_status(link.expires_at, link.revoked_at).into(),
        })
        .collect();

    Ok(ListResponse { links: link_infos })
}

async fn handle_revoke(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RevokeRequest>,
) -> Response {
    match revoke_link(state, req).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn revoke_link(
    state: Arc<AppState>,
    req: RevokeRequest,
) -> Result<RevokeResponse> {
    let success = state.db.revoke_link(req.id)?;

    if success {
        Ok(RevokeResponse {
            success: true,
            message: format!("Link {} revoked successfully", req.id),
        })
    } else {
        Ok(RevokeResponse {
            success: false,
            message: format!("Link {} not found or already revoked", req.id),
        })
    }
}

async fn handle_extend(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ExtendRequest>,
) -> Response {
    match extend_link(state, req).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

async fn extend_link(
    state: Arc<AppState>,
    req: ExtendRequest,
) -> Result<ExtendResponse> {
    // Get existing link
    let link = state
        .db
        .get_link_by_id(req.id)?
        .context(format!("Link {} not found", req.id))?;

    // Parse new TTL
    let duration = parse_ttl(&req.ttl)?;

    // Calculate new expiration
    let expires_at = duration.map(|d| Utc::now() + d);
    let exp_timestamp = expires_at.map(|dt| dt.timestamp());

    // Generate new JWT with new JTI
    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        album_id: link.album_id.clone(),
        exp: exp_timestamp,
        jti: jti.clone(),
    };

    let token = jwt::sign_jwt(&claims, &state.secret)?;

    // Build new URL
    let url = format!(
        "https://{}/share/{}?token={}",
        state.config.public_domain, link.album_id, token
    );

    // Generate new QR code
    let qr_png = qr::generate_qr_code_png(&url)?;
    let qr_code_png_base64 = base64::prelude::BASE64_STANDARD.encode(&qr_png);

    // Update database
    state.db.extend_link(req.id, expires_at, &jti, &url)?;

    Ok(ExtendResponse {
        id: req.id,
        url,
        qr_code_png_base64,
        expires_at: expires_at.map(|dt| dt.to_rfc3339()),
    })
}
