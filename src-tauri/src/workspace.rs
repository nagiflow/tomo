use crate::{auth, models::*, AppState};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

// ponytail: Google Contacts is not offered for now; add "contacts" back here and in the Settings list to restore it
pub const TOOLS: [&str; 5] = ["calendar", "drive", "docs", "sheets", "slides"];
pub fn mode(settings: &Settings, kind: &str) -> String {
    settings.tools.get(kind).cloned().unwrap_or("off".into())
}
pub fn scopes(settings: &Settings) -> String {
    let mut scopes = vec![
        // marking filed mail as read or archiving it needs more than read-only access
        if settings.after_archive == "none" {
            "https://www.googleapis.com/auth/gmail.readonly"
        } else {
            "https://www.googleapis.com/auth/gmail.modify"
        },
        "https://www.googleapis.com/auth/gmail.send",
    ];
    for kind in TOOLS {
        let access = mode(settings, kind);
        if access == "off" {
            continue;
        }
        scopes.push(tool_scope(kind, &access));
    }
    scopes.join(" ")
}
fn tool_scope(kind: &str, access: &str) -> &'static str {
    match (kind, access) {
        ("calendar", "read") => "https://www.googleapis.com/auth/calendar.events.readonly",
        ("calendar", _) => "https://www.googleapis.com/auth/calendar.events",
        ("drive", "read") => "https://www.googleapis.com/auth/drive.readonly",
        ("drive", _) => "https://www.googleapis.com/auth/drive",
        ("docs", _) => "https://www.googleapis.com/auth/documents",
        // Tomo only creates new spreadsheets and decks, so the per-file scope is enough: it cannot open your existing ones.
        ("sheets" | "slides", _) => "https://www.googleapis.com/auth/drive.file",
        _ => "https://www.googleapis.com/auth/contacts.readonly",
    }
}
pub fn account(state: &AppState) -> Result<Account, String> {
    let s = state.store.lock().map_err(|_| "Storage busy")?;
    let settings = s.settings()?;
    let accounts: Vec<Account> = s.list("account")?;
    accounts
        .into_iter()
        .find(|a| {
            a.provider == "gmail"
                && a.status == "connected"
                && (settings.google_account_id.is_empty() || a.id == settings.google_account_id)
        })
        .ok_or("Connect a Google account in Settings to use this tool.".into())
}
pub async fn request(
    state: &AppState,
    method: reqwest::Method,
    url: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let parsed = url::Url::parse(url).map_err(|_| "Invalid Google endpoint")?;
    if parsed.scheme() != "https"
        || !matches!(
            parsed.host_str(),
            Some("www.googleapis.com" | "docs.googleapis.com" | "people.googleapis.com")
        )
    {
        return Err("Unexpected Google endpoint".into());
    }
    let a = account(state)?;
    let token = auth::access(state, &a.id, "gmail").await?;
    let mut req = state.http.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        req = req.json(&body)
    }
    let res = req.send().await.map_err(|_| {
        "Google could not confirm the request. Check the action outcome before retrying."
    })?;
    if res.status() == reqwest::StatusCode::NO_CONTENT {
        return Ok(json!({}));
    }
    auth::response_json(res).await
}
pub fn segment(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
pub async fn events(state: &AppState, query: &str) -> Result<Value, String> {
    let now = chrono::Utc::now();
    let mut url =
        url::Url::parse("https://www.googleapis.com/calendar/v3/calendars/primary/events").unwrap();
    url.query_pairs_mut()
        .append_pair("singleEvents", "true")
        .append_pair("orderBy", "startTime")
        .append_pair("maxResults", "30")
        .append_pair("timeMin", &(now - chrono::Duration::days(7)).to_rfc3339())
        .append_pair("timeMax", &(now + chrono::Duration::days(45)).to_rfc3339());
    if !query.trim().is_empty() {
        url.query_pairs_mut().append_pair("q", query);
    }
    request(state, reqwest::Method::GET, url.as_str(), None).await
}
pub async fn files(state: &AppState, query: &str) -> Result<Value, String> {
    let mut url = url::Url::parse("https://www.googleapis.com/drive/v3/files").unwrap();
    url.query_pairs_mut()
        .append_pair(
            "q",
            &format!(
                "trashed = false and (name contains '{}')",
                query.replace('\\', "\\\\").replace('\'', "\\'")
            ),
        )
        .append_pair("pageSize", "15")
        .append_pair("fields", "files(id,name,mimeType,webViewLink,modifiedTime)");
    request(state, reqwest::Method::GET, url.as_str(), None).await
}
fn import_client(state: &AppState, path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 64_000 {
        return Err("OAuth client file is too large.".into());
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Choose a valid Google Desktop client_secret.json file.")?;
    let client = &value["installed"];
    let id = client["client_id"]
        .as_str()
        .ok_or("This JSON is not a Google Desktop OAuth client.")?;
    let secret = client["client_secret"]
        .as_str()
        .ok_or("Missing client secret")?;
    crate::storage::credential_set("google-client-secret", secret)?;
    let s = state.store.lock().map_err(|_| "Storage busy")?;
    let mut settings = s.settings()?;
    settings.google_client_id = id.into();
    settings.google_project_id = client["project_id"].as_str().unwrap_or("").into();
    s.put("settings", "current", &settings)?;
    Ok(())
}
#[tauri::command]
/// `connect` is false during first-run setup: sign-in waits until the services are chosen,
/// so Google asks for consent once.
pub async fn import_google_client(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    connect: bool,
) -> Result<(), String> {
    let file = app
        .dialog()
        .file()
        .add_filter("Google OAuth client", &["json"])
        .blocking_pick_file();
    if let Some(file) = file {
        let path = file.into_path().map_err(|_| "Choose a local JSON file.")?;
        import_client(&state, &path)?;
        if connect {
            auth::connect(state.inner().clone(), "gmail".into()).await?;
        }
        crate::engine::updated(&state);
    }
    Ok(())
}
#[tauri::command]
pub async fn connect_workspace_tool(
    state: State<'_, Arc<AppState>>,
    kind: String,
    access: String,
) -> Result<(), String> {
    if !TOOLS.contains(&kind.as_str()) || !["off", "read", "ask"].contains(&access.as_str()) {
        return Err("Unknown tool or permission mode.".into());
    }
    let previous = {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        let mut settings = s.settings()?;
        let previous = mode(&settings, &kind);
        settings.tools.insert(kind.clone(), access.clone());
        s.put("settings", "current", &settings)?;
        previous
    };
    if access != "off" {
        if let Err(e) = auth::connect(state.inner().clone(), "gmail".into()).await {
            let s = state.store.lock().map_err(|_| "Storage busy")?;
            let mut settings = s.settings()?;
            settings.tools.insert(kind, previous);
            s.put("settings", "current", &settings)?;
            return Err(e);
        }
    }
    crate::engine::updated(&state);
    Ok(())
}
