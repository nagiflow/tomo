use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sender {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub provider: String,
    pub account_id: String,
    pub conversation_id: String,
    pub sender: Sender,
    pub content: String,
    pub timestamp: String,
    pub is_direct_message: bool,
    pub is_mention: bool,
    pub requires_processing: bool,
    pub metadata: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Ignore,
    Fyi,
    Acknowledge,
    RoutineRequest,
    NeedsDecision,
    Sensitive,
    Urgent,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Medium,
    High,
}
impl Risk {
    pub fn score(&self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category: Category,
    pub confidence: f64,
    pub reason: String,
    pub suggested_action: String,
    pub risk: Risk,
    /// Short topic label shown in the queue, e.g. "Date change". Empty for records stored before it existed.
    #[serde(default)]
    pub tag: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplyDraft {
    pub content: String,
    pub missing_facts: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskEvaluation {
    pub risk: Risk,
    pub reason: String,
    pub uncertain_facts: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanAction {
    pub id: String,
    pub kind: String,
    pub operation: String,
    pub title: String,
    pub summary: String,
    pub detail: String,
    pub params: Value,
    #[serde(default = "pending")]
    pub status: String,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<String>,
}
fn pending() -> String {
    "pending".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WritingStyle {
    pub content: String,
    pub sample_count: usize,
    pub updated_at: Option<String>,
    pub path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub message: Message,
    pub subject: String,
    pub classification: Classification,
    pub draft: String,
    pub context: Vec<Message>,
    pub status: String,
    pub send_error: Option<String>,
    #[serde(default)]
    pub plan: Vec<PlanAction>,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub revision: u32,
    /// When Tomo will send this reply on its own, unless the user cancels first.
    #[serde(default)]
    pub auto_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub provider: String,
    pub email: String,
    pub status: String,
    pub error: Option<String>,
    pub cursor: Option<String>,
    pub last_sync: Option<String>,
    pub retry_at: Option<String>,
    pub failures: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub id: String,
    pub message_id: String,
    pub account_id: String,
    pub provider: String,
    pub sender: String,
    pub subject: String,
    pub category: Category,
    pub action: String,
    pub reason: String,
    pub timestamp: String,
    pub human_approved: bool,
    pub content: Option<String>,
    pub send_result: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct Settings {
    pub onboarded: bool,
    pub paused: bool,
    pub auto_send: bool,
    /// YOLO mode: every plan Tomo makes runs at once, replies included, with no approval. Only
    /// "Never automate" for a sender, pausing, and a failed step still stop it.
    pub yolo: bool,
    pub consent: bool,
    pub notifications: bool,
    pub poll_seconds: u64,
    /// How many messages Tomo works on at once, 1 to `engine::MAX_WORKERS`. More is faster after
    /// a burst of mail, and uses the AI provider's rate limit faster.
    pub workers: u32,
    pub tone: String,
    pub blocked_senders: Vec<String>,
    pub blocked_threads: Vec<String>,
    pub google_client_id: String,
    pub google_client_secret: String,
    pub microsoft_client_id: String,
    /// Client ID of the user's own Slack app, for signing in to workspaces with it (see slack::MANIFEST).
    pub slack_client_id: String,
    pub ai_provider: String,
    pub model: String,
    /// "" leaves the model's own default; otherwise low, medium or high.
    pub reasoning_effort: String,
    pub tools: std::collections::HashMap<String, String>,
    pub google_project_id: String,
    pub google_account_id: String,
    /// The one folder the Codex tool may work in.
    pub codex_dir: String,
    /// What happens in the mailbox to mail Tomo files without a reply: none, read or archive.
    pub after_archive: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarded: false,
            paused: false,
            auto_send: false,
            yolo: false,
            consent: false,
            notifications: true,
            poll_seconds: 120,
            workers: 1,
            tone: "Brief and warm. Do the task first, then ask for feedback; never invent commitments.".into(),
            blocked_senders: vec![],
            blocked_threads: vec![],
            google_client_id: String::new(),
            google_client_secret: String::new(),
            microsoft_client_id: String::new(),
            slack_client_id: String::new(),
            ai_provider: "siwc".into(),
            model: String::new(),
            reasoning_effort: String::new(),
            tools: crate::workspace::TOOLS
                .into_iter()
                .map(|k| {
                    let mode = if k == "calendar" { "ask" } else { "off" };
                    (k.into(), mode.into())
                })
                .collect(),
            google_project_id: String::new(),
            google_account_id: String::new(),
            codex_dir: String::new(),
            after_archive: "read".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub connected: bool,
    pub provider: String,
    pub email: Option<String>,
    pub status: String,
    pub models: Vec<Model>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: Settings,
    pub accounts: Vec<Account>,
    pub approvals: Vec<Approval>,
    pub activity: Vec<Activity>,
    /// Mail fetched that Tomo has not processed yet, shaped like activity.
    pub pending: Vec<Activity>,
    /// What Tomo is doing right now, by message ID.
    pub working: std::collections::HashMap<String, String>,
    pub ai: AiStatus,
    pub syncing: bool,
    pub last_error: Option<String>,
    pub style: WritingStyle,
    /// rules.md, "" when the user has none.
    pub rules: String,
    pub codex_installed: bool,
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
