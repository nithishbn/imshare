mod config;
mod db;
mod jwt;

use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode, Uri},
    response::{IntoResponse, Redirect, Response},
    routing::{any, get},
    Router,
};
use chrono::Utc;
use config::Config;
use db::Database;
use sha2::{Digest, Sha512};
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_cookies::{Cookie, CookieManagerLayer, Cookies, Key};

struct AppState {
    config: Config,
    db: Database,
    secret: String,
    client: reqwest::Client,
    cookie_key: Key,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load()?;
    let db = Database::new(&config.db_path())?;
    let secret =
        std::env::var("IMSHARE_SECRET").context("IMSHARE_SECRET environment variable not set")?;

    // Validate secret strength
    if secret.len() < 32 {
        eprintln!("WARNING: IMSHARE_SECRET is too short (< 32 characters). This is insecure!");
        eprintln!("Generate a strong secret with: openssl rand -base64 32");
        anyhow::bail!("IMSHARE_SECRET must be at least 32 characters");
    }

    // Derive cookie signing key from secret (needs 64 bytes for signing)
    // Use SHA-512 to derive a 64-byte key from the secret
    let mut hasher = Sha512::new();
    hasher.update(secret.as_bytes());
    hasher.update(b"imshare-cookie-key"); // Domain separation
    let key_bytes = hasher.finalize();
    let cookie_key = Key::from(&key_bytes[..]);

    let state = Arc::new(AppState {
        config: config.clone(),
        db,
        secret,
        client: reqwest::Client::new(),
        cookie_key,
    });

    let app = Router::new()
        .route("/s/:code", get(handle_short_url))
        .route("/share/*path", any(handle_request))
        .layer(CookieManagerLayer::new())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", config.verify_port);
    println!("imshare-verify listening on {}", addr);
    println!("Proxying to: {}", config.upstream);
    println!("Session cookies enabled (24h TTL)");

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

const COOKIE_NAME: &str = "imshare_session";
const COOKIE_MAX_AGE: i64 = 86400; // 24 hours in seconds

fn extract_token_from_uri(uri: &Uri) -> Option<String> {
    uri.query().and_then(|q| {
        q.split('&').find_map(|param| {
            let mut parts = param.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some("token"), Some(value)) => Some(value.to_string()),
                _ => None,
            }
        })
    })
}

fn extract_album_id_from_path(path: &str) -> Option<String> {
    // Extract album ID from path
    // Handles both:
    // - /share/<album_id>
    // - /share/photo/<album_id>/...
    if let Some(stripped) = path.strip_prefix("/share/") {
        let parts: Vec<&str> = stripped.split('/').collect();

        // Skip "static" paths
        if parts.first() == Some(&"static") {
            return None;
        }

        // If first part is "photo", album ID is second part
        if parts.first() == Some(&"photo") && parts.len() >= 2 {
            let album_id = parts[1];
            if !album_id.is_empty() {
                return Some(album_id.to_string());
            }
        }

        // Otherwise, album ID is first part (direct /share/<album_id>)
        let album_id = parts.first()?;
        if !album_id.is_empty() {
            return Some(album_id.to_string());
        }
    }
    None
}

fn create_session_cookie(album_id: &str, jti: &str) -> Cookie<'static> {
    // Store both album_id and jti in cookie value, separated by ":"
    let cookie_value = format!("{}:{}", album_id, jti);
    Cookie::build((COOKIE_NAME, cookie_value))
        .path("/") // Changed from /share to / for broader scope
        .max_age(time::Duration::seconds(COOKIE_MAX_AGE))
        .http_only(true)
        .secure(false) // Set to false to work over HTTP (Tailscale, localhost)
        .same_site(tower_cookies::cookie::SameSite::Lax)
        .build()
}

