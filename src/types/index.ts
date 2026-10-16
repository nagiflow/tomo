export type Provider = "gmail" | "outlook" | "slack" | "teams";
export type Category =
  | "ignore"
  | "fyi"
  | "acknowledge"
  | "routine_request"
  | "needs_decision"
  | "sensitive"
  | "urgent";
export type Risk = "low" | "medium" | "high";
export interface UnifiedMessage {
  id: string;
  provider: Provider;
  accountId: string;
  conversationId: string;
  sender: { id: string; name: string; address?: string };
  content: string;
  timestamp: string;
  isDirectMessage: boolean;
  isMention: boolean;
  requiresProcessing: boolean;
  metadata: Record<string, unknown>;
}
export interface Classification {
  category: Category;
  confidence: number;
  reason: string;
  suggestedAction:
    "ignore" | "record_only" | "draft" | "require_approval" | "auto_send";
  risk: Risk;
  tag?: string;
}
export interface ReplyDraft {
  content: string;
  missingFacts: string[];
}
export interface RiskEvaluation {
  risk: Risk;
  reason: string;
  uncertainFacts: boolean;
}
export type ToolKind =
  "calendar" | "drive" | "docs" | "sheets" | "slides" | "contacts" | "codex";
export type ToolMode = "off" | "read" | "ask";
export interface PlanAction {
  id: string;
  kind: "mail" | "slack" | ToolKind;
  operation: string;
  title: string;
  summary: string;
  detail: string;
  params: Record<string, unknown>;
  status: "pending" | "running" | "completed" | "failed" | "uncertain";
  result?: Record<string, unknown>;
  error?: string;
}
export interface WritingStyle {
  content: string;
  sampleCount: number;
  updatedAt?: string;
  path?: string;
}
export const toolLabels: Record<ToolKind | "slack", string> = {
  calendar: "Google Calendar",
  drive: "Google Drive",
  docs: "Google Docs",
  sheets: "Google Sheets",
  slides: "Google Slides",
  codex: "Codex CLI",
  contacts: "Google Contacts",
  slack: "Slack",
};
export interface Approval {
  id: string;
  message: UnifiedMessage;
  subject: string;
  classification: Classification;
  draft: string;
  context: UnifiedMessage[];
  status:
    "pending" | "sending" | "sent" | "dismissed" | "uncertain" | "executing";
  sendError?: string;
  plan?: PlanAction[];
  instruction?: string;
  revision?: number;
  /** When Tomo will send this on its own unless cancelled. */
  autoAt?: string;
}
export interface Account {
  id: string;
  provider: Provider;
  email: string;
  status: "connected" | "expired" | "error";
  error?: string;
  cursor?: string;
  lastSync?: string;
  retryAt?: string;
  failures: number;
}
export interface Activity {
  id: string;
  messageId: string;
  accountId: string;
  provider: Provider;
  sender: string;
  subject: string;
  category: Category;
  action: string;
  reason: string;
  timestamp: string;
  humanApproved: boolean;
  content?: string;
  sendResult?: string;
}
export interface Settings {
  onboarded: boolean;
  paused: boolean;
  autoSend: boolean;
  /** Every plan runs without approval. */
  yolo?: boolean;
  consent: boolean;
  notifications: boolean;
  pollSeconds: number;
  /** Messages Tomo works on at once, 1 to 8. */
  workers?: number;
  tone: string;
  blockedSenders: string[];
  blockedThreads: string[];
  googleClientId: string;
  googleClientSecret: string;
  microsoftClientId: string;
  slackClientId?: string;
  aiProvider: "siwc" | "openai";
  model: string;
  reasoningEffort?: "" | "low" | "medium" | "high";
  tools?: Record<ToolKind, ToolMode>;
  googleProjectId?: string;
  googleAccountId?: string;
  codexDir?: string;
  afterArchive?: "none" | "read" | "archive";
}
export interface AIStatus {
  connected: boolean;
  provider: string;
  email?: string;
  status: string;
  models: { id: string; name: string }[];
}
export interface Snapshot {
  pending?: Activity[];
  working?: Record<string, string>;
  settings: Settings;
  accounts: Account[];
  approvals: Approval[];
  activity: Activity[];
  ai: AIStatus;
  syncing: boolean;
  lastError?: string;
  style?: WritingStyle;
  /** rules.md, "" when the user has none. */
  rules?: string;
  codexInstalled?: boolean;
}
export const categoryLabels: Record<Category, string> = {
  ignore: "Ignore",
  fyi: "FYI",
  acknowledge: "Acknowledgment",
  routine_request: "Routine request",
  needs_decision: "Needs a decision",
  sensitive: "Sensitive",
  urgent: "Urgent",
};
export const providerLabels: Record<Provider, string> = {
  gmail: "Gmail",
  outlook: "Outlook",
  slack: "Slack",
  teams: "Microsoft Teams",
};
