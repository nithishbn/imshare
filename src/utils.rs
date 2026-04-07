use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use regex::Regex;

pub fn parse_ttl(ttl: &str) -> Result<Option<Duration>> {
    if ttl == "unlimited" || ttl == "never" {
        return Ok(None);
    }

    let re = Regex::new(r"^(\d+)([hdwmy])$").unwrap();
    let caps = re.captures(ttl).ok_or_else(|| {
        anyhow!("Invalid TTL format. Use format like: 7d, 24h, 1w, or 'unlimited'")
    })?;

    let value: i64 = caps[1].parse()?;
    let unit = &caps[2];

    let duration = match unit {
        "h" => Duration::hours(value),
        "d" => Duration::days(value),
        "w" => Duration::weeks(value),
        "m" => Duration::days(value * 30),  // Approximate
        "y" => Duration::days(value * 365), // Approximate
        _ => return Err(anyhow!("Invalid time unit")),
    };

    Ok(Some(duration))
}

pub fn extract_album_id(input: &str) -> Result<String> {
    // Check if it's a URL
    if input.contains("://") || input.contains('/') {
        // Match Immich share keys (base64url-like: alphanumeric, underscore, hyphen)
        let re = Regex::new(r"/share/([A-Za-z0-9_-]+)").unwrap();
        if let Some(caps) = re.captures(input) {
            return Ok(caps[1].to_string());
        }
        return Err(anyhow!("Could not extract album UUID from URL"));
    }

    // Otherwise treat as raw UUID
    Ok(input.to_string())
}

pub fn format_expires_at(expires_at: Option<chrono::DateTime<chrono::Utc>>) -> String {
    match expires_at {
        Some(dt) => {
            let now = Utc::now();
            if dt < now {
                format!("{} (expired)", dt.format("%Y-%m-%d %H:%M UTC"))
            } else {
                dt.format("%Y-%m-%d %H:%M UTC").to_string()
            }
        }
        None => "unlimited".to_string(),
    }
}

pub fn get_status(
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
) -> &'static str {
    if revoked_at.is_some() {
        return "revoked";
    }

    if let Some(exp) = expires_at {
        if exp < Utc::now() {
            return "expired";
        }
    }

    "active"
}
