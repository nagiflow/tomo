import { invoke, isTauri } from "@tauri-apps/api/core";
import { previewSnapshot } from "./preview";
import type { Snapshot, Settings, Activity, PlanAction } from "../types";
export const desktop = isTauri();
let preview: Snapshot = previewSnapshot();
if (!desktop) {
  try {
    const saved = sessionStorage.getItem("tomo-preview-v2");
    if (saved) preview = JSON.parse(saved);
  } catch {
    sessionStorage.removeItem("tomo-preview-v2");
  }
}
function addPreviewGmail() {
  preview.accounts.push({
    id: "demo-gmail",
    provider: "gmail",
    email: "you@gmail.com",
    status: "connected",
    failures: 0,
  });
}
function persist() {
  sessionStorage.setItem("tomo-preview-v2", JSON.stringify(preview));
}
export async function command<T = void>(
  name: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (desktop) return invoke<T>(name, args);
  const id = args.id as string;
  switch (name) {
    case "import_google_client":
      preview.settings.googleClientId = "preview";
      if (args.connect) addPreviewGmail();
      break;
    case "choose_codex_dir":
      preview.settings.codexDir = "/Users/you/Projects/site";
      preview.settings.tools = {
        ...preview.settings.tools!,
        codex: "ask",
      };
      break;
    case "connect_workspace_tool":
      preview.settings.tools = {
        calendar: "off",
        drive: "off",
        docs: "off",
        sheets: "off",
        slides: "off",
        codex: "off",
        contacts: "off",
        ...preview.settings.tools,
        [String(args.kind)]: args.access as string,
      } as Settings["tools"];
      break;
    case "save_writing_style":
      preview.style = {
        ...preview.style!,
        content: String(args.content),
        updatedAt: new Date().toISOString(),
      };
      break;
    case "save_rules":
      preview.rules = String(args.content);
      break;
    case "learn_writing_style":
      preview.style = {
        ...preview.style!,
        sampleCount: args.more ? (preview.style?.sampleCount || 0) + 200 : 200,
        updatedAt: new Date().toISOString(),
      };
      break;
    case "show_style_file":
      throw Error("Show in Finder is available in the desktop app.");
    case "revise_plan": {
      const item = preview.approvals.find((a) => a.id === id);
      if (!item) throw Error("Plan no longer exists");
      const note = String(args.instruction).trim();
      item.instruction = note;
      item.revision = (item.revision || 0) + 1;
      const mail =
        item.plan?.find((a) => a.kind === "mail") ||
        ({
          id: "mail-" + item.id,
          kind: "mail",
          operation: "reply",
          title: "Reply",
          summary: "",
          detail: item.draft,
          params: { body: item.draft },
          status: "pending",
        } as PlanAction);
      if (/attach|drive|pdf|share/i.test(note)) {
        item.plan = [
          ...(item.plan || []).filter((a) => a.kind === "calendar"),
          {
            id: crypto.randomUUID(),
            kind: "drive",
            operation: "share",
            title: "Share “Northline renewal.pdf”",
            summary: "View-only · only people on this thread",
            detail:
              "From Drive › Contracts\nShares a view-only link with the sender.",
            params: {
              fileId: "demo-renewal",
              email: item.message.sender.address,
            },
            status: "pending",
          },
          {
            ...mail,
            params: {
              body: "Thanks for checking. Here is the renewal for reference: %STEP_2_URL%. I need to review clause 7 before confirming.",
            },
          },
        ];
      } else if (
        /reschedule|meeting|block|calendar|[0-9].*(pm|am)/i.test(note)
      ) {
        const calendar = item.plan?.find((a) => a.kind === "calendar");
        const matched = note.match(/(\d{1,2})(?::(\d{2}))?\s*(pm|am)/i);
        const hour = matched
          ? (Number(matched[1]) % 12) +
            (matched[3].toLowerCase() === "pm" ? 12 : 0)
          : 17;
        const min = matched ? Number(matched[2] || 0) : 0;
        const start = new Date();
        start.setDate(start.getDate() + 1);
        start.setHours(hour, min, 0, 0);
        const end = new Date(start.getTime() + 60 * 60 * 1000);
        item.plan = [
          {
            id: calendar?.id || crypto.randomUUID(),
            kind: "calendar",
            operation: calendar ? "move" : "create",
            title: calendar?.title || "Block time to follow up",
            summary: "Check your calendar before acting",
            detail: "An updated invite will be sent only after you approve.",
            params: {
              ...calendar?.params,
              start: { dateTime: start.toISOString() },
              end: { dateTime: end.toISOString() },
            },
            status: "pending",
          },
          {
            ...mail,
            detail: "Thanks — I’ve updated the calendar as requested.",
            params: {
              body: "Thanks — I’ve updated the calendar as requested.",
            },
          },
        ];
      } else {
        item.plan = [
          ...item.plan!.filter((a) => a.kind !== "mail"),
          {
            ...mail,
            detail: `Thanks for your message. ${note}`,
            params: { body: `Thanks for your message. ${note}` },
          },
        ];
      }
      item.draft = String(item.plan.at(-1)?.params.body || "");
      break;
    }
    case "approve_plan": {
      const item = preview.approvals.find((a) => a.id === id);
      if (!item || item.status !== "pending")
        throw Error("This plan is no longer pending.");
      item.plan = (args.actions as PlanAction[]).map((a, i) => ({
        ...a,
        status: "completed",
        result: { url: `https://example.com/preview/step-${i + 1}` },
      }));
      item.status = "sent";
      item.draft = String(item.plan.at(-1)?.params.body || "");
      preview.activity.unshift({
        id: crypto.randomUUID(),
        messageId: item.message.id,
        accountId: item.message.accountId,
        provider: item.message.provider,
        sender: item.message.sender.name,
        subject: item.subject,
        category: item.classification.category,
        action: "approved",
        reason: `Preview: approved ${item.plan.length} actions. No external actions were performed.`,
        timestamp: new Date().toISOString(),
        humanApproved: true,
        content: item.draft,
        sendResult: "Preview only",
      });
      break;
    }
    case "get_message":
      return (preview.approvals.find((a) => a.id === id)?.message ?? null) as T;
    case "get_cached_messages":
      return preview.approvals.map((a) => a.message) as T;
    case "get_snapshot":
      return structuredClone(preview) as T;
    case "set_processing_paused":
      preview.settings.paused = args.paused as boolean;
      preview.settings.autoSend = false;
      break;
    case "set_auto_send":
      if (args.enabled && preview.settings.paused)
        throw Error("Resume processing first.");
      preview.settings.autoSend = args.enabled as boolean;
      break;
    case "save_settings":
      {
        const incoming = args.settings as Settings;
        preview.settings = {
          ...incoming,
          googleClientSecret: "",
          paused: preview.settings.paused,
          autoSend:
            preview.settings.autoSend &&
            incoming.consent &&
            !preview.settings.paused,
        };
      }
      break;
    case "ask_next_time": {
      const sender = preview.approvals.find((a) => a.id === id)?.message.sender;
      preview.settings.blockedSenders.push(
        sender?.address || sender?.id || `${id}@example.com`,
      );
      break;
    }
    case "approve_reply":
    case "dismiss_reply": {
      const item = preview.approvals.find((a) => a.id === id);
      if (!item || item.status !== "pending")
        throw Error("This item is no longer pending.");
      if (name === "approve_reply" && !String(args.content).trim())
        throw Error("Write a reply before sending.");
      item.status = name === "approve_reply" ? "sent" : "dismissed";
      item.draft = (args.content as string) || item.draft;
      const event: Activity = {
        id: crypto.randomUUID(),
        messageId: item.message.id,
        accountId: item.message.accountId,
        provider: item.message.provider,
        sender: item.message.sender.name,
        subject: item.subject,
        category: item.classification.category,
        action: name === "approve_reply" ? "approved" : "dismissed",
        reason: "Preview action. No message was sent.",
        timestamp: new Date().toISOString(),
        humanApproved: true,
        content: item.draft,
        sendResult: "Preview only",
      };
      preview.activity.unshift(event);
      break;
    }
    case "redraft_reply": {
      const item = preview.approvals.find((a) => a.id === id);
      if (item)
        item.draft = `Thanks for your message. ${item.classification.category === "sensitive" ? "I need to review this before confirming." : "Could you share more context so I can give you an accurate answer?"}`;
      break;
    }
    case "disconnect_account":
      preview.accounts = preview.accounts.filter((a) => a.id !== id);
      preview.approvals = preview.approvals.filter(
        (a) => a.message.accountId !== id,
      );
      preview.activity = preview.activity.filter((a) => a.accountId !== id);
      break;
    case "clear_history":
      preview.activity = [];
      break;
    case "reset_app":
      preview = previewSnapshot();
      preview.settings.onboarded = false;
      preview.accounts = [];
      preview.approvals = [];
      preview.activity = [];
      break;
    case "restore_preview":
      preview = previewSnapshot();
      break;
    case "export_configuration":
      return JSON.stringify(
        {
          settings: { ...preview.settings, googleClientSecret: undefined },
        },
        null,
        2,
      ) as T;
    case "sync_now":
      break;
    case "open_original":
      throw Error("Preview conversations have no original inbox message.");
    case "connect_account":
      // re-consent for an already connected preview inbox has nothing to do
      if (preview.accounts.some((a) => a.provider === args.provider)) break;
      if (args.provider === "gmail") {
        addPreviewGmail();
        break;
      }
    // falls through
    case "connect_ai":
    case "connect_slack":
    case "set_up_slack_app":
    case "open_codex_file":
    case "slack_manifest":
    case "save_api_key":
    case "list_models":
    case "reconcile_reply":
      throw Error(
        "Account connections are available in the desktop app. Run pnpm desktop:dev.",
      );
    case "disconnect_ai":
      preview.ai.connected = false;
      preview.settings.consent = false;
      break;
    case "show_main":
      location.hash = "";
      break;
    default:
      throw Error(`Unknown action: ${name}`);
  }
  persist();
  return undefined as T;
}
