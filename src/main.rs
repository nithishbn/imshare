mod config;
mod db;
mod jwt;
mod qr;
mod utils;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use config::Config;
use db::Database;
use jwt::Claims;
use utils::{extract_album_id, format_expires_at, get_status, parse_ttl};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "imshare")]
#[command(about = "Generate signed, expiring share links for Immich via immich-public-proxy")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new share link
    Generate {
        /// Immich album UUID or full share URL
        url_or_uuid: String,

        /// Time-to-live (e.g., 7d, 24h, 1w, unlimited)
        #[arg(short, long)]
        ttl: Option<String>,

        /// Human-readable label for this link
        #[arg(short, long)]
        label: Option<String>,
    },

    /// List all generated links
    List,

    /// Revoke an existing link
    Revoke {
        /// Link ID to revoke
        id: i64,
    },

    /// Extend an existing link with a new TTL
    ///
    /// WARNING: This invalidates the old URL by issuing a new token.
    /// Any previously shared links will need to be updated.
    Extend {
        /// Link ID to extend
        id: i64,

        /// New time-to-live (e.g., 7d, 24h, 1w, unlimited)
        ttl: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load()?;
    let db = Database::new(&config.db_path())?;

    match cli.command {
        Commands::Generate {
            url_or_uuid,
            ttl,
            label,
        } => generate(&config, &db, &url_or_uuid, ttl.as_deref(), label.as_deref())?,
        Commands::List => list(&db)?,
        Commands::Revoke { id } => revoke(&db, id)?,
        Commands::Extend { id, ttl } => extend(&config, &db, id, &ttl)?,
    }

    Ok(())
}

fn generate(
    config: &Config,
    db: &Database,
    url_or_uuid: &str,
    ttl: Option<&str>,
    label: Option<&str>,
) -> Result<()> {
    let album_id = extract_album_id(url_or_uuid)?;
    let secret = get_secret()?;

    // Parse TTL
    let ttl_str = ttl.unwrap_or(&config.default_ttl);
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

    let token = jwt::sign_jwt(&claims, &secret)?;

    // Build URL
    let url = format!(
        "https://{}/share/{}?token={}",
        config.public_domain, album_id, token
    );

    // Store in database
    let id = db.insert_link(&album_id, label, &url, &jti, expires_at)?;

    // Get the link to retrieve the generated short_code
    let link = db.get_link_by_id(id)?.context("Failed to retrieve created link")?;
    let short_url = format!("https://{}/s/{}", config.public_domain, link.short_code);

    // Output
    println!("✓ Generated link #{}", id);
    if let Some(lbl) = label {
        println!("  Label: {}", lbl);
    }
    println!("  Album: {}", album_id);
    println!("  Expires: {}", format_expires_at(expires_at));
    println!("\nShort URL:  {}", short_url);
    println!("Full URL:   {}", url);

    // Generate and display QR code
    match qr::generate_qr_code_terminal(&url) {
        Ok(qr) => {
            println!("\n{}", qr);
        }
        Err(e) => {
            eprintln!("\n⚠️  Failed to generate QR code: {}", e);
        }
    }

    Ok(())
}

fn list(db: &Database) -> Result<()> {
    let links = db.list_links()?;

    if links.is_empty() {
        println!("No links found.");
        return Ok(());
    }

    // Print header
    println!(
        "{:<5} {:<20} {:<30} {:<22} {:<10}",
        "ID", "Label", "Short URL", "Expires", "Status"
    );
    println!("{}", "-".repeat(90));

    // Print links
    for link in links {
        let label = link
            .label
            .as_deref()
            .unwrap_or("-")
            .chars()
            .take(20)
            .collect::<String>();

        // Extract domain from config for short URL
        let short_url_display = if link.short_code == "legacy" {
            "legacy".to_string()
        } else {
            // Just show the short code, not the full domain
            format!("/s/{}", link.short_code)
        };

        let expires = format_expires_at(link.expires_at);
        let status = get_status(link.expires_at, link.revoked_at);

        println!(
            "{:<5} {:<20} {:<30} {:<22} {:<10}",
            link.id, label, short_url_display, expires, status
        );
    }

    Ok(())
}

fn revoke(db: &Database, id: i64) -> Result<()> {
    let success = db.revoke_link(id)?;

    if success {
        println!("✓ Revoked link #{}", id);
    } else {
        println!("✗ Link #{} not found or already revoked", id);
    }

    Ok(())
}

fn extend(config: &Config, db: &Database, id: i64, ttl: &str) -> Result<()> {
    // Get existing link
    let link = db
        .get_link_by_id(id)?
        .ok_or_else(|| anyhow!("Link #{} not found", id))?;

    // Parse new TTL
    let duration = parse_ttl(ttl)?;
    let new_expires_at = duration.map(|d| Utc::now() + d);
    let exp_timestamp = new_expires_at.map(|dt| dt.timestamp());

    // Generate new JWT with new jti
    let secret = get_secret()?;
    let new_jti = Uuid::new_v4().to_string();
    let claims = Claims {
        album_id: link.album_id.clone(),
        exp: exp_timestamp,
        jti: new_jti.clone(),
    };

    let token = jwt::sign_jwt(&claims, &secret)?;

    // Build new URL
    let new_url = format!(
        "https://{}/share/{}?token={}",
        config.public_domain, link.album_id, token
    );

    // Update database
    db.extend_link(id, new_expires_at, &new_jti, &new_url)?;

    // Build short URL (short_code doesn't change when extending)
    let short_url = format!("https://{}/s/{}", config.public_domain, link.short_code);

    println!("✓ Extended link #{}", id);
    println!("  New expires: {}", format_expires_at(new_expires_at));
    println!("\n⚠️  WARNING: This has invalidated the old URL.");
    println!("   You must update any previously shared links.\n");
    println!("Short URL:  {}", short_url);
    println!("Full URL:   {}", new_url);

    // Generate and display QR code
    match qr::generate_qr_code_terminal(&new_url) {
        Ok(qr) => {
            println!("\n{}", qr);
        }
        Err(e) => {
            eprintln!("\n⚠️  Failed to generate QR code: {}", e);
        }
    }

    Ok(())
}

fn get_secret() -> Result<String> {
    std::env::var("IMSHARE_SECRET").context("IMSHARE_SECRET environment variable not set")
}
