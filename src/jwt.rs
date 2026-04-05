use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub album_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    pub jti: String,
}

pub fn sign_jwt(claims: &Claims, secret: &str) -> Result<String> {
    // Create header
    let header = serde_json::json!({
        "alg": "HS256",
        "typ": "JWT"
    });

    // Encode header and payload
    let header_encoded = URL_SAFE_NO_PAD.encode(serde_json::to_string(&header)?);
    let payload_encoded = URL_SAFE_NO_PAD.encode(serde_json::to_string(&claims)?);

    // Create message to sign
    let message = format!("{}.{}", header_encoded, payload_encoded);

    // Create HMAC signature
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow!("Invalid secret key: {}", e))?;
    mac.update(message.as_bytes());
    let signature = mac.finalize().into_bytes();

    // Encode signature
    let signature_encoded = URL_SAFE_NO_PAD.encode(&signature);

    // Combine into JWT
    Ok(format!("{}.{}", message, signature_encoded))
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims> {
    // Split token into parts
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(anyhow!("Invalid JWT format"));
    }

    let header_encoded = parts[0];
    let payload_encoded = parts[1];
    let signature_provided = parts[2];

    // Verify signature
    let message = format!("{}.{}", header_encoded, payload_encoded);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow!("Invalid secret key: {}", e))?;
    mac.update(message.as_bytes());

    let signature_expected = mac.finalize().into_bytes();
    let signature_expected_encoded = URL_SAFE_NO_PAD.encode(&signature_expected);

    if signature_provided != signature_expected_encoded {
        return Err(anyhow!("Invalid signature"));
    }

    // Decode payload
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_encoded)
        .map_err(|e| anyhow!("Failed to decode payload: {}", e))?;
    let claims: Claims = serde_json::from_slice(&payload_bytes)
        .map_err(|e| anyhow!("Failed to parse claims: {}", e))?;

    // Check expiration
    if let Some(exp) = claims.exp {
        let now = chrono::Utc::now().timestamp();
        if now > exp {
            return Err(anyhow!("Token expired"));
        }
    }

    Ok(claims)
}
