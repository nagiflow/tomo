//! Slack, through an app the user creates in their own workspace. Tomo acts with that app's user
//! token, so it reads and answers direct messages as the user and needs no workspace admin (unless
//! the workspace requires app approval). An app built inside its own workspace also keeps Slack's
//! normal rate limits, which a single app shared between workspaces would not.
//!
//! Messages arrive two ways. Socket Mode pushes every message the user can see as it is posted,
//! which is how channel mentions and thread replies are found; it needs the app-level token. A
//! slower check of direct messages catches up on what arrived while Tomo was not running.
use crate::{auth, engine, models::*, storage, AppState};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, OnceLock},
};

/// The app the user creates: user scopes only, no bot, never distributed.
pub const MANIFEST: &str = r#"{"display_information":{"name":"Tomo","description":"Lets Tomo read and answer your Slack messages as you."},"oauth_config":{"redirect_urls":["tomo://slack/callback"],"pkce_enabled":true,"scopes":{"user":["im:history","im:read","im:write","mpim:history","mpim:read","mpim:write","channels:history","groups:history","chat:write","files:write","users:read","channels:read","groups:read","search:read"]}},"settings":{"event_subscriptions":{"user_events":["message.im","message.mpim","message.channels","message.groups"]},"org_deploy_enabled":false,"socket_mode_enabled":true,"token_rotation_enabled":false}}"#;

