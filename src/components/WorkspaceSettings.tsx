import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import {
  CalendarDays,
  Sheet,
  Presentation,
  Files,
  FileText,
  Mail,
  AlertCircle,
  CheckCircle2,
  ArrowUpRight,
  Terminal,
} from "lucide-react";
import { useApp } from "../stores/app";
import { desktop } from "../services/api";
import type { ToolKind, ToolMode } from "../types";
import { toolLabels } from "../types";
import { Switch } from "./ui";
const tools: {
  kind: ToolKind;
  description: string;
  icon: typeof CalendarDays;
}[] = [
  {
    kind: "calendar",
    description: "See your schedule, move and create events",
    icon: CalendarDays,
  },
  {
    kind: "drive",
    description: "Find files and share view-only links in replies",
    icon: Files,
  },
  {
    kind: "docs",
    description: "Start meeting notes and longer drafts",
    icon: FileText,
  },
  {
    kind: "sheets",
    description: "Start a spreadsheet from the numbers in a thread",
    icon: Sheet,
  },
  {
    kind: "slides",
    description: "Draft a deck outline you can finish yourself",
    icon: Presentation,
  },
];
/** The Google tools step of onboarding: a switch per tool, asked for in one Google sign-in. */
export function ToolsSettings() {
  const { snapshot, run, busy } = useApp();
  if (!snapshot) return null;
  const s = snapshot.settings;
  const google = snapshot.accounts.filter((a) => a.provider === "gmail");
  return (
    <>
      <div className={`google-connected ${google.length ? "" : "missing"}`}>
        {google.length ? <CheckCircle2 size={16} /> : <AlertCircle size={16} />}
        <span className="grow">
          <strong>
            {google.length ? "Google connected" : "Google not connected"}
          </strong>
          {google.length
            ? ` · ${google[0].email}`
            : s.googleClientId
              ? " · choose what to connect, then sign in once"
              : " · tools start working once you connect"}
        </span>
        {s.googleProjectId && <code>{s.googleProjectId}</code>}
      </div>
      <div className="settings-group tools-group">
        <div className="settings-row">
          <span className="tool-icon">
            <Mail size={17} />
          </span>
          <div className="grow">
            <h3>Gmail</h3>
            <p>Watch your inbox, draft and send as you</p>
          </div>
          {/* ponytail: Gmail is the inbox itself, so it follows the Google connection; make it a real toggle when Outlook-only setups need Google tools without Gmail */}
          <Switch
            checked={!!google.length}
            label="Gmail"
            disabled
            onChange={() => {}}
          />
        </div>
        {tools
          .filter((t) =>
            ["calendar", "drive", "sheets", "slides"].includes(t.kind),
          )
          .map((t) => {
            const mode = s.tools?.[t.kind] || "off";
            return (
              <div className="settings-row" key={t.kind}>
                <span className="tool-icon">
                  <t.icon size={17} />
                </span>
                <div className="grow">
                  <h3>{toolLabels[t.kind]}</h3>
                  <p>{t.description}</p>
                </div>
                <Switch
                  checked={mode !== "off"}
                  label={toolLabels[t.kind]}
                  disabled={busy}
                  onChange={(v) =>
                    run("save_settings", {
                      settings: {
                        ...s,
                        tools: { ...s.tools, [t.kind]: v ? "ask" : "off" },
                      },
                    })
                  }
                />
              </div>
            );
          })}
      </div>
    </>
  );
}
/** The Google tools inside the Google card in Settings, each with its own permission. */
export function GoogleToolRows() {
  const { snapshot, run, busy } = useApp();
  if (!snapshot) return null;
  const s = snapshot.settings;
  const google = snapshot.accounts.filter((a) => a.provider === "gmail");
  return (
    <>
      {tools.map((t) => {
        const mode = s.tools?.[t.kind] || "off";
        return (
          <div className="settings-row service-row" key={t.kind}>
            <span className="tool-icon">
              <t.icon size={17} />
            </span>
            <div className="grow">
              <h3>{toolLabels[t.kind]}</h3>
              <p>{t.description}</p>
            </div>
            {mode === "off" ? (
              <button
                className="button"
                disabled={busy || !google.length}
                onClick={() =>
                  run("connect_workspace_tool", {
                    kind: t.kind,
                    access: "ask",
                  })
                }
              >
                Connect…
              </button>
            ) : (
              <select
                aria-label={`${toolLabels[t.kind]} permission`}
                value={mode}
                disabled={busy}
                onChange={(e) =>
                  run("connect_workspace_tool", {
                    kind: t.kind,
                    access: e.target.value as ToolMode,
                  })
                }
              >
                <option value="ask">Ask before acting</option>
                {["calendar", "drive"].includes(t.kind) && (
                  <option value="read">Read only</option>
                )}
                <option value="off">Disconnect</option>
              </select>
            )}
          </div>
        );
      })}
      {/* ponytail: one account for every Google tool; make it per tool when someone needs Calendar from one account and Drive from another */}
      {google.length > 1 && (
        <div className="settings-row service-row">
          <label htmlFor="tools-account">Google tools work through</label>
          <select
            id="tools-account"
            value={s.googleAccountId || google[0].id}
            disabled={busy}
            onChange={(e) =>
              run("save_settings", {
                settings: { ...s, googleAccountId: e.target.value },
              })
            }
          >
            {google.map((a) => (
              <option key={a.id} value={a.id}>
                {a.email}
              </option>
            ))}
          </select>
        </div>
      )}
    </>
  );
}
/** Codex runs on this Mac, inside one folder the user picks. */
export function CodexRow() {
  const { snapshot, run, busy } = useApp();
  if (!snapshot) return null;
  const s = snapshot.settings;
  return (
    <div className="settings-row service-row">
      <span className="tool-icon">
        <Terminal size={17} />
      </span>
      <div className="grow">
        <h3>{toolLabels.codex}</h3>
        <p>
          {s.tools?.codex === "ask" && s.codexDir
            ? `Works only inside ${s.codexDir.replace(/^\/Users\/[^/]+/, "~")}`
            : "Run approved tasks on this Mac inside one folder you choose, such as building a file or fixing a bug"}
        </p>
      </div>
      {!snapshot.codexInstalled ? (
        <span className="muted">Codex CLI not found</span>
      ) : (
        <>
          <button
            className="button"
            disabled={busy}
            onClick={() => run("choose_codex_dir")}
          >
            {s.tools?.codex === "ask" ? "Change Folder…" : "Choose Folder…"}
          </button>
          {s.tools?.codex === "ask" && (
            <button
              className="button"
              disabled={busy}
              onClick={() =>
                run("save_settings", {
                  settings: { ...s, tools: { ...s.tools, codex: "off" } },
                })
              }
            >
              Turn Off
            </button>
          )}
        </>
      )}
    </div>
  );
}
const rulesExample =
  "# Rules\n\n- When an invoice arrives by email, post the sender, amount and due date in #finance. Post it without asking me.\n- When a customer emails about an outage, post a summary in #support and wait for my OK.\n";
