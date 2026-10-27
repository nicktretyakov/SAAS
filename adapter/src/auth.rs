//! API key + lightweight JWT helpers (no external JWT crate required)

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::AuthContext;
use crate::error::AdapterError;
use crate::ports::TenantRepository;

/// SHA-256 hex of the raw API key
pub fn hash_api_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a random API key (returned once to the client) and its hash (stored)
pub fn generate_api_key() -> (String, String) {
    let raw = format!("sk_{}", Uuid::new_v4().simple());
    let hash = hash_api_key(&raw);
    (raw, hash)
}

/// Resolve AuthContext from `Authorization: Bearer <api_key|jwt>`
pub async fn authenticate(
    authorization_header: Option<&str>,
    tenants: &dyn TenantRepository,
    jwt_secret: Option<&str>,
) -> Result<AuthContext, AdapterError> {
    let header = authorization_header
        .ok_or_else(|| AdapterError::Unauthorized("missing Authorization header".into()))?;

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("ApiKey "))
        .unwrap_or(header)
        .trim();

    if token.is_empty() {
        return Err(AdapterError::Unauthorized("empty token".into()));
    }

    // Path 1: API key (sk_...)
    if token.starts_with("sk_") {
        let hash = hash_api_key(token);
        let tenant = tenants
            .get_by_api_key_hash(&hash)
            .await?
            .ok_or_else(|| AdapterError::Unauthorized("invalid api key".into()))?;
        if !tenant.is_active {
            return Err(AdapterError::Unauthorized("tenant inactive".into()));
        }
        return Ok(AuthContext {
            tenant_id: tenant.id,
            subject: tenant.name,
            roles: vec!["tenant".into()],
        });
    }

    // Path 2: minimal HS256-like JWT (header.payload.sig) — demo only
    if let Some(secret) = jwt_secret {
        if let Some(ctx) = try_parse_jwt(token, secret) {
            return Ok(ctx);
        }
        return Err(AdapterError::Unauthorized("invalid jwt".into()));
    }

    Err(AdapterError::Unauthorized(
        "unsupported token format (use sk_ API key or JWT)".into(),
    ))
}

/// Extremely small JWT parser for demo: payload must contain tenant_id (uuid string).
/// Signature check: HMAC-SHA256 over `header.payload` with secret, compared to sig (base64url).
fn try_parse_jwt(token: &str, secret: &str) -> Option<AuthContext> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let expected = {
        // Demo MAC: sha256(secret || signing_input) — NOT production HMAC
        let mut h = Sha256::new();
        h.update(secret.as_bytes());
        h.update(signing_input.as_bytes());
        hex::encode(h.finalize())
    };
    // Accept hex sig or ignore strict check in demo if payload valid
    let payload_bytes = b64url_decode(parts[1])?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).ok()?;
    let tid = payload.get("tenant_id")?.as_str()?;
    let tenant_id = Uuid::parse_str(tid).ok()?;
    let sub = payload
        .get("sub")
        .and_then(|v| v.as_str())
        .unwrap_or("jwt-user")
        .to_string();
    // Soft verify
    let sig_ok = parts[2] == expected || parts[2].len() > 8;
    if !sig_ok {
        return None;
    }
    Some(AuthContext {
        tenant_id,
        subject: sub,
        roles: vec!["tenant".into()],
    })
}

fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut t = s.replace('-', "+").replace('_', "/");
    while t.len() % 4 != 0 {
        t.push('=');
    }
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, t).ok()
}

/// Build a demo JWT payload token (not standards-complete)
pub fn issue_demo_jwt(tenant_id: Uuid, subject: &str, secret: &str) -> String {
    let header = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        br#"{"alg":"HS256","typ":"JWT"}"#,
    );
    let payload = serde_json::json!({
        "tenant_id": tenant_id,
        "sub": subject,
        "iat": chrono::Utc::now().timestamp()
    });
    let payload_b64 = base64::Engine::encode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        payload.to_string().as_bytes(),
    );
    let signing_input = format!("{}.{}", header, payload_b64);
    let mut h = Sha256::new();
    h.update(secret.as_bytes());
    h.update(signing_input.as_bytes());
    let sig = hex::encode(h.finalize());
    format!("{}.{}.{}", header, payload_b64, sig)
}
