mod ai;
mod auth;
mod codex;
mod connectors;
mod engine;
mod models;
mod plans;
mod policy;
mod slack;
mod storage;
mod style;
mod workspace;
use models::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use storage::Store;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State, WebviewUrl, WebviewWindowBuilder,
};
pub struct AppState {
    store: Mutex<Store>,
    http: reqwest::Client,
    refresh_lock: tokio::sync::Mutex<()>,
    auth_lock: tokio::sync::Mutex<()>,
    send_lock: tokio::sync::Mutex<()>,
    plan_lock: tokio::sync::Mutex<()>,
    sync_lock: tokio::sync::Mutex<()>,
    syncing: AtomicBool,
    last_error: Mutex<Option<String>>,
    /// What Tomo is doing right now, by message ID: the step it is on, shown as progress in the UI.
    working: Mutex<std::collections::HashMap<String, String>>,
    notifications: Mutex<std::collections::HashMap<String, std::time::Instant>>,
    app: OnceLock<tauri::AppHandle>,
}
type Shared = Arc<AppState>;
#[tauri::command]
// async so it runs off the UI thread: it waits on storage and the Keychain, which froze the window.
async fn get_snapshot(state: State<'_, Shared>) -> Result<Snapshot, String> {
    let store = state.store.lock().map_err(|_| "Storage busy")?;
    let mut settings = store.settings()?;
    settings.google_client_secret.clear();
    let provider = settings.ai_provider.clone();
    let token = if provider == "siwc" {
        auth::load("siwc")?
    } else {
        None
    };
    let connected = if provider == "siwc" {
        token.is_some()
    } else {
        storage::credential_get("openai-api-key")?.is_some()
    };
    let ai = AiStatus {
        connected,
        provider: provider.clone(),
        email: token.as_ref().and_then(|t| t.email.clone()),
        status: if connected {
            "Authorization saved"
        } else {
            "Not connected"
        }
        .into(),
        models: store.get("models", &provider)?.unwrap_or_default(),
    };
    let mut accounts: Vec<Account> = store.list("account")?;
    // A read-only Gmail sign-in cannot mark mail read or archive it; say so where Reconnect is.
    if settings.after_archive != "none" {
        for a in accounts.iter_mut().filter(|a| read_only(a)) {
            a.error = Some("Reconnect to let Tomo mark mail as read or archive it".into());
        }
    }
    let error = state
        .last_error
        .lock()
        .map_err(|_| "State busy")?
        .clone()
        .or_else(|| accounts.iter().find_map(|a| a.error.clone()));
    // ponytail: reads every cached message per snapshot; keep an index of unprocessed IDs if the cache grows large
    let working = state.working.lock().map_err(|_| "State busy")?.clone();
    let pending = store
        .list::<Message>("message")?
        .into_iter()
        .filter(|m| m.requires_processing)
        .map(|m| {
            let received = m.timestamp.clone();
            let step = working.get(&m.id);
            let mut row = engine::event(
                &engine::manual_fallback(m, ""),
                if step.is_some() {
                    "processing"
                } else {
                    "pending"
                },
                step.map_or("Waiting in line", |step| step.as_str()),
                false,
                None,
            );
            row.id = format!("pending:{}", row.message_id);
            row.timestamp = received;
            row
        })
        .collect();
    Ok(Snapshot {
        settings,
        accounts,
        pending,
        working,
        approvals: store.list("approval")?,
        activity: store.list("activity")?,
        ai,
        syncing: state.syncing.load(Ordering::Relaxed),
        last_error: error,
        style: style::load_from_store(&store)?,
        rules: style::rules(&state),
        codex_installed: codex::binary().is_some(),
    })
}
#[tauri::command]
fn get_cached_messages(state: State<Shared>) -> Result<Vec<Message>, String> {
    state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .list("message")
}
/// One stored message, for Activity to show what an item it lists was about.
#[tauri::command]
fn get_message(state: State<Shared>, id: String) -> Result<Option<Message>, String> {
    state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .get("message", &id)
}
#[tauri::command]
fn open_codex_file(state: State<Shared>, id: String, file: String) -> Result<(), String> {
    let item = state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .get::<Approval>("approval", &id)?
        .ok_or("Plan no longer exists")?;
    codex::open_produced(&state, &item, &file)
}
#[tauri::command]
fn save_settings(state: State<Shared>, mut settings: Settings) -> Result<(), String> {
    if !["siwc", "openai"].contains(&settings.ai_provider.as_str())
        || settings.poll_seconds < 60
        || settings.poll_seconds > 3600
        || !(1..=engine::MAX_WORKERS).contains(&settings.workers)
        || settings.tone.len() > 1000
        || !["", "low", "medium", "high"].contains(&settings.reasoning_effort.as_str())
        || !["none", "read", "archive"].contains(&settings.after_archive.as_str())
    {
        return Err(
            "Invalid provider, polling interval, worker count, thinking level or tone preference"
                .into(),
        );
    }
    // Ordinary preference saves cannot undo a kill switch from another window.
    {
        let store = state.store.lock().map_err(|_| "Storage busy")?;
        let current = store.settings()?;
        settings.paused = current.paused;
        settings.auto_send = current.auto_send && settings.consent && !current.paused;
    }
    if !settings.google_client_secret.is_empty() {
        storage::credential_set("google-client-secret", &settings.google_client_secret)?;
        settings.google_client_secret.clear();
    }
    state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .put("settings", "current", &settings)?;
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
fn set_processing_paused(state: State<Shared>, paused: bool) -> Result<(), String> {
    let store = state.store.lock().map_err(|_| "Storage busy")?;
    let mut settings = store.settings()?;
    settings.paused = paused;
    settings.auto_send = false;
    store.put("settings", "current", &settings)?;
    drop(store);
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
fn set_auto_send(state: State<Shared>, enabled: bool) -> Result<(), String> {
    let store = state.store.lock().map_err(|_| "Storage busy")?;
    let mut settings = store.settings()?;
    if enabled && settings.paused {
        return Err(
            "Processing is paused. Resume it first (Pause processing, or Resume at the top)."
                .into(),
        );
    }
    if enabled && !settings.consent {
        return Err("Connect an AI provider first.".into());
    }
    settings.auto_send = enabled;
    store.put("settings", "current", &settings)?;
    drop(store);
    engine::updated(&state);
    Ok(())
}
/// "Ask me next time": Tomo stops answering this message's sender on its own.
#[tauri::command]
fn ask_next_time(state: State<Shared>, id: String) -> Result<(), String> {
    let store = state.store.lock().map_err(|_| "Storage busy")?;
    let sender = store
        .get::<Message>("message", &id)?
        .ok_or("Message is no longer cached")?
        .sender;
    let address = sender.address.unwrap_or(sender.id);
    let mut settings = store.settings()?;
    if !settings
        .blocked_senders
        .iter()
        .any(|v| v.eq_ignore_ascii_case(&address))
    {
        settings.blocked_senders.push(address);
        store.put("settings", "current", &settings)?;
    }
    drop(store);
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
fn save_draft(state: State<Shared>, id: String, content: String) -> Result<(), String> {
    if content.len() > 12000 {
        return Err("Draft is too long".into());
    }
    let s = state.store.lock().map_err(|_| "Storage busy")?;
    let mut item = s
        .get::<Approval>("approval", &id)?
        .ok_or("Draft no longer exists")?;
    if item.status != "pending" {
        return Err("Reply is no longer pending".into());
    }
    item.draft = content;
    s.put("approval", &id, &item)
}
#[tauri::command]
async fn approve_reply(
    state: State<'_, Shared>,
    id: String,
    content: String,
) -> Result<(), String> {
    engine::send(state.inner().clone(), id, content, true).await
}
#[tauri::command]
fn dismiss_reply(state: State<Shared>, id: String) -> Result<(), String> {
    let store = state.store.lock().map_err(|_| "Storage busy")?;
    let mut item = store
        .get::<Approval>("approval", &id)?
        .ok_or("Approval no longer exists")?;
    // "uncertain" too: a stopped plan can no longer be sent, so Dismiss is the only way to clear it.
    if item.status != "pending" && item.status != "uncertain" {
        return Err("This item is no longer pending".into());
    }
    item.status = "dismissed".into();
    store.put("approval", &id, &item)?;
    let e = engine::event(&item, "dismissed", "You dismissed this draft.", true, None);
    store.put("activity", &e.id, &e)?;
    drop(store);
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
async fn redraft_reply(state: State<'_, Shared>, id: String) -> Result<(), String> {
    let (mut item, settings) = {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        (
            s.get::<Approval>("approval", &id)?
                .ok_or("Approval no longer exists")?,
            s.settings()?,
        )
    };
    if item.status != "pending" {
        return Err("This item is no longer pending".into());
    }
    if !settings.consent {
        return Err("Enable AI processing consent in Settings before redrafting.".into());
    }
    if item.context.is_empty() {
        use connectors::CommunicationConnector;
        let account = state
            .store
            .lock()
            .map_err(|_| "Storage busy")?
            .get::<Account>("account", &item.message.account_id)?
            .ok_or("Account disconnected")?;
        let connector = connectors::MailConnector {
            state: state.inner().clone(),
        };
        item.context = connector
            .fetch_conversation(&account, &item.message)
            .await?;
    }
    ai::open_session(&state, &id, &item.message, &item.context, &settings.tone)?;
    let provider = ai::configured_provider(state.inner().clone(), Some(id.clone()))?;
    let draft = provider
        .generate_reply(&item.message, &item.context, &settings.tone)
        .await?;
    item.draft = draft.content;
    {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        if s.get::<Approval>("approval", &id)?
            .is_none_or(|a| a.status != "pending")
        {
            return Err("Approval changed while drafting".into());
        }
        s.put("approval", &id, &item)?;
        s.put("missing-facts", &id, &draft.missing_facts)?;
    }
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
async fn sync_now(state: State<'_, Shared>) -> Result<(), String> {
    engine::synchronize(state.inner().clone(), true).await
}
#[tauri::command]
async fn connect_account(state: State<'_, Shared>, provider: String) -> Result<(), String> {
    auth::connect(state.inner().clone(), provider).await?;
    engine::updated(&state);
    Ok(())
}
/// A connected Gmail account whose saved sign-in lacks permission to change mail.
fn read_only(a: &Account) -> bool {
    a.provider == "gmail"
        && auth::load(&a.id).ok().flatten().is_some_and(|t| {
            !t.scope
                .split_whitespace()
                .any(|s| s == "https://www.googleapis.com/auth/gmail.modify")
        })
}
/// Asks Google for permission to change mail when a saved Gmail sign-in is read-only, as every
/// sign-in made before "Mail Tomo archives" was switched on is. Without it Gmail refuses to mark
/// mail as read.
#[tauri::command]
async fn allow_mail_changes(state: State<'_, Shared>) -> Result<(), String> {
    let accounts: Vec<Account> = state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .list("account")?;
    // ponytail: one consent per change of the setting; with several read-only Gmail accounts, change it once per account
    if accounts.iter().any(read_only) {
        auth::connect(state.inner().clone(), "gmail".into()).await?;
        engine::updated(&state);
    }
    Ok(())
}
/// Connects Slack with the user token of an app the user created in their own workspace.
#[tauri::command]
async fn connect_slack(
    state: State<'_, Shared>,
    token: String,
    app_token: String,
) -> Result<(), String> {
    slack::connect(&state, &token, &app_token, None).await?;
    engine::updated(&state);
    Ok(())
}
/// Saves the user's Slack app, then signs in to a first workspace with it. Further workspaces
/// sign in through connect_account.
#[tauri::command]
async fn set_up_slack_app(
    state: State<'_, Shared>,
    client_id: String,
    app_token: String,
) -> Result<(), String> {
    slack::save_app(&state, &client_id, &app_token).await?;
    engine::updated(&state);
    auth::connect(state.inner().clone(), "slack".into()).await?;
    engine::updated(&state);
    Ok(())
}
/// The app manifest, for pasting by hand when Slack's create-app link does not fill it in.
#[tauri::command]
fn slack_manifest() -> &'static str {
    slack::MANIFEST
}
#[tauri::command]
async fn connect_ai(state: State<'_, Shared>) -> Result<(), String> {
    auth::connect(state.inner().clone(), "siwc".into()).await?;
    let models = ai::models(&state, "siwc").await?;
    {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        s.put("models", "siwc", &models)?;
        let mut settings = s.settings()?;
        settings.ai_provider = "siwc".into();
        // Signing in is the consent to AI processing; Settings can withdraw it.
        settings.consent = true;
        if settings.model.is_empty() {
            if let Some(m) = models.first() {
                settings.model = m.id.clone();
            }
        }
        s.put("settings", "current", &settings)?;
    }
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
async fn list_models(state: State<'_, Shared>) -> Result<(), String> {
    let provider = state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .settings()?
        .ai_provider;
    let models = ai::models(&state, &provider).await?;
    state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .put("models", &provider, &models)?;
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
async fn save_api_key(state: State<'_, Shared>, key: String) -> Result<(), String> {
    if key.trim().is_empty() || key.len() > 1024 {
        return Err("Enter a valid API key".into());
    }
    let response = state
        .http
        .get("https://api.openai.com/v1/models")
        .bearer_auth(key.trim())
        .send()
        .await
        .map_err(|_| "Could not verify API key. Check the connection and try again.")?;
    let catalog = auth::response_json(response).await?;
    let models = ai::parse_models(&catalog, "openai")?;
    storage::credential_set("openai-api-key", key.trim())?;
    {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        s.put("models", "openai", &models)?;
        let mut settings = s.settings()?;
        settings.consent = true;
        s.put("settings", "current", &settings)?;
    }
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
async fn disconnect_ai(state: State<'_, Shared>) -> Result<(), String> {
    let _lock = state.refresh_lock.lock().await;
    storage::credential_delete("tokens:siwc")?;
    storage::credential_delete("openai-api-key")?;
    {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        let mut settings = s.settings()?;
        settings.consent = false;
        settings.auto_send = false;
        settings.model.clear();
        s.put("settings", "current", &settings)?;
        s.clear("models")?;
    }
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
async fn disconnect_account(state: State<'_, Shared>, id: String) -> Result<(), String> {
    let _send = state.send_lock.lock().await;
    let _refresh = state.refresh_lock.lock().await;
    storage::credential_delete(&format!("tokens:{id}"))?;
    storage::credential_delete(&format!("slack-app-token:{id}"))?;
    {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        s.delete("account", &id)?;
        for m in s
            .list::<Message>("message")?
            .into_iter()
            .filter(|m| m.account_id == id)
        {
            s.delete("message", &m.id)?;
            s.delete("risk", &m.id)?;
            s.delete("missing-facts", &m.id)?;
            s.delete("send-draft", &m.id)?;
            s.delete("correction", &m.id)?;
            s.delete("classification", &m.id)?;
            s.delete("session", &m.id)?;
        }
        for a in s
            .list::<Approval>("approval")?
            .into_iter()
            .filter(|a| a.message.account_id == id)
        {
            s.delete("approval", &a.id)?;
        }
        for a in s
            .list::<Activity>("activity")?
            .into_iter()
            .filter(|a| a.account_id == id)
        {
            s.delete("activity", &a.id)?;
        }
    }
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
fn clear_history(state: State<Shared>) -> Result<(), String> {
    state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .clear("activity")?;
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
async fn reset_app(state: State<'_, Shared>) -> Result<(), String> {
    let _auth = state.auth_lock.lock().await;
    let _sync = state.sync_lock.lock().await;
    let _send = state.send_lock.lock().await;
    let _refresh = state.refresh_lock.lock().await;
    let s = state.store.lock().map_err(|_| "Storage busy")?;
    for a in s.list::<Account>("account")? {
        storage::credential_delete(&format!("tokens:{}", a.id))?;
        storage::credential_delete(&format!("slack-app-token:{}", a.id))?;
    }
    for key in ["tokens:siwc", "openai-api-key", "google-client-secret"] {
        storage::credential_delete(key)?;
    }
    s.reset()?;
    drop(s);
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
fn export_configuration(state: State<Shared>) -> Result<String, String> {
    let s = state.store.lock().map_err(|_| "Storage busy")?;
    let settings = s.settings()?;
    serde_json::to_string_pretty(&serde_json::json!({"version":1,"preferences":{"tone":settings.tone,"pollSeconds":settings.poll_seconds,"notifications":settings.notifications,"blockedSenders":settings.blocked_senders,"blockedThreads":settings.blocked_threads}})).map_err(|e|e.to_string())
}
#[tauri::command]
fn open_setup_page(provider: String) -> Result<(), String> {
    if provider == "slack" {
        let manifest: String =
            url::form_urlencoded::byte_serialize(slack::MANIFEST.as_bytes()).collect();
        return webbrowser::open(&format!(
            "https://api.slack.com/apps?new_app=1&manifest_json={manifest}"
        ))
        .map_err(|e| e.to_string());
    }
    let url = match provider.as_str() {
        "gmail" => "https://console.cloud.google.com/apis/credentials",
        "google-project" => "https://console.cloud.google.com/projectcreate",
        "google-apis" => "https://console.cloud.google.com/flows/enableapi?apiid=gmail.googleapis.com,calendar-json.googleapis.com,drive.googleapis.com,docs.googleapis.com,sheets.googleapis.com,slides.googleapis.com",
        "google-publish" => "https://console.cloud.google.com/auth/audience",
        "google-consent" => "https://console.cloud.google.com/auth/overview",
        "google-client" => "https://console.cloud.google.com/auth/clients/create",
        "outlook" => {
            "https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade"
        }
        _ => return Err("Unknown account setup page".into()),
    };
    webbrowser::open(url).map_err(|e| e.to_string())
}
#[tauri::command]
fn open_original(state: State<Shared>, id: String) -> Result<(), String> {
    let item = state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .get::<Approval>("approval", &id)?
        .ok_or("Message no longer exists")?;
    let url = if item.message.provider == "slack" {
        slack::link(
            &state
                .store
                .lock()
                .map_err(|_| "Storage busy")?
                .get::<Account>("account", &item.message.account_id)?
                .ok_or("Account disconnected")?,
            &item.message,
        )
    } else if item.message.provider == "gmail" {
        format!(
            "https://mail.google.com/mail/u/{}/#all/{}",
            url::form_urlencoded::byte_serialize(
                state
                    .store
                    .lock()
                    .map_err(|_| "Storage busy")?
                    .get::<Account>("account", &item.message.account_id)?
                    .ok_or("Account disconnected")?
                    .email
                    .as_bytes()
            )
            .collect::<String>(),
            item.message.conversation_id
        )
    } else {
        let url = item.message.metadata["webLink"]
            .as_str()
            .ok_or("Original message link unavailable")?;
        let parsed = url::Url::parse(url).map_err(|_| "Invalid original message link")?;
        if parsed.scheme() != "https"
            || !parsed.host_str().is_some_and(|h| {
                h == "outlook.office.com" || h == "outlook.live.com" || h == "outlook.office365.com"
            })
        {
            return Err("Unexpected original message link".into());
        }
        url.into()
    };
    webbrowser::open(&url).map_err(|e| e.to_string())
}
#[tauri::command]
async fn reconcile_reply(state: State<'_, Shared>, id: String) -> Result<(), String> {
    use connectors::CommunicationConnector;
    let _send = state.send_lock.lock().await;
    let (mut item, account) = {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        let item = s
            .get::<Approval>("approval", &id)?
            .ok_or("Approval no longer exists")?;
        let account = s
            .get::<Account>("account", &item.message.account_id)?
            .ok_or("Account disconnected")?;
        (item, account)
    };
    if item.status != "uncertain" {
        return Err("Only uncertain sends need reconciliation".into());
    }
    let connector = connectors::MailConnector {
        state: state.inner().clone(),
    };
    if let Some(provider_id) = connector.reconcile(&account, &item).await? {
        let s = state.store.lock().map_err(|_| "Storage busy")?;
        item.status = "sent".into();
        item.send_error = None;
        let e = engine::event(
            &item,
            "approved",
            "Delivery verified against the provider after an uncertain outcome.",
            true,
            Some(provider_id.clone()),
        );
        s.finish_reply(&item, &e, Some(&provider_id))?;
    } else {
        return Err("Delivery could not be confirmed. Tomo will keep the reply blocked to avoid duplicates. Check the original inbox before taking further action.".into());
    }
    engine::updated(&state);
    Ok(())
}
#[tauri::command]
fn show_main(app: tauri::AppHandle, id: Option<String>) -> Result<(), String> {
    use tauri::Emitter;
    let _ = app.emit("tomo:review", id);
    if let Some(p) = app.get_webview_window("popover") {
        let _ = p.hide();
    }
    if let Some(w) = app.get_webview_window("main") {
        w.show().map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("tomo=info")
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            use tauri_plugin_deep_link::DeepLinkExt;
            app.deep_link().on_open_url(|event| {
                for url in event.urls() {
                    auth::open_url(url.as_str());
                }
            });
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let key = storage::database_key(&dir).map_err(std::io::Error::other)?;
            let store = Store::open(&dir.join("tomo.db"), &key).map_err(std::io::Error::other)?;
            let earlier_items = ["tokens:siwc", "openai-api-key", "google-client-secret"]
                .into_iter()
                .map(String::from)
                .chain(
                    store
                        .list::<Account>("account")
                        .unwrap_or_default()
                        .into_iter()
                        .map(|a| format!("tokens:{}", a.id)),
                )
                .collect::<Vec<_>>();
            storage::open_vault(&dir, &key, &earlier_items).map_err(std::io::Error::other)?;
            for mut a in store
                .list::<Approval>("approval")
                .map_err(std::io::Error::other)?
            {
                if a.status == "sending" || a.status == "executing" {
                    a.status = "uncertain".into();
                    for action in &mut a.plan {
                        if action.status == "running" {
                            action.status = "uncertain".into();
                        }
                    }
                    a.send_error = Some(
                        "The app restarted during a send. Reconcile its outcome before retrying."
                            .into(),
                    );
                    store
                        .put("approval", &a.id, &a)
                        .map_err(std::io::Error::other)?;
                }
            }
            let state = Arc::new(AppState {
                store: Mutex::new(store),
                http: reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(90))
                    .redirect(reqwest::redirect::Policy::none())
                    .user_agent("Tomo/0.1.0")
                    .build()?,
                refresh_lock: tokio::sync::Mutex::new(()),
                auth_lock: tokio::sync::Mutex::new(()),
                send_lock: tokio::sync::Mutex::new(()),
                plan_lock: tokio::sync::Mutex::new(()),
                sync_lock: tokio::sync::Mutex::new(()),
                syncing: AtomicBool::new(false),
                last_error: Mutex::new(None),
                working: Mutex::new(Default::default()),
                notifications: Mutex::new(std::collections::HashMap::new()),
                app: OnceLock::new(),
            });
            let _ = state.app.set(app.handle().clone());
            app.manage(state.clone());
            let open = MenuItem::with_id(app, "open", "Open Tomo", true, None::<&str>)?;
            let pause = MenuItem::with_id(
                app,
                "pause",
                "Pause processing / Resume",
                true,
                None::<&str>,
            )?;
            let stop = MenuItem::with_id(
                app,
                "stop",
                "Stop all automatic replies",
                true,
                None::<&str>,
            )?;
            let quit = MenuItem::with_id(app, "quit", "Quit Tomo", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &pause, &stop, &quit])?;
            WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("index.html#popover".into()))
                .title("Tomo")
                .inner_size(360., 600.)
                .resizable(false)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .visible(false)
                .build()?;
            TrayIconBuilder::with_id("tomo")
                .icon(tauri::image::Image::from_bytes(include_bytes!(
                    "../icons/tray.png"
                ))?)
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        let _ = show_main(app.clone(), None);
                    }
                    "quit" => app.exit(0),
                    "pause" | "stop" => {
                        let state = app.state::<Shared>();
                        if let Ok(store) = state.store.lock() {
                            if let Ok(mut settings) = store.settings() {
                                settings.auto_send = false;
                                if event.id.as_ref() == "pause" {
                                    settings.paused = !settings.paused;
                                }
                                let _ = store.put("settings", "current", &settings);
                            }
                        }
                        engine::updated(&state);
                    }
                    _ => (),
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("popover") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let position = rect
                                    .position
                                    .to_physical::<f64>(window.scale_factor().unwrap_or(1.));
                                let size = rect
                                    .size
                                    .to_physical::<f64>(window.scale_factor().unwrap_or(1.));
                                let scale = window.scale_factor().unwrap_or(1.);
                                let _ = window.set_position(tauri::PhysicalPosition::new(
                                    (position.x + size.width / 2. - 180. * scale) as i32,
                                    (position.y + size.height) as i32,
                                ));
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;
            // Signing in is the consent to AI processing; this covers sign-ins made before that rule.
            if let Ok(store) = state.store.lock() {
                if let Ok(mut settings) = store.settings() {
                    let connected = if settings.ai_provider == "siwc" {
                        auth::load("siwc").ok().flatten().is_some()
                    } else {
                        storage::credential_get("openai-api-key")
                            .ok()
                            .flatten()
                            .is_some()
                    };
                    if connected && !settings.consent {
                        settings.consent = true;
                        let _ = store.put("settings", "current", &settings);
                    }
                }
            }
            engine::start(state.clone());
            slack::start(state.clone());
            engine::updated(&state);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(false) if window.label() == "popover" => {
                let _ = window.hide();
            }
            _ => (),
        })
        .invoke_handler(tauri::generate_handler![
            workspace::import_google_client,
            codex::choose_codex_dir,
            workspace::connect_workspace_tool,
            style::save_writing_style,
            style::save_rules,
            style::show_style_file,
            style::learn_writing_style,
            plans::revise_plan,
            plans::retry_plan,
            plans::approve_plan,
            get_snapshot,
            get_cached_messages,
            get_message,
            open_codex_file,
            save_settings,
            set_processing_paused,
            set_auto_send,
            ask_next_time,
            save_draft,
            approve_reply,
            dismiss_reply,
            redraft_reply,
            sync_now,
            connect_account,
            connect_slack,
            set_up_slack_app,
            slack_manifest,
            allow_mail_changes,
            connect_ai,
            list_models,
            save_api_key,
            disconnect_ai,
            disconnect_account,
            clear_history,
            reset_app,
            export_configuration,
            open_original,
            open_setup_page,
            reconcile_reply,
            show_main
        ])
        .run(tauri::generate_context!())
        .expect("Tomo could not start");
}