fn validate_session_cookie(cookies: &Cookies, album_id: &str, key: &Key) -> Option<String> {
    println!("  Attempting to validate signed cookie '{}'", COOKIE_NAME);

    // Try to get the cookie without signature first to see if it exists
    if let Some(_unsigned_cookie) = cookies.get(COOKIE_NAME) {
        println!("  Cookie '{}' exists (checking signature...)", COOKIE_NAME);
    } else {
        println!("  No cookie '{}' found", COOKIE_NAME);
    }

    // Now try signed
    if let Some(cookie) = cookies.signed(key).get(COOKIE_NAME) {
        let cookie_value = cookie.value();

        // Parse "album_id:jti" format
        if let Some((cookie_album_id, jti)) = cookie_value.split_once(':') {
            let is_valid = cookie_album_id == album_id;
            println!(
                "  Signed cookie validated: album_match={}, valid={}",
                is_valid, is_valid
            );

            if is_valid {
                return Some(jti.to_string());
            }
        } else {
            println!("  Cookie value malformed (expected 'album_id:jti' format)");
        }
    } else {
        println!("  No valid signed cookie '{}' found", COOKIE_NAME);
    }

    None
}

async fn handle_short_url(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Response {
    // Look up the short code in the database
    match state.db.get_link_by_short_code(&code) {
        Ok(Some(link)) => {
            // Redirect to the full URL
            Redirect::temporary(&link.url).into_response()
        }
        Ok(None) => {
            // Short code not found
            (
                StatusCode::NOT_FOUND,
                "Short URL not found or has been removed",
            )
                .into_response()
        }
        Err(_) => {
            // Database error
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn handle_request(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    req: Request<Body>,
) -> Response {
    let uri = req.uri().clone();
    let path = uri.path();

    println!("\n=== Incoming Request ===");
    println!("Path: {}", path);
    // Note: Query string may contain tokens - not logging for security
    // Note: Cookie header may contain session tokens - not logging for security

    // Allow /share/static/* to bypass token validation
    if path.starts_with("/share/static/") {
        println!("Bypassing token validation for static resource: {}", path);
        match proxy_request(state, uri, req, false).await {
            Ok(response) => return response,
            Err(e) => {
                eprintln!("Proxy error: {}", e);
                return (StatusCode::BAD_GATEWAY, "Upstream error").into_response();
            }
        }
    }

    // Extract album ID from path
    let album_id = match extract_album_id_from_path(path) {
        Some(id) => {
            println!("Extracted album ID from path {}: {}", path, id);
            id
        }
        None => {
            eprintln!("Failed to extract album ID from path: {}", path);
            return (StatusCode::BAD_REQUEST, "Invalid share path").into_response();
        }
    };

    // Check for token in URL first (takes precedence over cookies)
    let token_from_url = extract_token_from_uri(&uri);

    if let Some(token) = token_from_url {
        // Token in URL - validate it (this allows refreshing with a new token)
        println!("Token found in URL, validating...");

        // Verify JWT signature and expiration
        let claims = match jwt::verify_jwt(&token, &state.secret) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("JWT verification failed: {}", e);
                return (StatusCode::UNAUTHORIZED, "Invalid or expired token").into_response();
            }
        };

        // Check revocation status in database
        let token_status = match state.db.check_token(&claims.jti) {
            Ok(Some(status)) => status,
            Ok(None) => {
                eprintln!("Token JTI not found in database: {}", claims.jti);
                return (StatusCode::UNAUTHORIZED, "Token not found").into_response();
            }
            Err(e) => {
                eprintln!("Database error: {}", e);
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Database unavailable - failing closed",
                )
                    .into_response();
            }
        };

        // Check if revoked
        if token_status.revoked_at.is_some() {
            return (StatusCode::UNAUTHORIZED, "Token has been revoked").into_response();
        }

        // Double-check expiration from database
        if let Some(expires_at) = token_status.expires_at {
            if Utc::now() > expires_at {
                return (StatusCode::UNAUTHORIZED, "Token expired").into_response();
            }
        }

        // Token is valid - set/update session cookie and proxy
        println!("✓ Token validated successfully");
        let session_cookie = create_session_cookie(&album_id, &claims.jti);
        println!("  Setting session cookie (24h TTL, http_only=true, secure=false)");

        cookies.signed(&state.cookie_key).add(session_cookie);
        println!("  Cookie added to response");

        match proxy_request(state, uri, req, true).await {
            Ok(response) => {
                println!("  Response proxied successfully");
                return response;
            }
            Err(e) => {
                eprintln!("Proxy error: {}", e);
                return (StatusCode::BAD_GATEWAY, "Upstream error").into_response();
            }
        }
    }

    // No token in URL - check for valid session cookie
    println!("No token in URL, checking for session cookie...");
    if let Some(jti) = validate_session_cookie(&cookies, &album_id, &state.cookie_key) {
        println!("✓ Valid session cookie found");

        // Even with a valid cookie, check if the token has been revoked
        println!("  Checking revocation status for cookie-based request...");
        match state.db.check_token(&jti) {
            Ok(Some(token_status)) => {
                if token_status.revoked_at.is_some() {
                    println!("  ✗ Token has been revoked - rejecting request");
                    return (StatusCode::UNAUTHORIZED, "Token has been revoked").into_response();
                }
                // Also check expiration
                if let Some(expires_at) = token_status.expires_at {
                    if Utc::now() > expires_at {
                        println!("  ✗ Token has expired - rejecting request");
                        return (StatusCode::UNAUTHORIZED, "Token expired").into_response();
                    }
                }
                println!("  ✓ Token is still valid - proxying request");
            }
            Ok(None) => {
                println!("  ✗ Token JTI not found in database - rejecting request");
                return (StatusCode::UNAUTHORIZED, "Token not found").into_response();
            }
            Err(e) => {
                eprintln!("  ✗ Database error: {}", e);
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Database unavailable - failing closed",
                )
                    .into_response();
            }
        }

        // Token is valid and not revoked - proxy the request
        match proxy_request(state, uri, req, false).await {
            Ok(response) => return response,
            Err(e) => {
                eprintln!("Proxy error: {}", e);
                return (StatusCode::BAD_GATEWAY, "Upstream error").into_response();
            }
        }
    }

    // No valid cookie or token
    println!("✗ No valid session cookie or token found");
    (StatusCode::UNAUTHORIZED, "Missing token or session cookie").into_response()
}