// rules.md: standing instructions, kept apart from style.md because relearning the style rewrites that file.
export function Rules() {
  const { snapshot, run } = useApp();
  const saved = snapshot?.rules ?? "";
  const [text, setText] = useState(saved);
  useEffect(() => setText(saved), [saved]);
  if (!snapshot) return null;
  return (
    <div className="writing-style">
      <div className="style-meta">
        <FileText size={15} />
        <label htmlFor="rules">rules.md</label>
        <span className="grow">
          Read with every message. Leave empty for no rules.
        </span>
      </div>
      <textarea
        id="rules"
        className="style-editor"
        spellCheck={false}
        placeholder={rulesExample}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          if (text !== saved) void run("save_rules", { content: text });
        }}
        rows={8}
      />
    </div>
  );
}
export function WritingStyle({ onboarding = false }: { onboarding?: boolean }) {
  const { snapshot, run, busy } = useApp();
  const style = snapshot?.style;
  const [text, setText] = useState(
    style?.content ||
      "# How I write\n\n## Tone\n- Warm but brief.\n\n## Never\n- Promise dates or prices I haven’t confirmed.",
  );
  useEffect(() => {
    if (style) setText(style.content);
  }, [style?.content]);
  if (!snapshot) return null;
  const samples = style?.sampleCount || 0;
  const edited =
    style?.updatedAt &&
    new Date(style.updatedAt).toDateString() === new Date().toDateString();
  return (
    <>
      <div className="writing-style">
        <div className="style-meta">
          <FileText size={15} />
          <label htmlFor="style">style.md</label>
          <span className="grow">
            {onboarding
              ? "Edit anything that’s off"
              : `${
                  samples
                    ? `${desktop ? "Learned from" : "Preview example from"} ${samples} sent emails`
                    : "Edit it yourself, or learn it from recent sent mail"
                }${style?.updatedAt ? ` · ${edited ? "edited today" : "saved on this Mac"}` : ""}`}
          </span>
          {!onboarding && (
            <>
              <button
                className="button small"
                disabled={
                  busy ||
                  !snapshot.accounts.length ||
                  !snapshot.ai.connected ||
                  !snapshot.settings.consent
                }
                onClick={() => run("learn_writing_style", { more: false })}
              >
                Relearn from Recent Mail…
              </button>
              <button
                className="button small"
                disabled={busy}
                onClick={() => run("show_style_file")}
              >
                Show in Finder
              </button>
            </>
          )}
        </div>
        <textarea
          id="style"
          className="style-editor"
          spellCheck={false}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => {
            if (text !== style?.content)
              void run("save_writing_style", { content: text });
          }}
          rows={onboarding ? 12 : 16}
        />
      </div>
      {onboarding && (
        <p className="style-disclosure">
          {samples && desktop
            ? "Samples of your messages were sent"
            : "Learning sends samples of your sent messages"}{" "}
          to {snapshot.settings.aiProvider === "siwc" ? "ChatGPT" : "OpenAI"} to
          write this summary. Only style.md is kept, on this Mac.
        </p>
      )}
    </>
  );
}
const cloudConsole = "https://console.cloud.google.com/";
const apis =
  "gmail.googleapis.com,calendar-json.googleapis.com,drive.googleapis.com,docs.googleapis.com,sheets.googleapis.com,slides.googleapis.com";
