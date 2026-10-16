import {
  GoogleToolRows,
  CodexRow,
  WritingStyle,
  Rules,
  GoogleSetupView,
} from "./WorkspaceSettings";
import { StoredData } from "./StoredData";
import { useEffect, useState } from "react";
import {
  Check,
  Monitor,
  Cloud,
  ArrowUpRight,
  Mail,
  HardDrive,
  Users,
} from "lucide-react";
import { useApp } from "../stores/app";
import { command, desktop } from "../services/api";
import type { Settings } from "../types";
import { Modal, Switch } from "./ui";
type Provider = "gmail" | "outlook" | "slack";
/**
 * Everything Tomo works through, grouped by who provides it. A provider's accounts come first,
 * then the services that run through them, so a new service joins its provider's card.
 */
export function Integrations() {
  const { snapshot, run, busy } = useApp();
  const [setupOpen, setSetupOpen] = useState(false);
  // "update" pastes a new token into the Slack app already connected, e.g. after adding a permission
  const [slackOpen, setSlackOpen] = useState<false | "new" | "update">(false);
  if (!snapshot) return null;
  const googleReady = snapshot.settings.googleClientId;
  // Once the user's Slack app is set up, every further workspace is one browser sign-in.
  const slackApp = snapshot.settings.slackClientId;
  const connect = (p: Provider) =>
    p === "slack" && !slackApp
      ? setSlackOpen("new")
      : p === "gmail" && !googleReady
        ? setSetupOpen(true)
        : run("connect_account", { provider: p });
  const accounts = (p: Provider) =>
    snapshot.accounts.filter((a) => a.provider === p);
  const head = (p: Provider, name: string, empty: string, add: string) => (
    <div className="settings-row integration-head">
      <span className="provider-icon">{name[0]}</span>
      <div className="grow">
        <h3>{name}</h3>
        {!accounts(p).length && <p>{empty}</p>}
      </div>
      <button className="button" disabled={busy} onClick={() => connect(p)}>
        {accounts(p).length ? add : "Connect…"}
      </button>
    </div>
  );
  const accountRows = (p: Provider) =>
    accounts(p).map((a) => (
      <div className="settings-row account-row" key={a.id}>
        <div className="grow">
          <h3>{a.email}</h3>
          <p>
            {a.status === "connected" && !a.error
              ? a.lastSync
                ? `Last checked ${new Intl.DateTimeFormat(undefined, {
                    timeStyle: "short",
                  }).format(new Date(a.lastSync))}`
                : "Connected"
              : a.error || "Reconnect to continue"}
          </p>
        </div>
        {(a.status !== "connected" || a.error) && (
          <button className="button" disabled={busy} onClick={() => connect(p)}>
            Reconnect
          </button>
        )}
        {/* a workspace connected with a pasted token, before the app could sign in */}
        {p === "slack" && !slackApp && a.status === "connected" && (
          <button
            className="button"
            disabled={busy}
            onClick={() => setSlackOpen("update")}
          >
            Update token…
          </button>
        )}
        <RemoveAccount id={a.id} email={a.email} />
      </div>
    ));
  const service = (
    Icon: typeof Mail,
    name: string,
    description: string,
    status: string,
  ) => (
    <div className="settings-row service-row">
      <span className="tool-icon">
        <Icon size={17} />
      </span>
      <div className="grow">
        <h3>{name}</h3>
        <p>{description}</p>
      </div>
      <span className="muted">{status}</span>
    </div>
  );
  return (
    <>
      <Modal
        open={setupOpen}
        onClose={() => setSetupOpen(false)}
        title="Connect your Google account"
      >
        <GoogleSetupView onComplete={() => setSetupOpen(false)} />
      </Modal>
      <Modal
        open={!!slackOpen}
        onClose={() => setSlackOpen(false)}
        title={
          slackOpen === "update" ? "Update the Slack token" : "Connect Slack"
        }
      >
        <SlackSetup
          update={slackOpen === "update"}
          onComplete={() => setSlackOpen(false)}
        />
      </Modal>
      <div className="integrations">
        <div className="settings-group">
          {head(
            "gmail",
            "Google",
            "Gmail and Google Workspace",
            "Add account…",
          )}
          {accountRows("gmail")}
          {service(
            Mail,
            "Gmail",
            "Watch each account's inbox, draft and send as you",
            accounts("gmail").length ? "On" : "Off",
          )}
          <GoogleToolRows />
        </div>
        <div className="settings-group">
          {head(
            "outlook",
            "Microsoft",
            "Personal or work Microsoft account",
            "Add account…",
          )}
          {accountRows("outlook")}
          {service(
            Mail,
            "Outlook",
            "Watch each account's inbox, draft and send as you",
            accounts("outlook").length ? "On" : "Off",
          )}
          {service(
            HardDrive,
            "OneDrive",
            "Find and share files",
            "Later release",
          )}
          {service(
            Users,
            "Microsoft Teams",
            "Messages and mentions",
            "Later release",
          )}
        </div>
        <div className="settings-group">
          {head(
            "slack",
            "Slack",
            "Messages and mentions, through your own Slack app",
            "Add workspace…",
          )}
          {accountRows("slack")}
        </div>
        <div className="settings-group">
          <div className="settings-row integration-head">
            <span className="provider-icon">
              <Monitor size={15} />
            </span>
            <div className="grow">
              <h3>This Mac</h3>
            </div>
          </div>
          <CodexRow />
        </div>
      </div>
    </>
  );
}
/**
 * Slack needs no admin when the app is the user's own. Set up once with its Client ID and
 * App-Level Token, it signs in to any number of workspaces in the browser. "update" is the older
 * way, still there for a workspace connected by pasting a user token.
 */