async fn proxy_request(
    state: Arc<AppState>,
    uri: Uri,
    req: Request<Body>,
    strip_token: bool,
) -> Result<Response> {
    // Build upstream URL, optionally removing the token parameter
    let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("");

    // Strip token from query string if requested
    let cleaned_path = if strip_token {
        if let Some(idx) = path_and_query.find('?') {
            let (path, query) = path_and_query.split_at(idx);
            let params: Vec<&str> = query[1..]
                .split('&')
                .filter(|p| !p.starts_with("token="))
                .collect();

            if params.is_empty() {
                path.to_string()
            } else {
                format!("{}?{}", path, params.join("&"))
            }
        } else {
            path_and_query.to_string()
        }
    } else {
        // For static resources, pass through as-is
        path_and_query.to_string()
    };

    let upstream_url = format!("{}{}", state.config.upstream, cleaned_path);

    // Forward the request
    let method = req.method().clone();
    let headers = req.headers().clone();
    let body = axum::body::to_bytes(req.into_body(), usize::MAX).await?;

    let upstream_resp = state
        .client
        .request(method, &upstream_url)
        .headers(headers)
        .body(body)
        .send()
        .await?;

    // Build response
    let status = upstream_resp.status();
    let headers = upstream_resp.headers().clone();
    let body = upstream_resp.bytes().await?;

    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    *response.headers_mut() = headers;

    Ok(response)
}
