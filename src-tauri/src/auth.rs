use crate::{
    models::*,
    storage::{credential_get, credential_set},
    AppState,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_at: i64,
    pub client_id: String,
    pub subject: Option<String>,
    pub email: Option<String>,
    pub scope: String,
    pub earliest_refresh_at: Option<i64>,
}
/// The sign-in waiting for its tomo:// callback, if any.
fn waiting() -> &'static std::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>> {
    static WAITING: std::sync::OnceLock<
        std::sync::Mutex<Option<tokio::sync::oneshot::Sender<String>>>,
    > = std::sync::OnceLock::new();
    WAITING.get_or_init(Default::default)
}
/// Every tomo:// link macOS opens Tomo with. A Slack sign-in callback goes to the sign-in waiting
/// for it; anything else, or one nobody waits for, is ignored. Its state is checked there.
pub fn open_url(url: &str) {
    if let Some(query) = url.strip_prefix(crate::slack::REDIRECT) {
        if let Some(tx) = waiting().lock().ok().and_then(|mut w| w.take()) {
            let _ = tx.send(format!("/auth/callback{query}"));
        }
    }
}
pub fn random() -> String {
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
pub fn load(id: &str) -> Result<Option<Tokens>, String> {
    credential_get(&format!("tokens:{id}"))?
        .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
        .transpose()
}
pub fn save(id: &str, t: &Tokens) -> Result<(), String> {
    credential_set(
        &format!("tokens:{id}"),
        &serde_json::to_string(t).map_err(|e| e.to_string())?,
    )
}
pub fn endpoint(provider: &str) -> &'static str {
    match provider {
        "gmail" => "https://oauth2.googleapis.com/token",
        "outlook" => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        "slack" => "https://slack.com/api/oauth.v2.access",
        _ => "https://auth.openai.com/api/accounts/oauth/token",
    }
}
pub async fn response_json(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let retry = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("60")
        .to_string();
    let value: Value = response
        .json()
        .await
        .map_err(|_| format!("Provider returned an invalid response ({status})"))?;
    if !status.is_success() {
        let code = value["error"]["code"]
            .as_str()
            .or(value["error"].as_str())
            .unwrap_or("provider_error");
        return Err(format!(
            "Provider error {status}: {code}. {}",
            if status.as_u16() == 429 {
                format!("Rate limited; retry after {retry} seconds.")
            } else if status.as_u16() == 401 {
                "Sign in again.".into()
            } else if status.as_u16() == 403 {
                "Your account or organization may need to authorize this permission.".into()
            } else {
                "Try again after checking the connection.".into()
            }
        ));
    }
    Ok(value)
}
fn tokens(v: &Value, client_id: String, old: Option<&Tokens>) -> Result<Tokens, String> {
    Ok(Tokens {
        access_token: v["access_token"]
            .as_str()
            .ok_or("No access token returned")?
            .into(),
        refresh_token: v["refresh_token"]
            .as_str()
            .map(str::to_string)
            .or_else(|| old.and_then(|t| t.refresh_token.clone())),
        id_token: v["id_token"]
            .as_str()
            .map(str::to_string)
            .or_else(|| old.and_then(|t| t.id_token.clone())),
        expires_at: chrono::Utc::now().timestamp() + v["expires_in"].as_i64().unwrap_or(3600),
        client_id,
        subject: old.and_then(|t| t.subject.clone()),
        email: old.and_then(|t| t.email.clone()),
        scope: v["scope"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| old.map(|t| t.scope.clone()).unwrap_or_default()),
        earliest_refresh_at: v["earliest_refresh_at"].as_i64(),
    })
}
pub async fn access(state: &AppState, id: &str, provider: &str) -> Result<String, String> {
    let _guard = state.refresh_lock.lock().await;
    let old = load(id)?.ok_or("Not connected. Sign in again.")?;
    if old.expires_at > chrono::Utc::now().timestamp() + 120 {
        return Ok(old.access_token);
    }
    if old
        .earliest_refresh_at
        .is_some_and(|v| v > chrono::Utc::now().timestamp())
    {
        if old.expires_at > chrono::Utc::now().timestamp() {
            return Ok(old.access_token);
        }
        return Err("Session is expired; please reconnect.".into());
    }
    let refresh = old
        .refresh_token
        .clone()
        .ok_or("Session expired. Sign in again.")?;
    let mut form = vec![
        ("grant_type", "refresh_token".into()),
        ("refresh_token", refresh),
        ("client_id", old.client_id.clone()),
    ];
    if provider == "siwc" {
        form.push(("resource", "https://api.openai.com/v1".into()))
    }
    if provider == "gmail" {
        if let Some(secret) = credential_get("google-client-secret")? {
            form.push(("client_secret", secret))
        }
    }
    let v = response_json(
        state
            .http
            .post(endpoint(provider))
            .form(&form)
            .send()
            .await
            .map_err(|_| "Network unavailable while refreshing authorization")?,
    )
    .await?;
    let v = if provider == "slack" {
        // Slack answers 200 with ok false; a refused refresh means the 30-day sign-in has lapsed.
        if v["ok"] != true {
            return Err(format!(
                "Session expired ({}). Sign in to this Slack workspace again.",
                v["error"].as_str().unwrap_or("refresh refused")
            ));
        }
        if v["authed_user"]["access_token"].is_string() {
            v["authed_user"].clone()
        } else {
            v
        }
    } else {
        v
    };
    let t = tokens(&v, old.client_id.clone(), Some(&old))?;
    save(id, &t)?;
    Ok(t.access_token)
}
/// Records a mail account for freshly issued tokens: looks up its address, stores the tokens and the account.
pub async fn save_account(state: &AppState, provider: String, t: Tokens) -> Result<(), String> {
    let profile_url = if provider == "gmail" {
        "https://gmail.googleapis.com/gmail/v1/users/me/profile"
    } else {
        "https://graph.microsoft.com/v1.0/me?$select=id,mail,userPrincipalName"
    };
    let p = response_json(
        state
            .http
            .get(profile_url)
            .bearer_auth(&t.access_token)
            .send()
            .await
            .map_err(|_| "Could not load account profile")?,
    )
    .await?;
    let email = p["emailAddress"]
        .as_str()
        .or(p["mail"].as_str())
        .or(p["userPrincipalName"].as_str())
        .ok_or("No account email returned")?
        .to_string();
    let account_id = format!(
        "{}:{}",
        provider,
        URL_SAFE_NO_PAD.encode(Sha256::digest(email.to_lowercase().as_bytes()))
    );
    save(&account_id, &t)?;
    let account = Account {
        id: account_id.clone(),
        provider,
        email,
        status: "connected".into(),
        error: None,
        cursor: None,
        last_sync: None,
        retry_at: None,
        failures: 0,
    };
    state
        .store
        .lock()
        .map_err(|_| "Storage busy")?
        .put("account", &account_id, &account)?;
    if account.provider == "gmail" {
        let store = state.store.lock().map_err(|_| "Storage busy")?;
        let mut settings = store.settings()?;
        if settings.google_account_id.is_empty() {
            settings.google_account_id = account.id.clone();
            store.put("settings", "current", &settings)?;
        }
    }
    Ok(())
}
#[derive(Deserialize)]
struct Identity {
    sub: String,
    email: Option<String>,
    nonce: String,
}
async fn verify_identity(
    state: &AppState,
    token: &str,
    client: &str,
    nonce: &str,
) -> Result<Identity, String> {
    use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
    let header = decode_header(token).map_err(|_| "Invalid ID token")?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err("Unsupported identity signature".into());
    }
    let jwks: JwkSet = state
        .http
        .get("https://auth.openai.com/.well-known/jwks.json")
        .send()
        .await
        .map_err(|_| "Cannot load identity signing keys")?
        .error_for_status()
        .map_err(|_| "Cannot load identity signing keys")?
        .json()
        .await
        .map_err(|_| "Invalid identity signing keys")?;
    let jwk = jwks
        .find(header.kid.as_deref().ok_or("Missing signing key ID")?)
        .ok_or("Unknown identity signing key")?;
    let mut validation = Validation::new(header.alg);
    validation.set_issuer(&["https://auth.openai.com"]);
    validation.set_audience(&[client]);
    validation.set_required_spec_claims(&["sub", "exp", "iat", "iss", "aud"]);
    let identity = decode::<Identity>(
        token,
        &DecodingKey::from_jwk(jwk).map_err(|_| "Invalid signing key")?,
        &validation,
    )
    .map_err(|_| "Identity verification failed")?
    .claims;
    if identity.nonce != nonce || identity.sub.is_empty() {
        return Err("Identity nonce mismatch".into());
    }
    Ok(identity)
}
pub fn parse_callback(target: &str, state: &str) -> Result<HashMap<String, String>, String> {
    let u =
        url::Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| "Invalid callback")?;
    if u.path() != "/auth/callback" {
        return Err("Unexpected callback path".into());
    }
    let mut params = HashMap::new();
    for (k, v) in u.query_pairs() {
        if params.insert(k.into_owned(), v.into_owned()).is_some() {
            return Err("Duplicate callback parameter".into());
        }
    }
    if params.get("state").map(String::as_str) != Some(state) {
        return Err("Authorization state mismatch".into());
    }
    if params.contains_key("error") {
        return Err("Authorization was declined or requires organization approval. Other inboxes remain available.".into());
    }
    if !params.contains_key("code") {
        return Err("No authorization code".into());
    }
    Ok(params)
}
pub async fn connect(state: Arc<AppState>, provider: String) -> Result<(), String> {
    let _guard = state
        .auth_lock
        .try_lock()
        .map_err(|_| "A sign-in is already in progress")?;
    if !["siwc", "gmail", "outlook", "slack"].contains(&provider.as_str()) {
        return Err("This integration is coming in a later release.".into());
    }
    let settings = state.store.lock().map_err(|_| "Storage busy")?.settings()?;
    let old = if provider == "siwc" {
        load("siwc")?
    } else {
        None
    };
    let client = match provider.as_str() {
        "gmail" => settings.google_client_id.clone(),
        "outlook" => settings.microsoft_client_id.clone(),
        "slack" => settings.slack_client_id.clone(),
        _ => old
            .as_ref()
            .map(|t| t.client_id.clone())
            .unwrap_or("dynamic_agent_client".into()),
    };
    if client.trim().is_empty() {
        return Err(format!(
            "Add the {} app client ID in Settings first.",
            match provider.as_str() {
                "gmail" => "Google Desktop OAuth",
                "slack" => "Slack",
                _ => "Microsoft public-client",
            }
        ));
    }
    // ponytail: Slack's callback comes back as a tomo:// link instead (see `open_url`), so its listener goes unused
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let redirect = if provider == "slack" {
        crate::slack::REDIRECT.to_string()
    } else {
        format!(
            "http://{}:{}/auth/callback",
            if provider == "outlook" {
                "localhost"
            } else {
                "127.0.0.1"
            },
            listener.local_addr().map_err(|e| e.to_string())?.port()
        )
    };
    let verifier = random();
    let nonce = random();
    let expected_state = random();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let auth_url = match provider.as_str() {
        "gmail" => "https://accounts.google.com/o/oauth2/v2/auth",
        "outlook" => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        "slack" => "https://slack.com/oauth/v2/authorize",
        _ => "https://auth.openai.com/api/accounts/authorize",
    };
    let scope = match provider.as_str() {
        "gmail" => crate::workspace::scopes(&settings),
        "outlook" => {
            "openid profile email offline_access User.Read Mail.ReadWrite Mail.Send".into()
        }
        "slack" => crate::slack::user_scopes(),
        _ => "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct".into(),
    };
    let mut url = url::Url::parse(auth_url).unwrap();
    url.query_pairs_mut().extend_pairs([
        ("client_id", client.as_str()),
        ("response_type", "code"),
        ("redirect_uri", &redirect),
        // Slack asks for user permissions separately from a bot's; Tomo has no bot.
        (
            if provider == "slack" {
                "user_scope"
            } else {
                "scope"
            },
            scope.as_str(),
        ),
        ("state", &expected_state),
        ("nonce", &nonce),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);
    if provider == "gmail" {
        url.query_pairs_mut()
            .append_pair("access_type", "offline")
            // the account chooser every time, so a second Google account can be added
            .append_pair("prompt", "consent select_account");
    }
    if provider == "outlook" {
        url.query_pairs_mut()
            .append_pair("prompt", "select_account");
    }
    if provider == "siwc" {
        let host = {
            let s = state.store.lock().map_err(|_| "Storage busy")?;
            let host = s
                .get::<String>("meta", "host-id")?
                .unwrap_or_else(|| format!("urn:uuid:{}", id()));
            s.put("meta", "host-id", &host)?;
            host
        };
        url.query_pairs_mut()
            .append_pair("ext_agent_host_id", &host)
            .append_pair("resource", "https://api.openai.com/v1");
        if client == "dynamic_agent_client" {
            url.query_pairs_mut().append_pair("agent_name_hint", "Tomo");
        } else if let Some(hint) = old.as_ref().and_then(|t| t.id_token.as_ref()) {
            url.query_pairs_mut().append_pair("id_token_hint", hint);
        }
    }
    // Slack accepts only an https or a custom-scheme redirect, so it returns through a tomo:// link.
    let link = (provider == "slack").then(|| {
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Ok(mut w) = waiting().lock() {
            *w = Some(tx);
        }
        rx
    });
    webbrowser::open(url.as_str()).map_err(|_| "Could not open the system browser")?;
    let params = if let Some(rx) = link {
        let target = tokio::time::timeout(std::time::Duration::from_secs(180), rx)
            .await
            .map_err(|_| "Sign-in timed out. Try again.")?
            .map_err(|_| "Sign-in was interrupted. Try again.")?;
        parse_callback(&target, &expected_state)?
    } else {
        let params=tokio::time::timeout(std::time::Duration::from_secs(180),async {loop {let (mut socket,_)=listener.accept().await.map_err(|e|e.to_string())?;let mut buffer=[0u8;8192];let n=tokio::time::timeout(std::time::Duration::from_secs(5),socket.read(&mut buffer)).await.map_err(|_|"Callback read timed out")?.map_err(|e|e.to_string())?;let request=String::from_utf8_lossy(&buffer[..n]);let target=request.lines().next().and_then(|l|l.split_whitespace().nth(1)).unwrap_or("");if !target.starts_with("/auth/callback?"){let _=socket.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;continue;}
 let result=parse_callback(target,&expected_state);let body=if result.is_ok(){"You can return to Tomo. Finishing account verification."}else{"Authorization failed. Return to Tomo to try again."};let reply=format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());let _=socket.write_all(reply.as_bytes()).await;return result;}}).await.map_err(|_|"Sign-in timed out. Try again.")??;
        params
    };
    let issued = if provider == "siwc" && client == "dynamic_agent_client" {
        params
            .get("client_id")
            .filter(|v| v.starts_with("oaiapp_"))
            .cloned()
            .ok_or("Registration did not return an issued client ID")?
    } else {
        if params.get("client_id").is_some_and(|v| v != &client) {
            return Err("Returned client ID does not match registration".into());
        }
        client
    };
    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("client_id", issued.clone()),
        ("code", params["code"].clone()),
        ("code_verifier", verifier),
        ("redirect_uri", redirect),
    ];
    if provider == "gmail" {
        if let Some(secret) = credential_get("google-client-secret")? {
            form.push(("client_secret", secret));
        }
    }
    if provider == "siwc" {
        form.push(("resource", "https://api.openai.com/v1".into()));
    }
    let v = response_json(
        state
            .http
            .post(endpoint(&provider))
            .form(&form)
            .send()
            .await
            .map_err(|_| "Network error exchanging authorization")?,
    )
    .await?;
    if provider == "slack" {
        // Slack answers 200 with ok false on a refused exchange, and puts the user's token under authed_user.
        if v["ok"] != true {
            return Err(format!(
                "Slack refused the sign-in ({}).",
                v["error"].as_str().unwrap_or("unknown error")
            ));
        }
        let user = &v["authed_user"];
        let token = user["access_token"]
            .as_str()
            .ok_or("Slack returned no user token. Check the app's User Token Scopes.")?;
        // A custom-scheme sign-in always gets rotating tokens: 12 hours, renewed with a refresh token.
        let rotation = user["refresh_token"].as_str().map(|r| {
            (
                r.to_string(),
                user["expires_in"].as_i64().unwrap_or(43_200),
                issued.clone(),
            )
        });
        return crate::slack::connect(&state, token, "", rotation).await;
    }
    let mut t = tokens(&v, issued.clone(), old.as_ref())?;
    if provider == "siwc" {
        let identity = verify_identity(
            &state,
            t.id_token.as_deref().ok_or("No ID token returned")?,
            &issued,
            &nonce,
        )
        .await?;
        if old
            .as_ref()
            .is_some_and(|o| o.subject.as_ref() != Some(&identity.sub))
        {
            return Err(
                "This is a different ChatGPT account. Disconnect before changing accounts.".into(),
            );
        }
        if !t
            .scope
            .split_whitespace()
            .any(|s| s == "chatgpt.tokens.use.direct")
        {
            return Err(
                "ChatGPT plan usage was not authorized. Reconnect and grant AI usage permission."
                    .into(),
            );
        }
        t.subject = Some(identity.sub);
        t.email = identity.email;
        save("siwc", &t)?;
    } else {
        save_account(&state, provider, t).await?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_slack_link_reaches_the_sign_in_waiting_for_it() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        *waiting().lock().unwrap() = Some(tx);
        open_url("tomo://elsewhere?code=x&state=s");
        assert!(rx.try_recv().is_err());
        open_url("tomo://slack/callback?code=a&state=s");
        let target = rx.try_recv().unwrap();
        assert_eq!(parse_callback(&target, "s").unwrap()["code"], "a");
        // nobody waits any more: a second link is dropped
        open_url("tomo://slack/callback?code=b&state=s");
    }
    #[test]
    fn callback_is_bound_to_attempt() {
        assert!(parse_callback("/auth/callback?code=a&state=wrong", "right").is_err());
        assert!(parse_callback("/auth/callback?code=a&state=right&state=right", "right").is_err());
        assert!(parse_callback("/other?code=a&state=right", "right").is_err());
        assert!(parse_callback("/auth/callback?error=access_denied&state=right", "right").is_err());
        assert_eq!(
            parse_callback("/auth/callback?code=a&state=right", "right").unwrap()["code"],
            "a"
        );
    }
}