// One screen per step: what to do in plain words, the Google page to open, and a sketch of
// that page with the thing to click highlighted. `page` is the key open_setup_page accepts.
type Line = { label: string; value?: string; kind?: "field" | "choice" };
const guide: {
  page?: string;
  path?: string;
  title: string;
  text: string;
  sketch: { heading: string; lines: Line[]; click: string };
}[] = [
  {
    page: "google-project",
    path: "projectcreate",
    title: "Create a project",
    text: "Google keeps your sign-in settings in something it calls a project. Type any name, such as Tomo, and click Create. It is free and nobody else can see it.",
    sketch: {
      heading: "New Project",
      lines: [{ label: "Project name", value: "Tomo", kind: "field" }],
      click: "Create",
    },
  },
  {
    page: "google-apis",
    path: `flows/enableapi?apiid=${apis}`,
    title: "Turn on the Google services",
    text: "This page lists the services Tomo talks to. Check that your new project is the one selected, click Next, then Enable.",
    sketch: {
      heading: "Enable access to APIs",
      lines: [
        { label: "Project", value: "Tomo", kind: "choice" },
        { label: "Gmail, Calendar, Drive, Docs, Sheets, Slides" },
      ],
      click: "Enable",
    },
  },
  {
    page: "google-consent",
    path: "auth/overview",
    title: "Tell Google about the app",
    text: "Click Get started. App name: Tomo. Support email: your own. For Audience choose External. Type your email once more as the contact, tick the agreement, and click Create.",
    sketch: {
      heading: "Project configuration",
      lines: [
        { label: "App name", value: "Tomo", kind: "field" },
        { label: "Audience", value: "External", kind: "choice" },
      ],
      click: "Create",
    },
  },
  {
    page: "google-publish",
    path: "auth/audience",
    title: "Publish the app",
    text: "Click Publish app, then Confirm. Do not skip this: while the status says Testing, Google signs Tomo out every 7 days. You do not need to send anything for verification.",
    sketch: {
      heading: "Audience",
      lines: [{ label: "Publishing status", value: "Testing" }],
      click: "Publish app",
    },
  },
  {
    page: "google-client",
    path: "auth/clients/create",
    title: "Create the key file",
    text: "Application type: Desktop app. Name: Tomo. Click Create. A box appears: click Download JSON. The file lands in your Downloads folder.",
    sketch: {
      heading: "Create OAuth client ID",
      lines: [
        { label: "Application type", value: "Desktop app", kind: "choice" },
        { label: "Name", value: "Tomo", kind: "field" },
      ],
      click: "Download JSON",
    },
  },
  {
    title: "Give the file to Tomo",
    text: "Click Choose File… below and pick the file you just downloaded; its name starts with client_secret. When your browser opens to sign in, Google will warn that it hasn’t verified the app. That is expected, because the app is your own: click Advanced, then Go to Tomo.",
    sketch: {
      heading: "Google hasn’t verified this app",
      lines: [{ label: "Advanced" }],
      click: "Go to Tomo (unsafe)",
    },
  },
];
// `footer` is the onboarding window's button row; without it (Settings dialog) the buttons render inline.
export function GoogleSetupView({
  onComplete,
  footer,
  onBack,
  onSkip,
}: {
  onComplete?: () => void;
  footer?: HTMLElement | null;
  onBack?: () => void;
  onSkip?: () => void;
}) {
  const { snapshot, run, busy } = useApp();
  const [index, setIndex] = useState(0);
  const step = guide[index];
  // An OAuth client imported earlier (its account was removed, or sign-in was cancelled) can be reused.
  const imported = !!snapshot?.settings.googleClientId;
  const importClient = async () => {
    // First-run setup signs in after the services are chosen, so Google asks only once.
    if (await run("import_google_client", { connect: footer === undefined }))
      onComplete?.();
  };
  const place = (buttons: ReactNode) =>
    footer === undefined ? (
      <div className="google-actions">{buttons}</div>
    ) : (
      footer && createPortal(buttons, footer)
    );
  return (
    <div className="google-options">
      {footer !== undefined && (
        <header>
          <h1>Connect your Google account</h1>
          <p>
            Tomo signs in through a Google Cloud project that you own, so
            nothing passes through anyone else’s servers. A one-time setup of
            about five minutes.
          </p>
        </header>
      )}
      <section className="google-guide" aria-label="Google setup guide">
        <div className="guide-text">
          <span className="eyebrow">
            Step {index + 1} of {guide.length}
          </span>
          <h2>{step.title}</h2>
          <p>{step.text}</p>
          {step.path && (
            <a
              className="button"
              href={cloudConsole + step.path}
              target="_blank"
              rel="noreferrer"
              onClick={(e) => {
                if (desktop) {
                  e.preventDefault();
                  void run("open_setup_page", { provider: step.page });
                }
              }}
            >
              Open this page in Google
              <ArrowUpRight size={13} />
            </a>
          )}
        </div>
        {/* A drawing of the Google page, not a screenshot: the words above carry the same instructions. */}
        <div className="guide-sketch" aria-hidden="true">
          <div className="sketch-bar">
            <i />
            <i />
            <i />
            <span>Google Cloud</span>
          </div>
          <strong>{step.sketch.heading}</strong>
          {step.sketch.lines.map((line) => (
            <div className={`sketch-line ${line.kind || ""}`} key={line.label}>
              <small>{line.label}</small>
              {line.value && <span>{line.value}</span>}
            </div>
          ))}
          <div className="sketch-click">
            <span>{step.sketch.click}</span>
            <em>Click here</em>
          </div>
        </div>
        <div className="guide-nav">
          <button
            className="button small"
            disabled={index === 0}
            onClick={() => setIndex(index - 1)}
          >
            Previous
          </button>
          <div className="step-dots">
            {guide.map((g, n) => (
              <span key={g.title} className={n === index ? "current" : ""} />
            ))}
          </div>
          <button
            className="button small"
            disabled={index === guide.length - 1}
            onClick={() => setIndex(index + 1)}
          >
            Next
          </button>
        </div>
      </section>
      <p className="google-explanation">
        Using a work account? Your Google Workspace admin may need to allow the
        app first.
      </p>
      {place(
        <>
          {onBack && (
            <button className="button" disabled={busy} onClick={onBack}>
              Back
            </button>
          )}
          {onSkip && (
            <button className="button" disabled={busy} onClick={onSkip}>
              Connect later
            </button>
          )}
          {imported && (
            <button
              className="button"
              disabled={busy}
              onClick={async () => {
                if (await run("connect_account", { provider: "gmail" }))
                  onComplete?.();
              }}
            >
              Sign In Again
            </button>
          )}
          <button
            className={`button ${index === guide.length - 1 || imported ? "primary" : ""}`}
            disabled={busy}
            onClick={importClient}
          >
            Choose File…
          </button>
        </>,
      )}
    </div>
  );
}