function SlackSetup({
  onComplete,
  update = false,
}: {
  onComplete: () => void;
  update?: boolean;
}) {
  const { run, busy } = useApp();
  const [clientId, setClientId] = useState("");
  const [token, setToken] = useState("");
  const [appToken, setAppToken] = useState("");
  const [manifest, setManifest] = useState("");
  useEffect(() => {
    command<string>("slack_manifest")
      .then(setManifest)
      .catch(() => {});
  }, []);
  const appTokenField = (
    <label>
      App-Level Token
      <input
        type="password"
        value={appToken}
        onChange={(e) => setAppToken(e.target.value)}
        placeholder="xapp-…"
        autoComplete="off"
      />
      <small>
        Lets Slack tell Tomo about new messages as they arrive. Without it Tomo
        only finds direct messages, by checking every few minutes.
        {update && " Leave it empty to keep the current one."}
      </small>
    </label>
  );
  if (update)
    return (
      <form
        className="form"
        onSubmit={async (e) => {
          e.preventDefault();
          if (await run("connect_slack", { token, appToken })) onComplete();
        }}
      >
        <ol>
          <li>
            Open your Tomo app at{" "}
            <a
              href="https://api.slack.com/apps"
              target="_blank"
              rel="noreferrer"
            >
              api.slack.com/apps
            </a>{" "}
            and go to <strong>OAuth &amp; Permissions</strong>.
          </li>
          <li>
            Under <strong>User Token Scopes</strong>, add any that are missing:{" "}
            <code>channels:read</code>, <code>groups:read</code> and{" "}
            <code>search:read</code>.
          </li>
          <li>
            Click <strong>Reinstall to Workspace</strong> at the top of the
            page, then copy the <strong>User OAuth Token</strong> and paste it
            below.
          </li>
        </ol>
        {appTokenField}
        <label>
          User OAuth Token
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="xoxp-…"
            autoComplete="off"
          />
          <small>Both tokens are stored encrypted on this computer.</small>
        </label>
        <button className="button" disabled={busy || !token.trim()}>
          Save token
        </button>
      </form>
    );
  return (
    <form
      className="form"
      onSubmit={async (e) => {
        e.preventDefault();
        if (await run("set_up_slack_app", { clientId, appToken })) onComplete();
      }}
    >
      <p>
        Tomo reads and answers your Slack messages as you, through a small Slack
        app that only you use: direct messages, mentions of you in channels, and
        replies in your threads. You set the app up once; after that, adding a
        workspace is a sign-in in your browser.
      </p>
      <ol>
        <li>
          <a
            onClick={(e) => {
              if (desktop) {
                e.preventDefault();
                void run("open_setup_page", { provider: "slack" });
              }
            }}
            href="https://api.slack.com/apps?new_app=1"
            target="_blank"
            rel="noreferrer"
          >
            Create the Slack app
          </a>{" "}
          in your workspace. The permissions and sign-in address are already
          filled in.
        </li>
        <li>
          On <strong>Basic Information</strong>, copy the{" "}
          <strong>Client ID</strong> under App Credentials, and under{" "}
          <strong>App-Level Tokens</strong> generate a token with the{" "}
          <code>connections:write</code> scope. Paste both below.
        </li>
        <li>
          To add more than one workspace, open{" "}
          <strong>Manage Distribution</strong> and activate public distribution.
          Nobody else can find or install the app unless you share its link.
        </li>
      </ol>
      <details>
        <summary>Already created the app?</summary>
        <p>
          Under <strong>OAuth &amp; Permissions</strong>, turn on{" "}
          <strong>PKCE</strong> (Slack does not let you turn it off again), add
          the redirect URL <code>tomo://slack/callback</code>, and add{" "}
          <code>channels:read</code>, <code>groups:read</code> and{" "}
          <code>search:read</code> to the User Token Scopes. Save, then fill in
          below.
        </p>
      </details>
      <label>
        Client ID
        <input
          value={clientId}
          onChange={(e) => setClientId(e.target.value)}
          placeholder="1234567890.1234567890"
          autoComplete="off"
        />
      </label>
      {appTokenField}
      <details>
        <summary>The permissions were not filled in?</summary>
        <p>
          Choose <strong>Create New App</strong>, then{" "}
          <strong>From a manifest</strong>, and paste this:
        </p>
        <textarea readOnly rows={4} value={manifest} />
      </details>
      <button className="button" disabled={busy || !clientId.trim()}>
        Sign in with Slack
      </button>
    </form>
  );
}
function RemoveAccount({ id, email }: { id: string; email: string }) {
  const [open, setOpen] = useState(false);
  const { run, busy } = useApp();
  return (
    <>
      <button className="button" disabled={busy} onClick={() => setOpen(true)}>
        Remove…
      </button>
      <Modal
        open={open}
        onClose={() => setOpen(false)}
        title="Remove this inbox?"
      >
        <p>
          Disconnect {email} and delete its cached messages, drafts, and history
          from this Mac. Previously sent messages remain in your inbox. You can
          also revoke Tomo’s permissions in the provider’s account settings.
        </p>
        <div className="modal-actions">
          <button className="button" onClick={() => setOpen(false)}>
            Cancel
          </button>
          <button
            className="button destructive"
            disabled={busy}
            onClick={async () => {
              if (await run("disconnect_account", { id })) setOpen(false);
            }}
          >
            Remove inbox
          </button>
        </div>
      </Modal>
    </>
  );
}
export function AISettings({ setup = false }: { setup?: boolean }) {
  const { snapshot, run, busy } = useApp();
  const [apiKey, setApiKey] = useState("");
  const connected = snapshot?.ai.connected;
  // Keep the model list current without a Refresh button; a failed fetch keeps the saved list.
  useEffect(() => {
    if (connected)
      void command("list_models")
        .then(() => useApp.getState().load())
        .catch(() => {});
  }, [connected]);
  if (!snapshot) return null;
  const s = snapshot.settings;
  return (
    <div className="settings-group">
      <div className="settings-row">
        <div className="grow">
          <h3>
            {snapshot.ai.connected
              ? snapshot.ai.provider === "siwc"
                ? "Signed in with ChatGPT"
                : "OpenAI API connected"
              : "Choose how Tomo thinks"}
          </h3>
          <p>{snapshot.ai.email || snapshot.ai.status}</p>
        </div>
        {snapshot.ai.connected ? (
          <>
            <span className="status success">
              <Check size={13} />
              Connected
            </span>
            {!setup && (
              <button
                className="button"
                disabled={busy}
                onClick={() => run("disconnect_ai")}
              >
                Disconnect
              </button>
            )}
          </>
        ) : (
          s.aiProvider === "siwc" && (
            <button
              className="button primary"
              disabled={busy}
              onClick={() => run("connect_ai")}
            >
              Continue with ChatGPT
            </button>
          )
        )}
      </div>
      <div className="settings-row">
        <label htmlFor="provider">Provider</label>
        <select
          id="provider"
          value={s.aiProvider}
          onChange={(e) =>
            run("save_settings", {
              settings: { ...s, aiProvider: e.target.value, model: "" },
            })
          }
          disabled={busy}
        >
          <option value="siwc">Sign in with ChatGPT</option>
          <option value="openai">OpenAI API key</option>
          <option disabled>Local model (coming later)</option>
        </select>
      </div>
      {s.aiProvider === "openai" && (
        <form
          className="settings-row"
          onSubmit={async (e) => {
            e.preventDefault();
            if (await run("save_api_key", { key: apiKey })) setApiKey("");
          }}
        >
          <label htmlFor="api-key">API key</label>
          <input
            id="api-key"
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="Stored encrypted on this Mac"
            autoComplete="off"
            required
          />
          <button className="button" disabled={busy || !apiKey.trim()}>
            Save key
          </button>
        </form>
      )}
      <div className="settings-row">
        <label htmlFor="model">Model</label>
        <select
          id="model"
          value={s.model}
          onChange={(e) =>
            run("save_settings", { settings: { ...s, model: e.target.value } })
          }
          disabled={busy || !snapshot.ai.models.length}
        >
          <option value="">Choose a model</option>
          {snapshot.ai.models.map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </select>
      </div>
      <div className="settings-row">
        <div className="grow">
          <h3>Thinking level</h3>
          <p>
            Higher levels reason longer: slower, and they use more of your plan.
          </p>
        </div>
        <select
          aria-label="Thinking level"
          value={s.reasoningEffort || ""}
          onChange={(e) =>
            run("save_settings", {
              settings: { ...s, reasoningEffort: e.target.value },
            })
          }
          disabled={busy}
        >
          <option value="">Default</option>
          <option value="low">Low</option>
          <option value="medium">Medium</option>
          <option value="high">High</option>
        </select>
      </div>
    </div>
  );
}
export function ClientSettings() {
  const { snapshot, run, busy } = useApp();
  const [config, setConfig] = useState<
    Pick<
      Settings,
      "googleClientId" | "googleClientSecret" | "microsoftClientId"
    >
  >({
    googleClientId: snapshot?.settings.googleClientId || "",
    googleClientSecret: "",
    microsoftClientId: snapshot?.settings.microsoftClientId || "",
  });
  return (
    <details className="client-setup">
      <summary>Use your own OAuth app registrations</summary>
      <p>
        Create a Google Desktop OAuth client with Gmail enabled, and a Microsoft
        public-client registration allowing personal and work accounts. Sign-in
        uses a local callback in your system browser. Your organization may
        require consent.
      </p>
      <div className="setup-links">
        <a
          onClick={(e) => {
            if (desktop) {
              e.preventDefault();
              void run("open_setup_page", { provider: "gmail" });
            }
          }}
          href="https://console.cloud.google.com/apis/credentials"
          target="_blank"
          rel="noreferrer"
        >
          Google Cloud <ArrowUpRight size={12} />
        </a>
        <a
          onClick={(e) => {
            if (desktop) {
              e.preventDefault();
              void run("open_setup_page", { provider: "outlook" });
            }
          }}
          href="https://portal.azure.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade"
          target="_blank"
          rel="noreferrer"
        >
          Microsoft Entra <ArrowUpRight size={12} />
        </a>
      </div>
      <form
        className="form"
        onSubmit={async (e) => {
          e.preventDefault();
          if (
            snapshot &&
            (await run("save_settings", {
              settings: { ...snapshot.settings, ...config },
            }))
          )
            setConfig({ ...config, googleClientSecret: "" });
        }}
      >
        <label>
          Google Desktop client ID
          <input
            value={config.googleClientId}
            onChange={(e) =>
              setConfig({ ...config, googleClientId: e.target.value })
            }
            placeholder="….apps.googleusercontent.com"
          />
        </label>
        <label>
          Google Desktop client secret
          <input
            type="password"
            value={config.googleClientSecret}
            onChange={(e) =>
              setConfig({ ...config, googleClientSecret: e.target.value })
            }
            placeholder="Stored encrypted · leave blank to keep saved secret"
            autoComplete="off"
          />
        </label>
        <label>
          Microsoft application (client) ID
          <input
            value={config.microsoftClientId}
            onChange={(e) =>
              setConfig({ ...config, microsoftClientId: e.target.value })
            }
            placeholder="Application UUID"
          />
        </label>
        <p>
          Microsoft: register <code>http://localhost/auth/callback</code> as a
          Mobile and desktop redirect URI. No Microsoft client secret is needed.
        </p>
        <button className="button" disabled={busy}>
          Save account configuration
        </button>
      </form>
    </details>
  );
}
export function SettingsView() {
  const { snapshot, run, busy } = useApp();
  const [confirm, setConfirm] = useState<"reset" | "history" | "data" | null>(
    null,
  );
  const [privacy, setPrivacy] = useState(false);
  if (!snapshot) return null;
  const s = snapshot.settings;
  async function exportRules() {
    try {
      const text = await command<string>("export_configuration");
      const link = document.createElement("a");
      link.href = URL.createObjectURL(
        new Blob([text], { type: "application/json" }),
      );
      link.download = "tomo-configuration.json";
      link.click();
      setTimeout(() => URL.revokeObjectURL(link.href), 1000);
    } catch (e) {
      useApp.setState({ error: String(e) });
    }
  }
  return (
    <main className="page settings-page">
      <header className="page-header">
        <h1>Settings</h1>
        <span className="muted">Community Edition · 0.1.0</span>
      </header>
      <section>
        <h2 className="section-title">AI Provider</h2>
        <AISettings />
      </section>
      <section>
        <h2 className="section-title">Your Style</h2>
        <p className="section-description">
          How you write, in your words. Tomo reads this before every draft so
          replies sound like you.
        </p>
        <WritingStyle />
      </section>
      <section>
        <h2 className="section-title">Your Rules</h2>
        <p className="section-description">
          What to do with certain messages, in your words. Say when a rule may
          run without asking you; otherwise Tomo asks first.
        </p>
        <Rules />
      </section>
      <section>
        <h2 className="section-title">Integrations</h2>
        <p className="section-description">
          The accounts Tomo works through. Every connected inbox is watched;
          each tool asks for its own permission.
        </p>
        <Integrations />
      </section>
      <section>
        <h2 className="section-title">Checking and Replying</h2>
        <div className="settings-group">
          <div className="settings-row">
            <div className="grow">
              <h3>Check for new mail</h3>
              <p>While your Mac is awake and Tomo is running</p>
            </div>
            <select
              aria-label="Polling interval"
              value={s.pollSeconds}
              disabled={busy}
              onChange={(e) =>
                run("save_settings", {
                  settings: { ...s, pollSeconds: +e.target.value },
                })
              }
            >
              <option value={120}>Every 2 minutes</option>
              <option value={300}>Every 5 minutes</option>
              <option value={900}>Every 15 minutes</option>
            </select>
          </div>
          <div className="settings-row">
            <div className="grow">
              <h3>Messages handled at once</h3>
              <p>
                More clears a burst of mail sooner and uses your AI plan’s
                limits faster.
              </p>
            </div>
            <select
              aria-label="Messages handled at once"
              value={s.workers || 1}
              disabled={busy}
              onChange={(e) =>
                run("save_settings", {
                  settings: { ...s, workers: +e.target.value },
                })
              }
            >
              {[1, 2, 3, 4, 6, 8].map((n) => (
                <option key={n} value={n}>
                  {n === 1 ? "One at a time" : `${n} at a time`}
                </option>
              ))}
            </select>
          </div>
          <div className="settings-row">
            <div className="grow">
              <h3>YOLO mode</h3>
              <p className={s.yolo ? "danger-text" : undefined}>
                Every plan runs as soon as Tomo makes it, with no approval:
                replies, Slack posts, calendar changes, file shares and Codex
                tasks. Only “Never automate” for a sender and Pause still stop
                it.
              </p>
            </div>
            <Switch
              checked={!!s.yolo}
              onChange={(yolo) =>
                run("save_settings", { settings: { ...s, yolo } })
              }
              label="YOLO mode"
              disabled={busy}
            />
          </div>
          <div className="settings-row">
            <div className="grow">
              <h3>Replies Tomo sends alone</h3>
              <p>
                Simple replies to people you have emailed before, and to Slack
                DMs and mentions.
              </p>
            </div>
            <select
              aria-label="Replies Tomo sends alone"
              value={s.autoSend ? "easy" : "ask"}
              disabled={busy}
              onChange={(e) =>
                run("set_auto_send", { enabled: e.target.value === "easy" })
              }
            >
              <option value="ask">Ask me first</option>
              <option value="easy">Send the easy ones</option>
            </select>
          </div>
          <div className="settings-row">
            <div className="grow">
              <h3>Mail Tomo archives</h3>
              <p>What happens in your inbox to mail that needs no reply</p>
            </div>
            <select
              aria-label="Mail Tomo archives"
              value={s.afterArchive || "read"}
              disabled={busy}
              onChange={async (e) => {
                const afterArchive = e.target.value;
                const saved = await run("save_settings", {
                  settings: { ...s, afterArchive },
                });
                // A Gmail sign-in that is still read-only needs one more consent before Tomo can change mail.
                if (saved && afterArchive !== "none")
                  void run("allow_mail_changes");
              }}
            >
              <option value="none">Leave as is</option>
              <option value="read">Mark as read</option>
              <option value="archive">Mark as read and archive</option>
            </select>
          </div>
          <div className="settings-row">
            <div className="grow">
              <h3>Pause processing</h3>
              <p>
                Pause synchronization, classification, and automatic replies.
              </p>
            </div>
            <Switch
              checked={s.paused}
              onChange={(paused) => run("set_processing_paused", { paused })}
              label="Pause processing"
            />
          </div>
          <div className="settings-row">
            <div className="grow">
              <h3>Desktop notifications</h3>
              <p>
                Approval requests, urgent messages, and connection problems.
              </p>
            </div>
            <Switch
              checked={s.notifications}
              onChange={(notifications) =>
                run("save_settings", { settings: { ...s, notifications } })
              }
              label="Desktop notifications"
              disabled={busy}
            />
          </div>
        </div>
        <ClientSettings />
      </section>
      <section>
        <h2 className="section-title">Privacy</h2>
        <div className="privacy-grid">
          <div className="settings-group">
            <h3>
              <Monitor size={15} />
              Stays on this Mac
            </h3>
            <ul>
              <li>Message cache, encrypted</li>
              <li>Plans, decisions and history</li>
              <li>Your preferences and style.md</li>
              <li>Sign-in tokens, encrypted with a key in Keychain</li>
            </ul>
          </div>
          <div className="settings-group">
            <h3>
              <Cloud size={15} />
              Sent to {s.aiProvider === "siwc" ? "ChatGPT" : "OpenAI API"}
            </h3>
            <ul>
              <li>The new message and only the thread it needs</li>
              <li>Only the event, file or contact details a plan uses</li>
              <li>Your style.md, with each draft</li>
              <li>Kept per OpenAI’s terms for your plan</li>
            </ul>
            <button className="text-button" onClick={() => setPrivacy(true)}>
              What happens when I disconnect?
            </button>
          </div>
        </div>
      </section>
      <section>
        <h2 className="section-title">Your Data</h2>
        <div className="data-actions">
          <button className="button" onClick={() => setConfirm("data")}>
            Show Stored Data
          </button>
          <button className="button" onClick={exportRules}>
            Export Settings…
          </button>
          <button className="button" onClick={() => setConfirm("history")}>
            Clear History…
          </button>
          <button className="button danger" onClick={() => setConfirm("reset")}>
            Reset Tomo…
          </button>
          {!desktop && (
            <button className="button" onClick={() => run("restore_preview")}>
              Restore preview
            </button>
          )}
        </div>
      </section>
      {!!s.blockedSenders.length && (
        <section>
          <h2 className="section-title">Tomo always asks first for</h2>
          <div className="settings-group">
            {s.blockedSenders.map((address) => (
              <div className="settings-row" key={address}>
                <span className="grow">{address}</span>
                <button
                  className="button"
                  disabled={busy}
                  onClick={() =>
                    run("save_settings", {
                      settings: {
                        ...s,
                        blockedSenders: s.blockedSenders.filter(
                          (v) => v !== address,
                        ),
                      },
                    })
                  }
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        </section>
      )}
      <Modal
        open={privacy}
        onClose={() => setPrivacy(false)}
        title="Your data, your control"
      >
        <p>
          Disconnecting an inbox removes its saved sign-in and deletes its local
          messages, drafts, and activity. Provider-side messages are unchanged.
          Revoke permissions in your provider’s account settings to fully revoke
          authorization.
        </p>
        <p>
          Disconnecting AI removes its credentials and turns off background AI
          consent and auto-send. Data already submitted to an AI provider
          follows that provider’s retention terms.
        </p>
      </Modal>
      <Modal
        open={!!confirm}
        onClose={() => setConfirm(null)}
        title={
          confirm === "data"
            ? "Stored on this Mac"
            : confirm === "reset"
              ? "Reset Tomo?"
              : "Clear processing history?"
        }
      >
        {confirm === "data" ? (
          <>
            <StoredData />
            <p>
              {snapshot.accounts.length} inboxes · {snapshot.approvals.length}{" "}
              cached approval records · {snapshot.activity.length} activity
              records.
            </p>
            <p>
              Messages, sign-ins and API keys are encrypted with AES-256-GCM on
              this Mac. The one key that unlocks them is kept in your Keychain.
            </p>
            {!desktop && (
              <p>
                Browser preview data is sample content stored in this tab’s
                session storage.
              </p>
            )}
          </>
        ) : (
          <>
            <p>
              {confirm === "reset"
                ? "Disconnect all accounts and remove local messages, drafts, preferences, and history. You will return to setup."
                : "Delete local activity records. Pending drafts and send protection records remain."}
            </p>
            <p>Send protection records remain to prevent duplicate replies.</p>
            <div className="modal-actions">
              <button className="button" onClick={() => setConfirm(null)}>
                Cancel
              </button>
              <button
                className="button destructive"
                disabled={busy}
                onClick={async () => {
                  if (
                    await run(
                      confirm === "reset" ? "reset_app" : "clear_history",
                    )
                  )
                    setConfirm(null);
                }}
              >
                {confirm === "reset" ? "Reset Tomo" : "Clear history"}
              </button>
            </div>
          </>
        )}
      </Modal>
    </main>
  );
}
