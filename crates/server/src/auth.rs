//! Optional auth: Argon2 password hashing + HS256 JWT. Enabled only when
//! `JWT_SECRET` is set — otherwise the server runs open (desktop/local).

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::ApiError;
use crate::handlers::AppState;

const TOKEN_TTL_SECS: u64 = 7 * 24 * 3600;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id
    pub username: String,
    pub role: String,
    pub exp: usize,
}

pub fn hash_password(pw: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::internal(format!("hash failed: {e}")))
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(pw.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

pub fn issue_token(secret: &str, sub: &str, username: &str, role: &str) -> Result<String, ApiError> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let claims = Claims {
        sub: sub.to_string(),
        username: username.to_string(),
        role: role.to_string(),
        exp: (now + TOKEN_TTL_SECS) as usize,
    };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| ApiError::internal(format!("token issue failed: {e}")))
}

fn verify_token(secret: &str, token: &str) -> Option<Claims> {
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::default())
        .ok()
        .map(|d| d.claims)
}

/// Config-center paths (namespaces, service tokens, and consumer `resolve`).
/// Note `/config` (Skill Shelf's own settings) is deliberately excluded.
fn is_config_center(path: &str) -> bool {
    path.starts_with("/config/")
}

/// True for authoring mutations that require a login when auth is enabled.
/// Reads (GET), auth endpoints, routing/validation, and agent feedback submit
/// stay open.
fn needs_auth(method: &Method, path: &str) -> bool {
    // Config-center consumption is authenticated by a service token inside the
    // handler (X-Config-Token), not the admin JWT — leave it open here.
    if path == "/config/resolve" {
        return false;
    }
    // Everything else under /config (self settings, namespaces, clients) reveals
    // or changes settings → admin-only for both read and write.
    if path == "/config" || path.starts_with("/config/") {
        return true;
    }
    let open = path == "/user/signup"
        || path == "/user/signin"
        || path == "/route"
        || path == "/validate"
        || (method == Method::POST && path.ends_with("/feedback"));
    if open {
        return false;
    }
    matches!(*method, Method::POST | Method::PUT | Method::PATCH | Method::DELETE)
}

/// Middleware: when a secret is configured, require a valid Bearer JWT on
/// authoring mutations. No-op when auth is disabled.
pub async fn require_auth(
    State(st): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(secret) = st.auth_secret() else {
        // Auth disabled (desktop/local). The config CENTER is a networked,
        // multi-service surface — without an admin identity to protect it, an
        // attacker could self-issue a service token and read every namespace in
        // plaintext. Refuse it entirely when auth is off. Skill Shelf's own
        // settings at `/config` stay open (the desktop app needs them).
        if is_config_center(req.uri().path()) {
            return Err(StatusCode::FORBIDDEN);
        }
        return Ok(next.run(req).await);
    };
    if !needs_auth(req.method(), req.uri().path()) {
        return Ok(next.run(req).await);
    }
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    match token.and_then(|t| verify_token(&secret, t)) {
        // Authoring mutations are admin-only; a valid non-admin token is forbidden.
        Some(claims) if claims.role == skill_shelf_core::ROLE_ADMIN => Ok(next.run(req).await),
        Some(_) => Err(StatusCode::FORBIDDEN),
        None => Err(StatusCode::UNAUTHORIZED),
    }
}
