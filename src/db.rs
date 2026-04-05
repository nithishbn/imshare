use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct Link {
    pub id: i64,
    pub album_id: String,
    pub label: Option<String>,
    pub url: String,
    pub jti: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // Create table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS links (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                album_id TEXT NOT NULL,
                label TEXT,
                url TEXT NOT NULL,
                jti TEXT NOT NULL UNIQUE,
                created_at TEXT NOT NULL,
                expires_at TEXT,
                revoked_at TEXT
            )",
            [],
        )?;

        Ok(Database { conn: Mutex::new(conn) })
    }

    pub fn insert_link(
        &self,
        album_id: &str,
        label: Option<&str>,
        url: &str,
        jti: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<i64> {
        let created_at = Utc::now();
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO links (album_id, label, url, jti, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                album_id,
                label,
                url,
                jti,
                created_at.to_rfc3339(),
                expires_at.map(|dt| dt.to_rfc3339())
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    pub fn list_links(&self) -> Result<Vec<Link>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, album_id, label, url, jti, created_at, expires_at, revoked_at FROM links ORDER BY created_at DESC",
        )?;

        let links = stmt
            .query_map([], |row| {
                Ok(Link {
                    id: row.get(0)?,
                    album_id: row.get(1)?,
                    label: row.get(2)?,
                    url: row.get(3)?,
                    jti: row.get(4)?,
                    created_at: row
                        .get::<_, String>(5)?
                        .parse::<DateTime<Utc>>()
                        .unwrap(),
                    expires_at: row
                        .get::<_, Option<String>>(6)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    revoked_at: row
                        .get::<_, Option<String>>(7)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(links)
    }

    pub fn revoke_link(&self, id: i64) -> Result<bool> {
        let revoked_at = Utc::now();
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE links SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            params![revoked_at.to_rfc3339(), id],
        )?;

        Ok(affected > 0)
    }

    pub fn extend_link(&self, id: i64, new_expires_at: Option<DateTime<Utc>>, new_jti: &str, new_url: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE links SET expires_at = ?1, jti = ?2, url = ?3 WHERE id = ?4",
            params![
                new_expires_at.map(|dt| dt.to_rfc3339()),
                new_jti,
                new_url,
                id
            ],
        )?;

        Ok(affected > 0)
    }

    pub fn get_link_by_id(&self, id: i64) -> Result<Option<Link>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, album_id, label, url, jti, created_at, expires_at, revoked_at FROM links WHERE id = ?1",
        )?;

        let link = stmt
            .query_row([id], |row| {
                Ok(Link {
                    id: row.get(0)?,
                    album_id: row.get(1)?,
                    label: row.get(2)?,
                    url: row.get(3)?,
                    jti: row.get(4)?,
                    created_at: row
                        .get::<_, String>(5)?
                        .parse::<DateTime<Utc>>()
                        .unwrap(),
                    expires_at: row
                        .get::<_, Option<String>>(6)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    revoked_at: row
                        .get::<_, Option<String>>(7)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                })
            })
            .optional()?;

        Ok(link)
    }

    pub fn check_token(&self, jti: &str) -> Result<Option<TokenStatus>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT expires_at, revoked_at FROM links WHERE jti = ?1",
        )?;

        let status = stmt
            .query_row([jti], |row| {
                let expires_at: Option<String> = row.get(0)?;
                let revoked_at: Option<String> = row.get(1)?;

                Ok(TokenStatus {
                    expires_at: expires_at.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    revoked_at: revoked_at.and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                })
            })
            .optional()?;

        Ok(status)
    }
}

pub struct TokenStatus {
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}
