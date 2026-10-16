import { useEffect, useState } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { CheckCircle2, LoaderCircle } from "lucide-react";
import { useApp } from "../stores/app";
import { desktop } from "../services/api";
import { providerLabels } from "../types";
import { Mascot, TrafficLights } from "./ui";
import {
  GoogleSetupView,
  ToolsSettings,
  WritingStyle,
} from "./WorkspaceSettings";
import { AISettings } from "./SettingsView";
export function Onboarding() {
  const [step, setStep] = useState(0);
  const [api, setApi] = useState(false);
  const [footer, setFooter] = useState<HTMLElement | null>(null);
  // Tool choices the current Google sign-in already covers; changing them needs one more consent screen.
  const [granted, setGranted] = useState("");
  const [skipped, setSkipped] = useState(false);
  const { snapshot, run, busy } = useApp();
  useEffect(() => {
    if (!desktop) return;
    const size = (w: number, h: number) => {
      const win = getCurrentWindow();
      void win
        .setSize(new LogicalSize(w, h))
        .then(() => win.center())
        .catch(() => {});
    };
    size(760, 620);
    return () => size(1280, 900);
  }, []);
  if (!snapshot) return null;
  const s = snapshot.settings;
  const google = snapshot.accounts.find((a) => a.provider === "gmail");
  const settingUpGoogle = step === 1 && !google;
  const choosingModel = api || snapshot.ai.connected;
  const samples = snapshot.style?.sampleCount || 0;
  const canLearn =
    s.consent && snapshot.ai.connected && snapshot.accounts.length > 0;
  const tools = JSON.stringify(s.tools || {});
  const toTools = () => {
    // read the store, not this render: finishing Google setup may just have changed the tools
    setGranted(
      JSON.stringify(useApp.getState().snapshot?.settings.tools || {}),
    );
    setStep(2);
  };
  // The one Google sign-in happens here, after the services are chosen, so it asks for all of them.
  const signIn = google
    ? // switching a tool off, or leaving them alone, never needs Google again
      Object.entries(s.tools || {}).some(
        ([kind, mode]) =>
          mode !== "off" &&
          (JSON.parse(granted || "{}")[kind] || "off") === "off",
      )
    : !!s.googleClientId && !skipped;
  async function next() {
    if (step === 1) return toTools();
    if (step === 2) {
      if (signIn && !(await run("connect_account", { provider: "gmail" })))
        return;
      setGranted(tools);
      setStep(3);
      if (!samples && canLearn)
        void run("learn_writing_style", { more: false });
      return;
    }
    if (step < 3) setStep(step + 1);
    else
      await run("save_settings", {
        settings: { ...s, onboarded: true, autoSend: false },
      });
  }
  return (
    <div className={`setup-shell ${desktop ? "native" : ""}`}>
      <div
        className={`setup-window onboarding-v2 step-${step} ${step === 0 && choosingModel ? "config" : ""}`}
      >
        <TrafficLights />
        <main className="setup-content">
          {step === 0 && (
            <>
              {choosingModel ? (
                <div className="setup-head">
                  <div className="app-icon">
                    <Mascot size={36} mood="happy" />
                  </div>
                  <div>
                    <h1>Choose how Tomo thinks</h1>
                    <p>
                      Pick the model and how hard it should think. You can
                      change this anytime in Settings.
                    </p>
                  </div>
                </div>
              ) : (
                <>
                  <div className="app-icon large">
                    <Mascot size={64} mood="happy" />
                  </div>
                  <header>
                    <h1>Hi, I’m Tomo</h1>
                    <p>
                      I sort your messages, plan the replies and follow-ups, and
                      only interrupt you when something needs your OK.
                    </p>
                  </header>
                </>
              )}
              {/* once signed in, this step becomes the model choice */}
              {choosingModel ? (
                <AISettings setup />
              ) : (
                <div className="meet-tomo">
                  <button
                    className="button dark"
                    disabled={busy}
                    onClick={() => run("connect_ai")}
                  >
                    Sign in with ChatGPT
                  </button>
                  <p>
                    Tomo thinks with your ChatGPT plan. Messages stay on this
                    Mac, except the parts each request needs.
                  </p>
                  <button
                    className="text-button"
                    onClick={async () => {
                      await run("save_settings", {
                        settings: { ...s, aiProvider: "openai", model: "" },
                      });
                      setApi(true);
                    }}
                  >
                    Use an OpenAI API key instead
                  </button>
                </div>
              )}
            </>
          )}
          {settingUpGoogle && (
            <GoogleSetupView
              footer={footer}
              onBack={() => setStep(0)}
              onSkip={() => {
                setSkipped(true);
                toTools();
              }}
              onComplete={() => {
                setSkipped(false);
                toTools();
              }}
            />
          )}
          {step === 1 && google && (
            <>
              <header>
                <h1>Connect your Google account</h1>
                <p>
                  Tomo signs in through a Google Cloud project that you own, so
                  nothing passes through anyone else’s servers.
                </p>
              </header>
              <div className="google-connected">
                <CheckCircle2 size={16} />
                <span className="grow">
                  <strong>Google connected</strong> · {google.email}
                </span>
                {s.googleProjectId && <code>{s.googleProjectId}</code>}
              </div>
            </>
          )}
          {step === 2 && (
            <>
              <header>
                <h1>Choose what Tomo can use</h1>
                <p>You can change this anytime in Settings.</p>
              </header>
              <ToolsSettings />
              <div className="settings-group outlook-setup">
                <div className="settings-row">
                  <span className="provider-icon">O</span>
                  <div className="grow">
                    <h3>Also use Outlook?</h3>
                    <p>Signs in with Microsoft separately</p>
                  </div>
                  <button
                    className="button"
                    disabled={busy}
                    onClick={() =>
                      run("connect_account", { provider: "outlook" })
                    }
                  >
                    {snapshot.accounts.some((a) => a.provider === "outlook")
                      ? "Connected"
                      : "Connect…"}
                  </button>
                </div>
              </div>
            </>
          )}
          {step === 3 && (
            <>
              <div className="setup-head">
                <div className="app-icon">
                  <Mascot size={36} mood="happy" />
                </div>
                <div>
                  <h1>Here’s how you write</h1>
                  <p>
                    {samples
                      ? "Tomo read your recent sent messages and wrote this down. It reads this file before every draft."
                      : "Tomo reads this file before every draft so replies sound like you. Edit it to match your voice."}
                  </p>
                </div>
              </div>
              <div className="style-learned">
                {samples ? (
                  <>
                    <span>
                      <CheckCircle2 size={14} />
                      Read {samples} sent emails from{" "}
                      {[...new Set(snapshot.accounts.map((a) => a.provider))]
                        .map((p) => providerLabels[p])
                        .join(" and ") || "your inboxes"}
                    </span>
                    <span>
                      <CheckCircle2 size={14} />
                      Wrote style.md
                    </span>
                  </>
                ) : busy ? (
                  <span>
                    <LoaderCircle size={14} className="spinning" />
                    Reading your recent sent messages…
                  </span>
                ) : (
                  <span>
                    Starter style. Connect an inbox and AI, then choose Read
                    More Messages to learn yours.
                  </span>
                )}
              </div>
              <WritingStyle onboarding />
            </>
          )}
        </main>
        <footer className="setup-footer">
          <div className="setup-step-label">
            <div className="step-dots" aria-label={`Step ${step + 1} of 4`}>
              {[0, 1, 2, 3].map((i) => (
                <span key={i} className={step === i ? "current" : ""} />
              ))}
            </div>
            <span>
              Step {step + 1} of 4 ·{" "}
              {["AI", "Google", "Inboxes and tools", "Your style"][step]}
            </span>
          </div>
          {settingUpGoogle ? (
            <div ref={setFooter} />
          ) : (
            <div>
              {step > 0 && (
                <button
                  className="button"
                  disabled={busy}
                  onClick={() => setStep(step - 1)}
                >
                  Back
                </button>
              )}
              {step === 3 && (
                <button
                  className="button"
                  disabled={busy || !canLearn}
                  onClick={() =>
                    run("learn_writing_style", { more: samples > 0 })
                  }
                >
                  Read More Messages
                </button>
              )}
              <button className="button primary" disabled={busy} onClick={next}>
                {step === 3
                  ? "Start Tomo"
                  : step === 2 && signIn && !google
                    ? "Sign In with Google"
                    : "Continue"}
              </button>
            </div>
          )}
        </footer>
      </div>
    </div>
  );
}
