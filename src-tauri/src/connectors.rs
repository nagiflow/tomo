use crate::{auth, models::*, AppState};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;
#[async_trait]
pub trait CommunicationConnector: Send + Sync {
    async fn fetch_new_messages(&self, account: &Account)
        -> Result<(Vec<Message>, String), String>;
    async fn fetch_conversation(
        &self,
        account: &Account,
        message: &Message,
    ) -> Result<Vec<Message>, String>;
    async fn send_reply(
        &self,
        account: &Account,
        approval: &Approval,
        content: &str,
    ) -> Result<String, String>;
    async fn reconcile(
        &self,
        account: &Account,
        approval: &Approval,
    ) -> Result<Option<String>, String>;
}
fn mime_base64(content: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(content);
    encoded
        .as_bytes()
        .chunks(76)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n")
}
fn content_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "pdf" => "application/pdf",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "csv" => "text/csv",
        "txt" | "md" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}
/// The body of a reply: plain text alone, or multipart/mixed when files are attached.
/// Returns the Content-Type header lines and the encoded body that follows the blank line.
fn mime_body(content: &str, files: &[(String, Vec<u8>)]) -> (String, String) {
    let text_headers =
        "Content-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64";
    let text = mime_base64(content.as_bytes());
    if files.is_empty() {
        return (text_headers.into(), text);
    }
    let boundary = format!("tomo-{}", crate::models::id());
    let mut body = format!("--{boundary}\r\n{text_headers}\r\n\r\n{text}\r\n");
    for (name, bytes) in files {
        // RFC 2231 file name, so non-ASCII names survive and no header can be injected through one
        let encoded: String = url::form_urlencoded::byte_serialize(name.as_bytes())
            .collect::<String>()
            .replace('+', "%20");
        body.push_str(&format!(
            "--{boundary}\r\nContent-Type: {}\r\nContent-Disposition: attachment; filename*=UTF-8''{encoded}\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n",
            content_type(name),
            mime_base64(bytes)
        ));
    }
    body.push_str(&format!("--{boundary}--\r\n"));
    (
        format!("Content-Type: multipart/mixed; boundary=\"{boundary}\""),
        body,
    )
}
fn send_marker(id: &str) -> String {
    format!(
        "tomo-{}@tomo.local",
        URL_SAFE_NO_PAD.encode(Sha256::digest(id.as_bytes()))
    )
}
pub struct MailConnector {
    pub state: Arc<AppState>,
}
fn header<'a>(payload: &'a Value, name: &str) -> Option<&'a str> {
    payload["headers"].as_array()?.iter().find(|h| {
        h["name"]
            .as_str()
            .is_some_and(|v| v.eq_ignore_ascii_case(name))
    })?["value"]
        .as_str()
}
pub(crate) fn decoded_header(payload: &Value, name: &str) -> String {
    let raw = header(payload, name).unwrap_or("");
    let line = format!("{name}: {raw}");
    mailparse::parse_header(line.as_bytes())
        .map(|(h, _)| h.get_value())
        .unwrap_or_else(|_| raw.into())
}
/// The first part of the given type anywhere in a Gmail payload, decoded.
fn gmail_part(p: &Value, mime: &str) -> Option<String> {
    if p["mimeType"] == mime || (mime == "text/plain" && p["mimeType"].is_null()) {
        if let Some(data) = p["body"]["data"].as_str() {
            // Gmail pads its base64url; the decoder here rejects padding
            return URL_SAFE_NO_PAD
                .decode(data.trim_end_matches('='))
                .ok()
                .map(|v| String::from_utf8_lossy(&v).into_owned())
                .filter(|s| !s.trim().is_empty());
        }
    }
    p["parts"]
        .as_array()?
        .iter()
        .find_map(|part| gmail_part(part, mime))
}
/// The message as text: its plain-text part, or its HTML part with the markup removed.
/// (Mail usually carries both; reading both doubled the text and the AI's input.)
pub(crate) fn gmail_body(p: &Value) -> String {
    gmail_part(p, "text/plain")
        .or_else(|| gmail_part(p, "text/html").map(|html| strip_html(&html)))
        .unwrap_or_default()
}
/// The HTML part as sent, for display only. Capped so one newsletter cannot bloat the local store.
fn html_for_display(html: Option<String>) -> Value {
    match html {
        Some(html) if html.len() <= 300_000 => json!(html),
        _ => Value::Null,
    }
}
/// Removes whole elements whose content is not text for a reader, such as style sheets.
fn drop_elements(html: &str, tag: &str) -> String {
    // ASCII-only lowercasing keeps byte positions identical to the original text
    let lower = html.to_ascii_lowercase();
    let (open, close) = (format!("<{tag}"), format!("</{tag}>"));
    let mut out = String::new();
    let mut at = 0;
    while let Some(start) = lower[at..].find(&open).map(|i| i + at) {
        out.push_str(&html[at..start]);
        match lower[start..].find(&close) {
            Some(end) => at = start + end + close.len(),
            None => return out,
        }
    }
    out.push_str(&html[at..]);
    out
}
pub fn strip_html(s: &str) -> String {
    let s = ["style", "script", "head"]
        .iter()
        .fold(s.to_string(), |html, tag| drop_elements(&html, tag));
    let mut inside = false;
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '<' => {
                inside = true;
                result.push(' ')
            }
            '>' => inside = false,
            _ if !inside => result.push(c),
            _ => (),
        }
    }
    result
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}
/// Mail sent by a machine (newsletter, notification, receipt, bounce), judged from the sender and the
/// standard bulk-mail headers. Tomo files these without spending any AI tokens on them.
pub(crate) fn automated(address: &str, payload: Option<&Value>) -> bool {
    let local = address.split('@').next().unwrap_or("").to_lowercase();
    let machine_sender = [
        "noreply",
        "no-reply",
        "no_reply",
        "donotreply",
        "do-not-reply",
        "mailer-daemon",
        "postmaster",
        "notification",
        "newsletter",
        "bounce",
    ]
    .iter()
    .any(|p| local.contains(p));
    let bulk_headers = payload.is_some_and(|p| {
        header(p, "List-Unsubscribe").is_some()
            || header(p, "Precedence").is_some_and(|v| {
                ["bulk", "list", "junk"].contains(&v.trim().to_lowercase().as_str())
            })
            || header(p, "Auto-Submitted").is_some_and(|v| !v.trim().eq_ignore_ascii_case("no"))
    });
    machine_sender || bulk_headers
}
fn gmail_normalize(v: &Value, a: &Account) -> Result<Message, String> {
    let raw_id = v["id"].as_str().ok_or("Missing Gmail message ID")?;
    let p = &v["payload"];
    let from_decoded = decoded_header(p, "From");
    let from = from_decoded.as_str();
    let (name, address) = if let Some(pos) = from.rfind('<') {
        (
            from[..pos].trim().trim_matches('"').to_string(),
            from[pos + 1..].trim_end_matches('>').to_string(),
        )
    } else {
        (from.to_string(), from.to_string())
    };
    let millis = v["internalDate"]
        .as_str()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    Ok(Message {
        id: format!("{}:{raw_id}", a.id),
        provider: "gmail".into(),
        account_id: a.id.clone(),
        conversation_id: v["threadId"].as_str().unwrap_or(raw_id).into(),
        sender: Sender {
            id: address.clone(),
            name,
            address: Some(address.clone()),
        },
        content: gmail_body(p).chars().take(20000).collect(),
        timestamp: chrono::DateTime::from_timestamp_millis(millis)
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(now),
        is_direct_message: true,
        is_mention: false,
        requires_processing: true,
        metadata: json!({"providerId":raw_id,"subject":decoded_header(p,"Subject"),"messageId":header(p,"Message-ID"),"references":header(p,"References"),"automated":automated(&address,Some(p)),"html":html_for_display(gmail_part(p,"text/html"))}),
    })
}
fn outlook_normalize(v: &Value, a: &Account) -> Result<Message, String> {
    let raw_id = v["id"].as_str().ok_or("Missing Outlook message ID")?;
    let sender = &v["from"]["emailAddress"];
    let address = sender["address"].as_str().unwrap_or("").to_string();
    let content = v["body"]["content"].as_str().unwrap_or("");
    Ok(Message {
        id: format!("{}:{raw_id}", a.id),
        provider: "outlook".into(),
        account_id: a.id.clone(),
        conversation_id: v["conversationId"].as_str().unwrap_or(raw_id).into(),
        sender: Sender {
            id: address.clone(),
            name: sender["name"].as_str().unwrap_or(&address).into(),
            address: Some(address.clone()),
        },
        content: if v["body"]["contentType"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("html"))
        {
            strip_html(content)
        } else {
            content.into()
        }
        .chars()
        .take(20000)
        .collect(),
        timestamp: v["receivedDateTime"].as_str().unwrap_or("").into(),
        is_direct_message: true,
        is_mention: false,
        requires_processing: true,
        // ponytail: sender-name check only; add internetMessageHeaders to the Graph $select to read bulk headers too
        metadata: json!({"providerId":raw_id,"subject":v["subject"],"webLink":v["webLink"],"internetMessageId":v["internetMessageId"],"automated":automated(&address,None),"html":html_for_display(v["body"]["contentType"].as_str().filter(|t|t.eq_ignore_ascii_case("html")).map(|_|content.to_string()))}),
    })
}
impl MailConnector {
    async fn get(&self, account: &Account, url: &str) -> Result<Value, String> {
        validate_api_url(&account.provider, url)?;
        let token = auth::access(&self.state, &account.id, &account.provider).await?;
        auth::response_json(
            self.state
                .http
                .get(url)
                .bearer_auth(token)
                .header(
                    "Prefer",
                    "IdType=\"ImmutableId\", outlook.body-content-type=\"text\"",
                )
                .send()
                .await
                .map_err(|_| "Network unavailable. Messages remain saved locally.")?,
        )
        .await
    }
}
impl MailConnector {
    /// Whether the user has ever written to this sender: the test for who Tomo may answer alone.
    /// Slack skips it: Tomo only sees DMs and mentions there, and both may be answered alone.
    pub async fn wrote_to(&self, a: &Account, m: &Message) -> Result<bool, String> {
        if a.provider == "slack" {
            return Ok(true);
        }
        let address = m.sender.address.as_deref().unwrap_or("");
        // The address goes into a search query, so anything but a plain address is refused.
        if !address.contains('@')
            || !address
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "@._+-".contains(c))
        {
            return Ok(false);
        }
        let token = auth::access(&self.state, &a.id, &a.provider).await?;
        let (url, query, list) = if a.provider == "gmail" {
            (
                "https://gmail.googleapis.com/gmail/v1/users/me/messages",
                vec![
                    ("q", format!("in:sent to:{address}")),
                    ("maxResults", "1".into()),
                ],
                "messages",
            )
        } else {
            (
                "https://graph.microsoft.com/v1.0/me/mailFolders/sentitems/messages",
                vec![
                    ("$search", format!("\"to:{address}\"")),
                    ("$top", "1".into()),
                    ("$select", "id".into()),
                ],
                "value",
            )
        };
        let found = auth::response_json(
            self.state
                .http
                .get(url)
                .query(&query)
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|_| "Network unavailable.")?,
        )
        .await?;
        Ok(found[list].as_array().is_some_and(|v| !v.is_empty()))
    }
    /// Marks mail Tomo filed without a reply as read; "archive" also moves it out of the inbox.
    pub async fn tidy(&self, a: &Account, m: &Message, mode: &str) -> Result<(), String> {
        if a.provider == "slack" {
            return crate::slack::tidy(&self.state, a, m).await;
        }
        let id = m.metadata["providerId"]
            .as_str()
            .ok_or("Missing provider message ID")?;
        let token = auth::access(&self.state, &a.id, &a.provider).await?;
        let archive = mode == "archive";
        let mut calls = vec![];
        if a.provider == "gmail" {
            let labels = if archive {
                json!(["UNREAD", "INBOX"])
            } else {
                json!(["UNREAD"])
            };
            calls.push((
                reqwest::Method::POST,
                format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}/modify"),
                json!({ "removeLabelIds": labels }),
            ));
        } else {
            calls.push((
                reqwest::Method::PATCH,
                format!("https://graph.microsoft.com/v1.0/me/messages/{id}"),
                json!({"isRead":true}),
            ));
            if archive {
                calls.push((
                    reqwest::Method::POST,
                    format!("https://graph.microsoft.com/v1.0/me/messages/{id}/move"),
                    json!({"destinationId":"archive"}),
                ));
            }
        }
        for (method, url, body) in calls {
            auth::response_json(
                self.state
                    .http
                    .request(method, &url)
                    .bearer_auth(&token)
                    .header("Prefer", "IdType=\"ImmutableId\"")
                    .json(&body)
                    .send()
                    .await
                    .map_err(|_| "Network unavailable.")?,
            )
            .await?;
        }
        Ok(())
    }
}
pub fn validate_api_url(provider: &str, s: &str) -> Result<(), String> {
    let url = url::Url::parse(s).map_err(|_| "Invalid provider URL")?;
    let expected = if provider == "gmail" {
        "gmail.googleapis.com"
    } else {
        "graph.microsoft.com"
    };
    if url.scheme() != "https"
        || url.host_str() != Some(expected)
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Rejected an unexpected provider URL".into());
    }
    Ok(())
}
#[async_trait]
impl CommunicationConnector for MailConnector {
    async fn fetch_new_messages(&self, a: &Account) -> Result<(Vec<Message>, String), String> {
        if a.provider == "slack" {
            return crate::slack::fetch_new(&self.state, a).await;
        }
        if a.provider == "gmail" {
            let profile = self
                .get(a, "https://gmail.googleapis.com/gmail/v1/users/me/profile")
                .await?;
            let checkpoint = profile["historyId"]
                .as_str()
                .ok_or("Missing Gmail history cursor")?
                .to_string();
            let mut ids = vec![];
            let mut next: Option<String> = None;
            let mut history = a.cursor.clone();
            loop {
                let mut url = url::Url::parse(if history.is_some() {
                    "https://gmail.googleapis.com/gmail/v1/users/me/history"
                } else {
                    "https://gmail.googleapis.com/gmail/v1/users/me/messages"
                })
                .unwrap();
                {
                    let mut q = url.query_pairs_mut();
                    q.append_pair("maxResults", "100");
                    if let Some(cursor) = &history {
                        q.append_pair("startHistoryId", cursor)
                            .append_pair("historyTypes", "messageAdded");
                    } else {
                        // First run only: start from the last day, not the whole unread backlog.
                        // After that the history cursor picks up everything since the last check.
                        q.append_pair("q", "is:unread in:inbox -category:promotions newer_than:1d");
                    }
                    if let Some(token) = &next {
                        q.append_pair("pageToken", token);
                    }
                }
                let v = match self.get(a, url.as_str()).await {
                    Ok(v) => v,
                    Err(e) if e.contains("404") && history.is_some() => {
                        history = None;
                        next = None;
                        continue;
                    }
                    Err(e) => return Err(e),
                };
                if history.is_some() {
                    if let Some(records) = v["history"].as_array() {
                        for record in records {
                            if let Some(added) = record["messagesAdded"].as_array() {
                                for added in added {
                                    let m = &added["message"];
                                    // Gmail files marketing mail under Promotions; Tomo leaves it alone.
                                    if m["labelIds"].as_array().is_some_and(|v| {
                                        v.iter().any(|v| v == "INBOX")
                                            && !v.iter().any(|v| v == "CATEGORY_PROMOTIONS")
                                    }) {
                                        if let Some(id) = m["id"].as_str() {
                                            ids.push(id.to_string())
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(messages) = v["messages"].as_array() {
                    for m in messages {
                        if let Some(id) = m["id"].as_str() {
                            ids.push(id.to_string())
                        }
                    }
                }
                next = v["nextPageToken"].as_str().map(str::to_string);
                if next.is_none() {
                    break;
                }
                if ids.len() > 1000 {
                    return Err("Inbox has a large backlog. Mark older messages read before initial synchronization.".into());
                }
            }
            ids.sort();
            ids.dedup();
            let mut messages = vec![];
            for id in ids {
                let raw=self.get(a,&format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{id}?format=full")).await?;
                let m = gmail_normalize(&raw, a)?;
                if m.sender.address.as_deref() != Some(&a.email) {
                    messages.push(m)
                }
            }
            Ok((messages, checkpoint))
        } else {
            // First run only: the last day. receivedDateTime is the one filter Graph's delta query accepts.
            let mut url = a.cursor.clone().unwrap_or_else(|| {
                format!(
                    "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages/delta?$select=id,conversationId,subject,from,body,receivedDateTime,isRead,webLink,internetMessageId&$filter=receivedDateTime+ge+{}",
                    (chrono::Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%dT%H:%M:%SZ")
                )
            });
            let mut messages = vec![];
            for _ in 0..10 {
                let v = self.get(a, &url).await?;
                if let Some(values) = v["value"].as_array() {
                    for raw in values {
                        if raw["@removed"].is_null() && raw["isRead"] == false {
                            messages.push(outlook_normalize(raw, a)?);
                        }
                    }
                }
                if let Some(next) = v["@odata.nextLink"].as_str() {
                    url = next.into();
                } else {
                    let cursor = v["@odata.deltaLink"]
                        .as_str()
                        .ok_or("Missing Outlook delta cursor")?
                        .into();
                    return Ok((messages, cursor));
                }
            }
            Ok((messages, url))
        }
    }
    async fn fetch_conversation(&self, a: &Account, m: &Message) -> Result<Vec<Message>, String> {
        if a.provider == "slack" {
            return crate::slack::fetch_conversation(&self.state, a, m).await;
        }
        if a.provider == "gmail" {
            let v = self
                .get(
                    a,
                    &format!(
                        "https://gmail.googleapis.com/gmail/v1/users/me/threads/{}?format=full",
                        m.conversation_id
                    ),
                )
                .await?;
            v["messages"]
                .as_array()
                .ok_or("Missing thread messages")?
                .iter()
                .rev()
                .take(6)
                .map(|v| gmail_normalize(v, a))
                .collect::<Result<Vec<_>, _>>()
                .map(|mut v| {
                    v.reverse();
                    v
                })
        } else {
            let mut url = url::Url::parse("https://graph.microsoft.com/v1.0/me/messages").unwrap();
            url.query_pairs_mut().append_pair("$filter",&format!("conversationId eq '{}'",m.conversation_id.replace('\'',"''"))).append_pair("$top","6").append_pair("$select","id,conversationId,subject,from,body,receivedDateTime,webLink,internetMessageId");
            let v = self.get(a, url.as_str()).await?;
            let mut messages = v["value"]
                .as_array()
                .ok_or("Missing Outlook conversation")?
                .iter()
                .map(|v| outlook_normalize(v, a))
                .collect::<Result<Vec<_>, _>>()?;
            messages.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            Ok(messages)
        }
    }
    async fn send_reply(
        &self,
        a: &Account,
        item: &Approval,
        content: &str,
    ) -> Result<String, String> {
        if a.provider == "slack" {
            return crate::slack::send_reply(&self.state, a, item, content).await;
        }
        let token = auth::access(&self.state, &a.id, &a.provider).await?;
        // Checked before anything is created or sent: a refused attachment stops the reply.
        let files = crate::codex::attachments(&self.state, item)?;
        let provider_id = item.message.metadata["providerId"]
            .as_str()
            .ok_or("Missing provider message ID")?;
        if a.provider == "gmail" {
            let recipient = item
                .message
                .sender
                .address
                .as_deref()
                .ok_or("Missing recipient")?;
            if recipient.contains(['\r', '\n']) || a.email.contains(['\r', '\n']) {
                return Err("Invalid mail headers".into());
            }
            let subject = if item.subject.starts_with("Re:") {
                item.subject.clone()
            } else {
                format!("Re: {}", item.subject)
            };
            let subject = base64::engine::general_purpose::STANDARD.encode(subject.as_bytes());
            let reference = item.message.metadata["messageId"]
                .as_str()
                .unwrap_or("")
                .replace(['\r', '\n'], "");
            let references = item.message.metadata["references"]
                .as_str()
                .unwrap_or("")
                .replace(['\r', '\n'], "");
            let marker = format!("<{}>", send_marker(&item.id));
            let (content_headers, content_body) = mime_body(content, &files);
            let mail=format!("From: {}\r\nTo: {recipient}\r\nSubject: =?UTF-8?B?{subject}?=\r\nMessage-ID: {marker}\r\nIn-Reply-To: {reference}\r\nReferences: {references} {reference}\r\nMIME-Version: 1.0\r\n{content_headers}\r\n\r\n{content_body}",a.email);
            let request = if files.is_empty() {
                self.state.http.post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send").json(&json!({"raw":URL_SAFE_NO_PAD.encode(mail),"threadId":item.message.conversation_id}))
            } else {
                // Attachments go through the upload endpoint, which accepts large messages. It takes no
                // threadId; Gmail threads the reply from its In-Reply-To and References headers.
                self.state.http.post("https://gmail.googleapis.com/upload/gmail/v1/users/me/messages/send?uploadType=media").header("Content-Type", "message/rfc822").body(mail)
            };
            let v = auth::response_json(
                request
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| "Send outcome uncertain. Check sent mail before retrying.")?,
            )
            .await?;
            Ok(v["id"]
                .as_str()
                .ok_or("Send response missing message ID; reconcile first")?
                .into())
        } else {
            let v = auth::response_json(
                self.state
                    .http
                    .post(format!(
                        "https://graph.microsoft.com/v1.0/me/messages/{provider_id}/createReply"
                    ))
                    .bearer_auth(&token)
                    .header("Prefer", "IdType=\"ImmutableId\"")
                    .json(&json!({"comment":content}))
                    .send()
                    .await
                    .map_err(|_| "Could not confirm reply draft creation")?,
            )
            .await?;
            let draft_id = v["id"].as_str().ok_or("Missing reply draft ID")?;
            self.state.store.lock().map_err(|_| "Storage busy")?.put(
                "send-draft",
                &item.id,
                &draft_id,
            )?;
            for (name, bytes) in &files {
                // ponytail: Graph's simple attach call takes about 3 MB per file; use an upload session for bigger ones
                if bytes.len() > 3 * 1024 * 1024 {
                    return Err(format!("{name} is larger than 3 MB, which Tomo cannot attach to Outlook replies yet. The reply was not sent; a draft may remain in Outlook."));
                }
                auth::response_json(
                    self.state
                        .http
                        .post(format!(
                            "https://graph.microsoft.com/v1.0/me/messages/{draft_id}/attachments"
                        ))
                        .bearer_auth(&token)
                        .json(&json!({"@odata.type":"#microsoft.graph.fileAttachment","name":name,"contentType":content_type(name),"contentBytes":base64::engine::general_purpose::STANDARD.encode(bytes)}))
                        .send()
                        .await
                        .map_err(|_| "Could not attach a file to the reply. The reply was not sent.")?,
                )
                .await?;
            }
            let response = self
                .state
                .http
                .post(format!(
                    "https://graph.microsoft.com/v1.0/me/messages/{draft_id}/send"
                ))
                .header("Prefer", "IdType=\"ImmutableId\"")
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| "Send outcome uncertain. Reconcile with Outlook before retrying.")?;
            if !response.status().is_success() {
                return Err(format!(
                    "Send outcome uncertain ({}). Check sent messages.",
                    response.status()
                ));
            }
            Ok(draft_id.into())
        }
    }
    async fn reconcile(&self, a: &Account, item: &Approval) -> Result<Option<String>, String> {
        if a.provider == "slack" {
            return crate::slack::reconcile(&self.state, a, item).await;
        }
        if a.provider == "gmail" {
            let mut url =
                url::Url::parse("https://gmail.googleapis.com/gmail/v1/users/me/messages").unwrap();
            url.query_pairs_mut().append_pair(
                "q",
                &format!("in:sent rfc822msgid:{}", send_marker(&item.id)),
            );
            let v = self.get(a, url.as_str()).await?;
            Ok(v["messages"][0]["id"].as_str().map(str::to_string))
        } else {
            let draft_id = self
                .state
                .store
                .lock()
                .map_err(|_| "Storage busy")?
                .get::<String>("send-draft", &item.id)?;
            if let Some(id) = draft_id {
                let v = self
                    .get(
                        a,
                        &format!(
                            "https://graph.microsoft.com/v1.0/me/messages/{id}?$select=id,isDraft"
                        ),
                    )
                    .await?;
                if v["isDraft"] == false {
                    return Ok(Some(id));
                }
            }
            Ok(None)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursors_cannot_exfiltrate_credentials() {
        assert!(validate_api_url("outlook", "https://evil.test/?token=x").is_err());
        assert!(validate_api_url("outlook", "http://graph.microsoft.com/v1.0/me").is_err());
        assert!(validate_api_url(
            "gmail",
            "https://gmail.googleapis.com/gmail/v1/users/me/messages"
        )
        .is_ok());
    }
    #[test]
    fn html_is_never_executed() {
        assert_eq!(strip_html("<b>Hi</b> &amp; bye"), " Hi  & bye");
    }
}
#[cfg(test)]
mod normalization_tests {
    use super::*;
    fn account() -> Account {
        Account {
            id: "test".into(),
            provider: "gmail".into(),
            email: "you@example.com".into(),
            status: "connected".into(),
            error: None,
            cursor: None,
            last_sync: None,
            retry_at: None,
            failures: 0,
        }
    }
    #[test]
    fn encoded_subjects_and_unicode_bodies_are_normalized_before_policy() {
        let value = json!({"id":"m","threadId":"t","internalDate":"1700000000000","payload":{"mimeType":"text/plain","headers":[{"name":"Subject","value":"=?UTF-8?B?Q29udHJhY3QgYXBwcm92YWw=?="},{"name":"From","value":"Maya <maya@example.com>"}],"body":{"data":URL_SAFE_NO_PAD.encode("確認してください")}}});
        let m = gmail_normalize(&value, &account()).unwrap();
        assert_eq!(m.metadata["subject"], "Contract approval");
        assert_eq!(m.content, "確認してください");
        assert!(crate::policy::safety_reason(&m, &[], "Thanks, received.").is_some());
    }
    #[test]
    fn outgoing_mime_lines_are_wrapped() {
        let body = "こんにちは".repeat(100);
        let encoded = mime_base64(body.as_bytes());
        assert!(encoded.lines().all(|l| l.len() <= 76));
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(encoded.replace("\r\n", ""))
                .unwrap(),
            body.as_bytes()
        );
    }
}
#[cfg(test)]
mod automated_tests {
    use super::*;
    #[test]
    fn machine_mail_is_recognised_without_ai() {
        let headers = |name: &str, value: &str| json!({"headers":[{"name":name,"value":value}]});
        assert!(automated("no-reply@shop.example", None));
        assert!(automated("notifications@github.com", None));
        assert!(automated(
            "ana@news.example",
            Some(&headers("List-Unsubscribe", "<mailto:u@x>"))
        ));
        assert!(automated(
            "ana@corp.example",
            Some(&headers("precedence", "Bulk"))
        ));
        assert!(!automated(
            "ana@corp.example",
            Some(&headers("Auto-Submitted", "no"))
        ));
        assert!(!automated(
            "sarah@client.example",
            Some(&headers("Subject", "Can we move the review?"))
        ));
    }
}
#[cfg(test)]
mod attachment_tests {
    use super::*;
    #[test]
    fn replies_with_files_become_multipart_and_names_cannot_inject_headers() {
        let (headers, body) = mime_body("Hi", &[]);
        assert!(headers.starts_with("Content-Type: text/plain"));
        assert!(!body.contains("--tomo-"));
        let files = [(
            "Q3 deck\r\nBcc: x@evil.example.pptx".to_string(),
            b"data".to_vec(),
        )];
        let (headers, body) = mime_body("Hi", &files);
        assert!(headers.starts_with("Content-Type: multipart/mixed; boundary="));
        assert!(body.contains("presentationml.presentation"));
        assert!(body.contains("filename*=UTF-8''Q3%20deck%0D%0ABcc"));
        assert!(!body.contains("\r\nBcc:"));
        assert!(body.trim_end().ends_with("--"));
    }
}
#[cfg(test)]
mod body_tests {
    use super::*;
    #[test]
    fn mail_text_prefers_the_plain_part_and_never_includes_style_sheets() {
        let b64 = |s: &str| URL_SAFE_NO_PAD.encode(s);
        let both = json!({"mimeType":"multipart/alternative","parts":[
            {"mimeType":"text/plain","body":{"data":b64("Hello plain")}},
            {"mimeType":"text/html","body":{"data":b64("<p>Hello html</p>")}}]});
        assert_eq!(gmail_body(&both), "Hello plain");
        assert_eq!(
            gmail_part(&both, "text/html").as_deref(),
            Some("<p>Hello html</p>")
        );
        let html_only = json!({"mimeType":"text/html","body":{"data":b64(
            "<html><head><STYLE>.a{color:red}</STYLE></head><body><p>Hi</p><script>x()</script></body></html>")}});
        let text = gmail_body(&html_only);
        assert!(text.contains("Hi") && !text.contains("color") && !text.contains("x()"));
        // Gmail sends padded base64url ("Hello" is 5 bytes, so it ends in "=")
        let padded = json!({"mimeType":"text/plain","body":{"data":"SGVsbG8="}});
        assert_eq!(gmail_body(&padded), "Hello");
    }
}