async fn call(
    state: &AppState,
    token: &str,
    method: &str,
    params: &[(&str, String)],
) -> Result<Value, String> {
    for _ in 0..3 {
        let response = state
            .http
            .post(format!("https://slack.com/api/{method}"))
            .bearer_auth(token)
            .form(params)
            .send()
            .await
            .map_err(|_| "Network unavailable. Messages remain saved locally.")?;
        // A rate-limited call was not carried out, so waiting and repeating it is safe even for a send.
        if response.status().as_u16() == 429 {
            let wait = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(30);
            tokio::time::sleep(std::time::Duration::from_secs(wait.min(60))).await;
            continue;
        }
        let v: Value = response
            .json()
            .await
            .map_err(|_| "Slack returned an invalid response")?;
        if v["ok"] == true {
            return Ok(v);
        }
        let code = v["error"].as_str().unwrap_or("unknown_error");
        return Err(match code {
            // "Session expired" is what marks the account as needing a new sign-in
            "invalid_auth" | "not_authed" | "token_revoked" | "token_expired"
            | "account_inactive" => {
                format!("Session expired ({code}). Paste a new Slack token in Settings.")
            }
            "missing_scope" => format!(
                "Your Slack app lacks the {} permission. Add it under OAuth & Permissions, reinstall the app, then paste the new token with Update token in Settings.",
                v["needed"].as_str().unwrap_or("required")
            ),
            _ => format!("Slack error: {code}."),
        });
    }
    Err("Slack is rate limiting Tomo. It will try again later.".into())
}
/// Where Slack sends the browser back after a sign-in. Slack's settings accept only https or a custom
/// scheme, and a custom scheme requires PKCE (enabled in the manifest); macOS hands it to Tomo.
pub const REDIRECT: &str = "tomo://slack/callback";
/// The user scopes Tomo asks for when signing in, the same ones the manifest lists.
pub fn user_scopes() -> String {
    serde_json::from_str::<Value>(MANIFEST).unwrap()["oauth_config"]["scopes"]["user"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(",")
}
/// Saves the Slack app every workspace signs in with: its client ID, and the app-level token that
/// pushes their messages to Tomo. One app serves any number of workspaces.
pub async fn save_app(state: &AppState, client_id: &str, app: &str) -> Result<(), String> {
    let (client_id, app) = (client_id.trim(), app.trim());
    if client_id.is_empty() || !client_id.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(
            "Paste the Client ID from Basic Information, digits with a dot in the middle.".into(),
        );
    }
    if !app.is_empty() {
        if !app.starts_with("xapp-") {
            return Err("The App-Level Token starts with xapp-. Generate one under Basic Information with the connections:write scope.".into());
        }
        call(state, app, "apps.connections.open", &[]).await?;
        storage::credential_set("slack-app-token", app)?;
    }
    let store = state.store.lock().map_err(|_| "Storage busy")?;
    let mut settings = store.settings()?;
    settings.slack_client_id = client_id.into();
    store.put("settings", "current", &settings)
}
/// The user's own Slack ID and workspace ID, which the account ID carries: slack:TEAM:USER.
fn ids(a: &Account) -> (&str, &str) {
    let mut parts = a.id.split(':').skip(1);
    (parts.next().unwrap_or(""), parts.next().unwrap_or(""))
}
fn seconds(ts: &str) -> f64 {
    ts.parse().unwrap_or(0.0)
}
/// Slack escapes exactly these three characters in message text.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
/// Where a message sits: its channel, and the thread it belongs to if it is a reply in one.
/// A conversation ID is the channel alone, or CHANNEL:THREAD for a thread.
fn place(m: &Message) -> (&str, Option<&str>) {
    match m.conversation_id.split_once(':') {
        Some((channel, thread)) => (channel, Some(thread)),
        None => (&m.conversation_id, None),
    }
}
/// The thread a reply to this message goes in. Direct messages are answered in the conversation
/// itself; a message in a channel is answered in a thread under it, never out in the channel.
fn reply_thread(m: &Message) -> Option<&str> {
    place(m).1.or_else(|| {
        (m.metadata["dm"] == false)
            .then(|| m.metadata["providerId"].as_str())
            .flatten()
    })
}
/// Posts a new message in a channel the user belongs to, named with or without # or given by ID.
/// Plans use it to pass on what came in elsewhere, such as an email the team should see. With
/// several workspaces connected, `workspace` (its name) picks one; without it the channel must
/// exist in only one of them.
pub async fn post(
    state: &AppState,
    workspace: Option<&str>,
    channel: &str,
    text: &str,
    files: Vec<(String, Vec<u8>)>,
) -> Result<Value, String> {
    let name = channel.trim().trim_start_matches('#');
    let accounts: Vec<Account> = state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .list::<Account>("account")?
        .into_iter()
        .filter(|a| a.provider == "slack" && a.status == "connected")
        .filter(|a| workspace.is_none_or(|w| in_workspace(a, w)))
        .collect();
    if accounts.is_empty() {
        return Err(match workspace {
            Some(w) => format!("No connected Slack workspace is named {w}."),
            None => "Connect Slack in Settings before Tomo posts there.".into(),
        });
    }
    let (mut found, mut failure) = (vec![], None);
    for a in &accounts {
        let token = auth::access(state, &a.id, "slack").await?;
        match find_channel(state, &token, name).await {
            Ok(Some(id)) => found.push((a, token, id)),
            Ok(None) => {}
            Err(e) => failure = Some(e),
        }
    }
    let (token, id) = match found.as_slice() {
        [(_, token, id)] => (token.clone(), id.clone()),
        [] => {
            return Err(failure
                .unwrap_or_else(|| format!("You are not a member of a Slack channel named #{name}.")))
        }
        several => {
            return Err(format!(
                "#{name} exists in several workspaces ({}). Name the workspace in your rule or instruction.",
                several
                    .iter()
                    .map(|(a, _, _)| workspace_name(a))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    };
    // Uploaded first, where nobody sees them, so a refused file stops the post before anything shows.
    let uploaded = if files.is_empty() {
        vec![]
    } else {
        upload(state, &token, files).await?
    };
    // markdown_text takes standard Markdown, so a table or **bold** from the plan shows as written.
    // "<!" is broken up so text from an email can never ping @channel or @here.
    let sent = call(
        state,
        &token,
        "chat.postMessage",
        &[
            ("channel", id.clone()),
            ("markdown_text", text.replace("<!", "<\u{200B}!")),
        ],
    )
    .await?;
    if !uploaded.is_empty() {
        // the files go right under the post, as their own message
        call(
            state,
            &token,
            "files.completeUploadExternal",
            &[
                ("channel_id", id.clone()),
                ("files", json!(uploaded).to_string()),
            ],
        )
        .await?;
    }
    let ts = sent["ts"].as_str().unwrap_or("").to_string();
    // The link only makes the step clickable in Tomo; the post already went out without it.
    let url = call(
        state,
        &token,
        "chat.getPermalink",
        &[("channel", id.clone()), ("message_ts", ts.clone())],
    )
    .await
    .ok()
    .and_then(|v| v["permalink"].as_str().map(str::to_string));
    Ok(json!({"channel":id,"ts":ts,"url":url}))
}
/// The workspace name the account row shows ("you · Workspace").
pub fn workspace_name(a: &Account) -> &str {
    a.email.rsplit(" · ").next().unwrap_or(&a.email)
}
fn in_workspace(a: &Account, w: &str) -> bool {
    let w = w.trim();
    w.is_empty() || workspace_name(a).eq_ignore_ascii_case(w) || ids(a).0 == w
}
/// The ID of a channel the user is in, by name or ID, or None when this workspace has no such channel.
async fn find_channel(state: &AppState, token: &str, name: &str) -> Result<Option<String>, String> {
    if is_channel_id(name) {
        return match call(
            state,
            token,
            "conversations.info",
            &[("channel", name.into())],
        )
        .await
        {
            Ok(_) => Ok(Some(name.into())),
            Err(e) if e.contains("channel_not_found") => Ok(None),
            Err(e) => Err(e),
        };
    }
    let mut page = String::new();
    loop {
        let list = call(
            state,
            token,
            "users.conversations",
            &[
                ("types", "public_channel,private_channel".into()),
                ("exclude_archived", "true".into()),
                ("limit", "200".into()),
                ("cursor", page.clone()),
            ],
        )
        .await?;
        let found = list["channels"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| {
                c["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .and_then(|c| c["id"].as_str().map(str::to_string));
        page = list["response_metadata"]["next_cursor"]
            .as_str()
            .unwrap_or("")
            .to_string();
        if found.is_some() || page.is_empty() {
            return Ok(found);
        }
    }
}
/// A Slack channel ID (C… or G…, upper-case letters and digits), as opposed to a channel name.
fn is_channel_id(s: &str) -> bool {
    s.len() >= 9
        && (s.starts_with('C') || s.starts_with('G'))
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}
/// The reply as Slack will show it: in a channel it opens by mentioning the person who asked,
/// so they are notified. Direct messages need no mention.
fn reply_text(m: &Message, content: &str) -> String {
    let id = &m.sender.id;
    if m.metadata["dm"] == false && !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())
    {
        format!("<@{id}> {}", escape(content))
    } else {
        escape(content)
    }
}
/// The app-level token that pushes this account's messages: one pasted with its own user token, or
/// else the one of the app set up in Settings.
fn app_token(account: &str) -> Result<Option<String>, String> {
    Ok(
        storage::credential_get(&format!("slack-app-token:{account}"))?
            .or(storage::credential_get("slack-app-token")?),
    )
}
/// Threads Tomo has taken a message from, so later replies in them count even without a mention.
// ponytail: kept in memory, so a thread started by someone else is forgotten when Tomo restarts
// and needs a fresh mention; rebuild this from stored messages at startup if that bites.
fn followed() -> &'static Mutex<HashSet<String>> {
    static FOLLOWED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    FOLLOWED.get_or_init(Default::default)
}
/// Whether a pushed message is the user's business: any direct message, and in channels a mention
/// of them, a reply in a thread they started, or a reply in a thread Tomo is already following.
fn relevant(e: &Value, me: &str, following: bool) -> bool {
    written(e)
        && e["user"] != me
        && (["im", "mpim"].contains(&e["channel_type"].as_str().unwrap_or(""))
            || e["text"]
                .as_str()
                .is_some_and(|t| t.contains(&format!("<@{me}>")))
            || e["parent_user_id"] == me
            || following)
}
/// Something a person or an app wrote, as opposed to "joined the conversation" and other notices.
fn written(v: &Value) -> bool {
    v["ts"].is_string()
        && match v["subtype"].as_str() {
            None => true,
            Some(s) => [
                "file_share",
                "thread_broadcast",
                "me_message",
                "bot_message",
            ]
            .contains(&s),
        }
}
/// Display names, looked up once per sync or thread.
async fn name(
    state: &AppState,
    token: &str,
    names: &mut HashMap<String, String>,
    user: &str,
) -> String {
    if let Some(known) = names.get(user) {
        return known.clone();
    }
    let found = call(state, token, "users.info", &[("user", user.into())])
        .await
        .ok()
        .and_then(|v| {
            [&v["user"]["real_name"], &v["user"]["name"]]
                .into_iter()
                .find_map(|n| n.as_str().filter(|n| !n.is_empty()).map(str::to_string))
        })
        .unwrap_or_else(|| user.to_string());
    names.insert(user.into(), found.clone());
    found
}
async fn normalize(
    state: &AppState,
    token: &str,
    names: &mut HashMap<String, String>,
    a: &Account,
    channel: &str,
    dm: bool,
    v: &Value,
) -> Message {
    let ts = v["ts"].as_str().unwrap_or("");
    let mention = v["text"]
        .as_str()
        .is_some_and(|t| t.contains(&format!("<@{}>", ids(a).1)));
    let app = !v["bot_id"].is_null() || v["user"] == "USLACKBOT";
    let (sender, sender_name) = match v["user"].as_str() {
        Some(user) => (user.to_string(), name(state, token, names, user).await),
        None => (
            v["bot_id"].as_str().unwrap_or("app").to_string(),
            v["username"].as_str().unwrap_or("Slack app").to_string(),
        ),
    };
    Message {
        id: format!("{}:{channel}:{ts}", a.id),
        provider: "slack".into(),
        account_id: a.id.clone(),
        // the first message of a thread carries its own timestamp as the thread's
        conversation_id: match v["thread_ts"].as_str().filter(|t| *t != ts) {
            Some(thread) => format!("{channel}:{thread}"),
            None => channel.into(),
        },
        sender: Sender {
            id: sender,
            name: sender_name,
            address: None,
        },
        content: unescape(v["text"].as_str().unwrap_or(""))
            .chars()
            .take(20000)
            .collect(),
        timestamp: chrono::DateTime::from_timestamp(seconds(ts) as i64, 0)
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(now),
        is_direct_message: dm,
        is_mention: mention,
        requires_processing: true,
        metadata: json!({"providerId":ts,"subject":if dm { "Slack message" } else if mention { "Slack mention" } else { "Slack thread" },"automated":app,"dm":dm}),
    }
}
/// Checks the pasted tokens and records the workspace as an account. The app-level token is what
/// lets Slack push messages to Tomo; without it only direct messages are found, by checking.
/// Saves a workspace's user token. `rotation` is (refresh token, seconds until expiry, client ID) for
/// a token from sign-in, which Slack rotates; a pasted token never expires and has none.
pub async fn connect(
    state: &AppState,
    token: &str,
    app: &str,
    rotation: Option<(String, i64, String)>,
) -> Result<(), String> {
    let (token, app) = (token.trim(), app.trim());
    if !app.is_empty() {
        if !app.starts_with("xapp-") {
            return Err("The App-Level Token starts with xapp-. Generate one under Basic Information with the connections:write scope.".into());
        }
        call(state, app, "apps.connections.open", &[]).await?;
    }
    if !token.starts_with("xoxp-") && !token.starts_with("xoxe.xoxp-") {
        return Err("Paste the User OAuth Token, which starts with xoxp-. A bot token (xoxb-) cannot read your direct messages.".into());
    }
    let me = call(state, token, "auth.test", &[]).await?;
    let id = format!(
        "slack:{}:{}",
        me["team_id"]
            .as_str()
            .ok_or("Slack did not name the workspace")?,
        me["user_id"]
            .as_str()
            .ok_or("Slack did not name the user")?
    );
    auth::save(
        &id,
        &auth::Tokens {
            access_token: token.into(),
            refresh_token: rotation.as_ref().map(|r| r.0.clone()),
            id_token: None,
            // a pasted token of an app without token rotation does not expire
            expires_at: rotation
                .as_ref()
                .map_or(i64::MAX / 2, |r| chrono::Utc::now().timestamp() + r.1),
            client_id: rotation.map(|r| r.2).unwrap_or_default(),
            subject: me["user_id"].as_str().map(str::to_string),
            email: None,
            scope: String::new(),
            earliest_refresh_at: None,
        },
    )?;
    if !app.is_empty() {
        storage::credential_set(&format!("slack-app-token:{id}"), app)?;
    }
    let store = state.store.lock().map_err(|_| "Storage busy")?;
    let account = Account {
        id: id.clone(),
        provider: "slack".into(),
        email: format!(
            "{} · {}",
            me["user"].as_str().unwrap_or("you"),
            me["team"].as_str().unwrap_or("Slack")
        ),
        status: "connected".into(),
        error: None,
        // Starts from now: Tomo handles what arrives after connecting, not the backlog. A token
        // pasted again for the same workspace carries on from where it was.
        cursor: store
            .get::<Account>("account", &id)?
            .and_then(|a| a.cursor)
            .or_else(|| Some(format!("{}.000000", chrono::Utc::now().timestamp()))),
        last_sync: None,
        retry_at: None,
        failures: 0,
    };
    store.put("account", &id, &account)
}
/// Catches up on what Slack did not push while Tomo was closed or offline: one search for messages
/// since the last check, newest first, of which Tomo keeps the DMs and mentions of the user. The live
/// connection delivers the same messages as they arrive; those already stored are ignored by the caller.
pub async fn fetch_new(state: &AppState, a: &Account) -> Result<(Vec<Message>, String), String> {
    let token = auth::access(state, &a.id, "slack").await?;
    let me = ids(a).1;
    // The next check starts two minutes before this one began, so a message that lands while the
    // search runs is not skipped.
    let checkpoint = format!("{}.000000", chrono::Utc::now().timestamp() - 120);
    let since = seconds(a.cursor.as_deref().unwrap_or(&checkpoint));
    // `after:` counts whole days in the user's own time zone and leaves out the day it names, so it
    // names a day two days back; the exact cut is made below by timestamp.
    let day = chrono::DateTime::from_timestamp(since as i64 - 2 * 86_400, 0)
        .unwrap_or_default()
        .format("%Y-%m-%d");
    let mut found = vec![];
    // ponytail: at most 10 pages (1,000 messages) per check; after a very long gap in a busy workspace the oldest are left out
    for page in 1..=10u64 {
        let v = call(
            state,
            &token,
            "search.messages",
            &[
                ("query", format!("after:{day}")),
                ("sort", "timestamp".into()),
                ("sort_dir", "desc".into()),
                ("count", "100".into()),
                ("page", page.to_string()),
            ],
        )
        .await?;
        let matches = v["messages"]["matches"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let reached = matches
            .last()
            .is_none_or(|m| seconds(m["ts"].as_str().unwrap_or("0")) <= since);
        found.extend(matches);
        if reached || page >= v["messages"]["paging"]["pages"].as_u64().unwrap_or(1) {
            break;
        }
    }
    let mut names = HashMap::new();
    let mut messages = vec![];
    for (channel, dm, e) in missed(&found, me, since) {
        messages.push(normalize(state, &token, &mut names, a, &channel, dm, &e).await);
    }
    Ok((messages, checkpoint))
}
/// From search results, the messages the live feed would have taken: direct messages and mentions
/// of the user since `since`, leaving out a direct message the user has since answered there.
/// Each comes back with its channel, whether it is a direct message, and the message itself.
fn missed(matches: &[Value], me: &str, since: f64) -> Vec<(String, bool, Value)> {
    let dm = |m: &Value| {
        m["type"] == "im" || m["channel"]["is_im"] == true || m["channel"]["is_mpim"] == true
    };
    let mut answered: HashMap<&str, f64> = HashMap::new();
    for m in matches.iter().filter(|m| m["user"] == me) {
        if let Some(channel) = m["channel"]["id"].as_str() {
            let ts = seconds(m["ts"].as_str().unwrap_or(""));
            let latest = answered.entry(channel).or_default();
            *latest = latest.max(ts);
        }
    }
    matches
        .iter()
        .filter_map(|m| {
            let channel = m["channel"]["id"].as_str()?;
            let ts = seconds(m["ts"].as_str()?);
            let dm = dm(m);
            let mut e = m.clone();
            e["channel_type"] = json!(if dm { "im" } else { "channel" });
            // a reply in a thread says so only in its link
            if let Some(thread) = m["permalink"]
                .as_str()
                .and_then(|l| url::Url::parse(l).ok())
                .and_then(|u| {
                    u.query_pairs()
                        .find(|(k, _)| k == "thread_ts")
                        .map(|(_, v)| v.into_owned())
                })
            {
                e["thread_ts"] = json!(thread);
            }
            let replied = dm && answered.get(channel).is_some_and(|r| *r > ts);
            (ts > since && !replied && relevant(&e, me, false))
                .then(|| (channel.to_string(), dm, e))
        })
        .collect()
}
/// The message and the five before it, oldest first: from its thread, or from the conversation.
pub async fn fetch_conversation(
    state: &AppState,
    a: &Account,
    m: &Message,
) -> Result<Vec<Message>, String> {
    let token = auth::access(state, &a.id, "slack").await?;
    let (channel, thread) = place(m);
    let mut params = vec![
        ("channel", channel.to_string()),
        (
            "latest",
            m.metadata["providerId"].as_str().unwrap_or("").into(),
        ),
        ("inclusive", "true".into()),
    ];
    // A thread lists oldest first and the conversation newest first.
    let mut found = match thread {
        Some(thread) => {
            params.extend([("ts", thread.to_string()), ("limit", "200".into())]);
            call(state, &token, "conversations.replies", &params).await?
        }
        None => {
            params.push(("limit", "6".into()));
            call(state, &token, "conversations.history", &params).await?
        }
    }["messages"]
        .as_array()
        .cloned()
        .ok_or("Missing Slack conversation")?;
    if thread.is_none() {
        found.reverse();
    }
    let dm = m.metadata["dm"] != false;
    let mut names = HashMap::new();
    let mut context = vec![];
    for v in found
        .iter()
        .filter(|v| written(v))
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        context.push(normalize(state, &token, &mut names, a, channel, dm, v).await);
    }
    Ok(context)
}
pub async fn send_reply(
    state: &AppState,
    a: &Account,
    item: &Approval,
    content: &str,
) -> Result<String, String> {
    // Checked before anything is uploaded or sent: a refused attachment stops the reply.
    let files = crate::codex::attachments(state, item)?;
    let token = auth::access(state, &a.id, "slack").await?;
    let channel = place(&item.message).0.to_string();
    let mut params = vec![];
    if let Some(thread) = reply_thread(&item.message) {
        params.push(("thread_ts", thread.to_string()));
    }
    if files.is_empty() {
        params.extend([
            ("channel", channel),
            ("text", reply_text(&item.message, content)),
        ]);
        let v = call(state, &token, "chat.postMessage", &params).await?;
        return Ok(v["ts"]
            .as_str()
            .ok_or("Send response missing message ID; reconcile first")?
            .into());
    }
    let uploaded = upload(state, &token, files).await?;
    params.extend([
        ("channel_id", channel),
        ("initial_comment", reply_text(&item.message, content)),
        ("files", json!(uploaded).to_string()),
    ]);
    let v = call(state, &token, "files.completeUploadExternal", &params).await?;
    Ok(v["files"][0]["id"]
        .as_str()
        .ok_or("Send response missing file ID; reconcile first")?
        .into())
}
/// After an uncertain send: whether the user's own messages since the original include this reply.
pub async fn reconcile(
    state: &AppState,
    a: &Account,
    item: &Approval,
) -> Result<Option<String>, String> {
    let token = auth::access(state, &a.id, "slack").await?;
    let mut params = vec![
        ("channel", place(&item.message).0.to_string()),
        (
            "oldest",
            item.message.metadata["providerId"]
                .as_str()
                .unwrap_or("")
                .into(),
        ),
        ("limit", "50".into()),
    ];
    let found = match reply_thread(&item.message) {
        Some(thread) => {
            params.push(("ts", thread.to_string()));
            call(state, &token, "conversations.replies", &params).await?
        }
        None => call(state, &token, "conversations.history", &params).await?,
    };
    Ok(found["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|v| {
            v["user"] == ids(a).1
                && unescape(v["text"].as_str().unwrap_or("")).trim() == item.draft.trim()
        })
        .and_then(|v| v["ts"].as_str())
        .map(str::to_string))
}
/// Uploads files where nobody can see them yet; files.completeUploadExternal then posts them
/// together with the message they belong to.
async fn upload(
    state: &AppState,
    token: &str,
    files: Vec<(String, Vec<u8>)>,
) -> Result<Vec<Value>, String> {
    let mut uploaded = vec![];
    for (name, bytes) in files {
        let slot = call(
            state,
            token,
            "files.getUploadURLExternal",
            &[
                ("filename", name.clone()),
                ("length", bytes.len().to_string()),
            ],
        )
        .await?;
        let url = url::Url::parse(slot["upload_url"].as_str().unwrap_or(""))
            .map_err(|_| "Slack gave no place to upload the file. Nothing was sent.")?;
        if url.scheme() != "https" || !url.host_str().is_some_and(|h| h.ends_with(".slack.com")) {
            return Err("Rejected an unexpected upload address. Nothing was sent.".into());
        }
        state
            .http
            .post(url)
            .body(bytes)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|_| format!("Could not upload {name}. Nothing was sent."))?;
        uploaded.push(json!({"id":slot["file_id"],"title":name}));
    }
    Ok(uploaded)
}
/// Marks the conversation read up to this message. Slack has no archive, so "archive" does the same.
/// Channels are left alone: marking one read would hide everything else posted there.
pub async fn tidy(state: &AppState, a: &Account, m: &Message) -> Result<(), String> {
    if m.metadata["dm"] == false {
        return Ok(());
    }
    let token = auth::access(state, &a.id, "slack").await?;
    call(
        state,
        &token,
        "conversations.mark",
        &[
            ("channel", place(m).0.to_string()),
            ("ts", m.metadata["providerId"].as_str().unwrap_or("").into()),
        ],
    )
    .await
    .map(|_| ())
}
/// Opens the conversation in Slack.
pub fn link(a: &Account, m: &Message) -> String {
    format!(
        "https://slack.com/app_redirect?team={}&channel={}",
        ids(a).0,
        place(m).0
    )
}
/// The connected Slack accounts and the app-level token each one's messages arrive through.
fn slack_accounts(state: &AppState) -> Vec<(Account, String)> {
    state
        .store
        .lock()
        .ok()
        .and_then(|s| s.list::<Account>("account").ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|a| a.provider == "slack")
        .filter_map(|a| Some((app_token(&a.id).ok()??, a)))
        .map(|(app, a)| (a, app))
        .collect()
}
/// Keeps one live connection open per Slack app. Several workspaces can share an app, and Slack
/// hands each event to just one of an app's connections, so one connection serves them all.
pub fn start(state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let live: Arc<Mutex<HashSet<String>>> = Default::default();
        loop {
            for (_, app) in slack_accounts(&state) {
                // A connection that ended, for any reason, is opened again on the next pass.
                if !live.lock().is_ok_and(|mut l| l.insert(app.clone())) {
                    continue;
                }
                let (state, live) = (state.clone(), live.clone());
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = listen(&state, &app).await {
                        tracing::info!("Slack live connection: {e}");
                    }
                    if let Ok(mut l) = live.lock() {
                        l.remove(&app);
                    }
                });
            }
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
        }
    });
}
/// Reads pushed messages until the connection ends, storing each one that concerns the user under
/// the account of the workspace it came from.
async fn listen(state: &Arc<AppState>, app: &str) -> Result<(), String> {
    let opened = call(state, app, "apps.connections.open", &[]).await?;
    let (mut socket, _) =
        tokio_tungstenite::connect_async(opened["url"].as_str().ok_or("Slack gave no connection")?)
            .await
            .map_err(|e| e.to_string())?;
    let mut names = HashMap::new();
    while let Some(frame) = socket.next().await {
        let tokio_tungstenite::tungstenite::Message::Text(text) =
            frame.map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        // Slack sends an event again unless it is acknowledged within three seconds.
        if let Some(envelope) = v["envelope_id"].as_str() {
            socket
                .send(json!({"envelope_id":envelope}).to_string().into())
                .await
                .map_err(|e| e.to_string())?;
        }
        if v["type"] == "disconnect" {
            break;
        }
        let e = &v["payload"]["event"];
        let Some(channel) = e["channel"].as_str() else {
            continue;
        };
        if v["type"] != "events_api" || e["type"] != "message" {
            continue;
        }
        // Read again for every event: workspaces are added and removed while the connection is open.
        let accounts = slack_accounts(state);
        if !accounts.iter().any(|(_, t)| t == app) {
            return Ok(());
        }
        let team = v["payload"]["team_id"].as_str().unwrap_or("");
        let Some((a, _)) = accounts.iter().find(|(a, t)| t == app && ids(a).0 == team) else {
            continue;
        };
        let me = ids(a).1;
        let thread = format!(
            "{}:{channel}:{}",
            a.id,
            e["thread_ts"].as_str().or(e["ts"].as_str()).unwrap_or("")
        );
        let following = followed().lock().is_ok_and(|f| f.contains(&thread));
        if !relevant(e, me, following) {
            continue;
        }
        let token = auth::access(state, &a.id, "slack").await?;
        let dm = ["im", "mpim"].contains(&e["channel_type"].as_str().unwrap_or(""));
        let m = normalize(state, &token, &mut names, a, channel, dm, e).await;
        {
            let store = state.store.lock().map_err(|_| "Storage busy")?;
            if store.get::<Account>("account", &a.id)?.is_none() {
                continue;
            }
            if store.settings()?.paused || store.get::<Message>("message", &m.id)?.is_some() {
                continue;
            }
            store.put("message", &m.id, &m)?;
        }
        if let Ok(mut f) = followed().lock() {
            f.insert(thread);
        }
        // Handle it now instead of waiting for the next scheduled check.
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            let _ = engine::synchronize(state, false).await;
        });
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notices_are_skipped_and_text_round_trips() {
        assert!(written(&json!({"ts":"1.0","text":"hi"})));
        assert!(written(&json!({"ts":"1.0","subtype":"file_share"})));
        assert!(!written(&json!({"ts":"1.0","subtype":"channel_join"})));
        assert!(!written(&json!({"text":"no timestamp"})));
        let text = "a < b && <@U1> said \"go\"";
        assert_eq!(unescape(&escape(text)), text);
        assert_eq!(escape("<!channel>"), "&lt;!channel&gt;");
        let account = Account {
            id: "slack:T1:U1".into(),
            provider: "slack".into(),
            email: String::new(),
            status: "connected".into(),
            error: None,
            cursor: None,
            last_sync: None,
            retry_at: None,
            failures: 0,
        };
        assert_eq!(ids(&account), ("T1", "U1"));
        let mut named = account.clone();
        named.email = "maya · Acme Inc".into();
        assert_eq!(workspace_name(&named), "Acme Inc");
        assert!(in_workspace(&named, "acme inc") && in_workspace(&named, "T1"));
        assert!(!in_workspace(&named, "Other"));
    }
    #[test]
    fn catching_up_keeps_what_the_live_feed_would_have_taken() {
        let m = |channel: &str, kind: &str, ts: &str, user: &str, text: &str| {
            json!({"type":kind,"channel":{"id":channel,"is_im":kind=="im"},"ts":ts,"user":user,"text":text,
                "permalink":format!("https://x.slack.com/archives/{channel}/p1?thread_ts=9.0&cid={channel}")})
        };
        let found = vec![
            m("D1", "im", "30.0", "U2", "are you there?"),
            m("D2", "im", "29.0", "U3", "lunch?"),
            m("D2", "im", "28.5", "U1", "yes"),
            m("D2", "im", "28.0", "U3", "free today?"),
            m("C1", "message", "27.0", "U2", "ping <@U1>"),
            m("C1", "message", "26.0", "U2", "general chatter"),
            m("D1", "im", "5.0", "U2", "before the last check"),
        ];
        let kept = missed(&found, "U1", 10.0);
        let texts: Vec<&str> = kept
            .iter()
            .map(|(_, _, e)| e["text"].as_str().unwrap())
            .collect();
        // D2's "free today?" was answered with "yes"; chatter and the user's own message are not for Tomo
        assert_eq!(texts, ["are you there?", "lunch?", "ping <@U1>"]);
        assert_eq!((kept[2].0.as_str(), kept[2].1), ("C1", false));
        assert_eq!(kept[2].2["thread_ts"], "9.0");
    }
    #[test]
    fn sign_in_asks_for_the_manifest_scopes_and_redirect() {
        let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(manifest["oauth_config"]["redirect_urls"][0], REDIRECT);
        assert_eq!(manifest["oauth_config"]["pkce_enabled"], true);
        assert!(user_scopes().split(',').any(|s| s == "channels:read"));
        assert!(!user_scopes().contains(' '));
    }
    #[test]
    fn only_messages_that_concern_the_user_are_taken_from_the_live_feed() {
        let e = |extra: Value| {
            let mut v = json!({"ts":"2.0","user":"U2","text":"hello","channel_type":"channel"});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            v
        };
        assert!(!relevant(&e(json!({})), "U1", false));
        assert!(relevant(&e(json!({"channel_type":"im"})), "U1", false));
        assert!(relevant(&e(json!({"text":"ping <@U1>"})), "U1", false));
        assert!(!relevant(&e(json!({"text":"ping <@U3>"})), "U1", false));
        assert!(relevant(&e(json!({"parent_user_id":"U1"})), "U1", false));
        assert!(relevant(&e(json!({})), "U1", true));
        // never the user's own messages, and never edits or other notices
        assert!(!relevant(
            &e(json!({"user":"U1","channel_type":"im"})),
            "U1",
            false
        ));
        assert!(!relevant(
            &e(json!({"subtype":"message_changed","channel_type":"im"})),
            "U1",
            false
        ));
    }
    #[test]
    fn replies_go_in_the_thread_except_in_direct_messages() {
        let m = |conversation: &str, dm: bool| Message {
            id: "m".into(),
            provider: "slack".into(),
            account_id: "slack:T1:U1".into(),
            conversation_id: conversation.into(),
            sender: Sender {
                id: "U2".into(),
                name: "Maya".into(),
                address: None,
            },
            content: String::new(),
            timestamp: now(),
            is_direct_message: dm,
            is_mention: false,
            requires_processing: true,
            metadata: json!({"providerId":"5.0","dm":dm}),
        };
        assert_eq!(reply_thread(&m("D1", true)), None);
        assert_eq!(reply_thread(&m("D1:3.0", true)), Some("3.0"));
        assert_eq!(reply_thread(&m("C1", false)), Some("5.0"));
        assert_eq!(reply_thread(&m("C1:3.0", false)), Some("3.0"));
        assert_eq!(place(&m("C1:3.0", false)), ("C1", Some("3.0")));
        assert_eq!(
            reply_text(&m("C1", false), "Hi <there>"),
            "<@U2> Hi &lt;there&gt;"
        );
        assert_eq!(reply_text(&m("D1", true), "Hi"), "Hi");
        assert!(is_channel_id("C0B3YMLJTKM"));
        assert!(!is_channel_id("general"));
        assert!(!is_channel_id("CHANNELS"));
    }
}
