#![allow(dead_code)]

use anyhow::{anyhow, Result};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub album_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    pub jti: String,
}

pub fn sign_jwt(claims: &Claims, secret: &str) -> Result<String> {
    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(secret.as_bytes());

    encode(&header, claims, &key)
        .map_err(|e| anyhow!("Failed to encode JWT: {}", e))
}

pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims> {
    let key = DecodingKey::from_secret(secret.as_bytes());

    // Create validation with HS256 algorithm
    let mut validation = Validation::new(Algorithm::HS256);

    // Disable exp validation by default - we'll check manually
    // This allows us to handle optional expiration (unlimited tokens)
    validation.validate_exp = false;

    // Decode and verify signature
    let token_data = decode::<Claims>(token, &key, &validation)
        .map_err(|e| anyhow!("JWT verification failed: {}", e))?;

    let claims = token_data.claims;

    // Manual expiration check for tokens that have exp field
    if let Some(exp) = claims.exp {
        let now = chrono::Utc::now().timestamp();
        if now > exp {
            return Err(anyhow!("Token expired"));
        }
    }

    Ok(claims)
}
