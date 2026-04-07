use crate::{jwt::Claims, qr, utils, AppState};
use anyhow::{Context, Result};
use axum::{extract::State, response::Html, Form};
use base64::Engine;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

pub async fn dashboard() -> Html<&'static str> {
    Html(include_str!("../templates/dashboard.html"))
}

#[derive(Deserialize)]
pub struct GenerateForm {
    album_id: String,
    #[serde(default)]
    ttl: Option<String>,
    #[serde(default)]
    label: Option<String>,
}

pub async fn handle_generate(
    State(state): State<Arc<AppState>>,
    Form(form): Form<GenerateForm>,
) -> Html<String> {
    match generate_link(&state, form) {
        Ok(html) => Html(html),
        Err(e) => Html(format!(
            r#"<tr class="bg-red-50"><td colspan="5" class="px-6 py-4 text-red-600">Error: {}</td></tr>"#,
            e
        )),
    }
}

fn generate_link(state: &AppState, form: GenerateForm) -> Result<String> {
    let album_id = utils::extract_album_id(&form.album_id)?;
    let ttl_str = form.ttl.as_deref().unwrap_or(&state.config.default_ttl);
    let duration = utils::parse_ttl(ttl_str)?;
    let expires_at = duration.map(|d| Utc::now() + d);
    let exp_timestamp = expires_at.map(|dt| dt.timestamp());

    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        album_id: album_id.clone(),
        exp: exp_timestamp,
        jti: jti.clone(),
    };

    let token = crate::jwt::sign_jwt(&claims, &state.secret)?;
    let url = format!(
        "https://{}/share/{}?token={}",
        state.config.public_domain, album_id, token
    );

    let qr_png = qr::generate_qr_code_png(&url)?;
    let qr_code_base64 = base64::prelude::BASE64_STANDARD.encode(&qr_png);

    let id = state
        .db
        .insert_link(&album_id, form.label.as_deref(), &url, &jti, expires_at)?;
    let link = state
        .db
        .get_link_by_id(id)?
        .context("Failed to retrieve created link")?;
    let short_url = format!(
        "https://{}/s/{}",
        state.config.public_domain, link.short_code
    );

    let expires_str = utils::format_expires_at(expires_at);
    let status = utils::get_status(expires_at, None);
    let label_display = form.label.as_deref().unwrap_or("-");

    Ok(format!(
        r#"<tr class="hover:bg-gray-50">
            <td class="px-6 py-4 text-sm text-gray-900">{}</td>
            <td class="px-6 py-4 text-sm">
                <div class="flex items-center space-x-2">
                    <a href="{}" target="_blank" class="text-blue-600 hover:text-blue-800 underline"><code>{}</code></a>
                    <button onclick="navigator.clipboard.writeText('{}')" class="text-xs text-gray-500 hover:text-gray-700">Copy</button>
                </div>
            </td>
            <td class="px-6 py-4 text-sm text-gray-600">{}</td>
            <td class="px-6 py-4 text-sm">
                <span class="px-2 py-1 text-xs rounded-full bg-green-100 text-green-800">{}</span>
            </td>
            <td class="px-6 py-4 text-sm space-x-2">
                <button onclick="document.getElementById('qr-content').innerHTML='<img src=&quot;data:image/png;base64,{}&quot; class=&quot;w-full&quot;/>';document.getElementById('qr-modal').classList.remove('hidden')" class="text-blue-600 hover:text-blue-800">QR</button>
                <form hx-post="/imshare/revoke" hx-swap="outerHTML" class="inline">
                    <input type="hidden" name="id" value="{}"/>
                    <button type="submit" class="text-red-600 hover:text-red-800">Revoke</button>
                </form>
            </td>
        </tr>"#,
        label_display, short_url, short_url, short_url, expires_str, status, qr_code_base64, id
    ))
}

pub async fn handle_list(State(state): State<Arc<AppState>>) -> Html<String> {
    match list_links(&state) {
        Ok(html) => Html(html),
        Err(e) => Html(format!(
            r#"<tr><td colspan="5" class="px-6 py-4 text-red-600">Error: {}</td></tr>"#,
            e
        )),
    }
}

fn list_links(state: &AppState) -> Result<String> {
    let links = state.db.list_links()?;

    let mut html = String::new();
    for link in links {
        let short_url = format!(
            "https://{}/s/{}",
            state.config.public_domain, link.short_code
        );
        let expires_str = utils::format_expires_at(link.expires_at);
        let status = utils::get_status(link.expires_at, link.revoked_at);
        let label_display = link.label.as_deref().unwrap_or("-");

        let qr_png = qr::generate_qr_code_png(&link.url)?;
        let qr_code_base64 = base64::prelude::BASE64_STANDARD.encode(&qr_png);

        let status_class = match status {
            "active" => "bg-green-100 text-green-800",
            "expired" => "bg-yellow-100 text-yellow-800",
            "revoked" => "bg-red-100 text-red-800",
            _ => "bg-gray-100 text-gray-800",
        };

        html.push_str(&format!(
            r#"<tr class="hover:bg-gray-50">
                <td class="px-6 py-4 text-sm text-gray-900">{}</td>
                <td class="px-6 py-4 text-sm">
                    <div class="flex items-center space-x-2">
                        <a href="{}" target="_blank" class="text-blue-600 hover:text-blue-800 underline"><code>{}</code></a>
                        <button onclick="navigator.clipboard.writeText('{}')" class="text-xs text-gray-500 hover:text-gray-700">Copy</button>
                    </div>
                </td>
                <td class="px-6 py-4 text-sm text-gray-600">{}</td>
                <td class="px-6 py-4 text-sm">
                    <span class="px-2 py-1 text-xs rounded-full {}">{}</span>
                </td>
                <td class="px-6 py-4 text-sm space-x-2">
                    <button onclick="document.getElementById('qr-content').innerHTML='<img src=&quot;data:image/png;base64,{}&quot; class=&quot;w-full&quot;/>';document.getElementById('qr-modal').classList.remove('hidden')" class="text-blue-600 hover:text-blue-800">QR</button>
                    {}
                </td>
            </tr>"#,
            label_display,
            short_url,
            short_url,
            short_url,
            expires_str,
            status_class,
            status,
            qr_code_base64,
            if status == "revoked" {
                String::new()
            } else {
                format!(r#"<form hx-post="/imshare/revoke" hx-swap="outerHTML" class="inline"><input type="hidden" name="id" value="{}"/><button type="submit" class="text-red-600 hover:text-red-800">Revoke</button></form>"#, link.id)
            }
        ));
    }

    Ok(html)
}

#[derive(Deserialize)]
pub struct RevokeForm {
    id: i64,
}

pub async fn handle_revoke(
    State(state): State<Arc<AppState>>,
    Form(form): Form<RevokeForm>,
) -> Html<String> {
    match state.db.revoke_link(form.id) {
        Ok(true) => Html(format!(
            r#"<tr class="hover:bg-gray-50 bg-red-50">
                    <td colspan="5" class="px-6 py-4 text-sm text-gray-600">Link {} revoked successfully. Refresh to see updated status.</td>
                </tr>"#,
            form.id
        )),
        Ok(false) => Html(format!(
            r#"<tr class="hover:bg-gray-50 bg-yellow-50">
                <td colspan="5" class="px-6 py-4 text-sm text-gray-600">Link {} not found or already revoked.</td>
            </tr>"#,
            form.id
        )),
        Err(e) => Html(format!(
            r#"<tr class="bg-red-50">
                <td colspan="5" class="px-6 py-4 text-sm text-red-600">Error: {}</td>
            </tr>"#,
            e
        )),
    }
}
