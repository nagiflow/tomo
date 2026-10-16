import { useEffect, useState } from "react";
import {
  Inbox,
  Clock3,
  Settings,
  Pause,
  Play,
  RefreshCw,
  X,
} from "lucide-react";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import { desktop } from "./services/api";
import { useApp } from "./stores/app";
import { Queue, Popover } from "./components/ApprovalView";
import { ActivityView } from "./components/ActivityView";
import { SettingsView } from "./components/SettingsView";
import { Onboarding } from "./components/Onboarding";
import { Mascot, TrafficLights } from "./components/ui";
type Page = "queue" | "activity" | "settings";
export default function App() {
  const { snapshot, load, error, clearError, run, busy } = useApp();
  const [page, setPage] = useState<Page>("queue");
  const [popover, setPopover] = useState(location.hash === "#popover");
  useEffect(() => {
    void load();
    const interval = setInterval(() => void load(), 10000);
    let cleanup: (() => void) | undefined;
    let cleanupReview: (() => void) | undefined;
    if (desktop)
      void listen<string | null>("tomo:review", (event) => {
        setPage("queue");
        useApp.setState({ selectedId: event.payload });
      }).then((fn) => (cleanupReview = fn));
    if (desktop)
      void listen("tomo:updated", () => void load()).then(
        (fn) => (cleanup = fn),
      );
    const hash = () => setPopover(location.hash === "#popover");
    window.addEventListener("hashchange", hash);
    return () => {
      clearInterval(interval);
      cleanup?.();
      cleanupReview?.();
      window.removeEventListener("hashchange", hash);
    };
  }, [load]);
  useEffect(() => {
    if (desktop && snapshot?.settings.notifications)
      void isPermissionGranted()
        .then(async (granted) => {
          if (!granted) await requestPermission();
        })
        .catch(() => {});
  }, [snapshot?.settings.notifications]);
  useEffect(() => {
    function keys(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key === ",") {
        e.preventDefault();
        setPage("settings");
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "1") {
        e.preventDefault();
        setPage("queue");
      }
    }
    window.addEventListener("keydown", keys);
    return () => window.removeEventListener("keydown", keys);
  }, []);
  useEffect(() => {
    if (snapshot && !snapshot.settings.onboarded) setPage("queue");
  }, [snapshot?.settings.onboarded]);
  const errorView = error && (
    <div className="error-toast" role="alert">
      <span>{error}</span>
      <button
        className="icon-button"
        onClick={clearError}
        aria-label="Dismiss error"
      >
        <X size={15} />
      </button>
    </div>
  );
  if (!snapshot)
    return (
      <>
        <div className="loading">
          <Mascot size={52} />
          <p>
            {error ? "Tomo could not open local storage." : "Opening Tomo…"}
          </p>
          {error && (
            <button className="button" onClick={load}>
              Try again
            </button>
          )}
        </div>
        {errorView}
      </>
    );
  if (popover)
    return (
      <>
        <Popover />
        {errorView}
      </>
    );
  if (!snapshot.settings.onboarded)
    return (
      <>
        <Onboarding />
        {errorView}
      </>
    );
  const count = snapshot.approvals.filter((a) =>
    ["pending", "executing", "uncertain"].includes(a.status),
  ).length;
  const s = snapshot.settings;
  return (
    <div className="app-shell">
      <nav className="sidebar" aria-label="Sidebar">
        <TrafficLights />
        {(
          [
            { key: "queue", label: "Inbox", icon: Inbox },
            { key: "activity", label: "Activity", icon: Clock3 },
            { key: "settings", label: "Settings", icon: Settings },
          ] as const
        ).map((item) => (
          <button
            key={item.key}
            aria-label={item.key === "queue" ? `Inbox ${count}` : item.label}
            className={`nav-item ${page === item.key ? "selected" : ""}`}
            aria-current={page === item.key ? "page" : undefined}
            onClick={() => setPage(item.key)}
          >
            <item.icon size={18} />
            <span>{item.label}</span>
            {item.key === "queue" && <small>{count}</small>}
          </button>
        ))}
        <div className="sidebar-bottom">
          {!desktop && (
            <div className="preview-notice">
              <strong>Interactive preview</strong>
              <p>
                Sample conversations.
                <br />
                No real accounts or sends.
              </p>
              <button
                className="text-button"
                onClick={() => (location.hash = "popover")}
              >
                Try menu bar view ↗
              </button>
            </div>
          )}
          <button
            className="sidebar-status"
            onClick={() => setPage("settings")}
          >
            <Mascot
              size={32}
              mood={s.paused ? "sleeping" : count ? "waiting" : "working"}
            />
            <div>
              <strong>
                {s.paused
                  ? "Tomo is napping"
                  : count
                    ? `${count} waiting for you`
                    : "All caught up"}
              </strong>
              <small
                title={
                  s.paused
                    ? undefined
                    : `Checks for new mail every ${Math.round(s.pollSeconds / 60)} minutes`
                }
              >
                {s.paused ? (
                  "Paused"
                ) : (
                  <>
                    Awake ·{" "}
                    <RefreshCw
                      size={10}
                      aria-label="checks every"
                      style={{ verticalAlign: "-1px" }}
                    />{" "}
                    {Math.round(s.pollSeconds / 60)} min
                  </>
                )}
              </small>
            </div>
          </button>
        </div>
      </nav>
      <div className="workspace">
        <div className="workspace-toolbar" data-tauri-drag-region>
          <span className="sync-status">
            <i
              className={
                s.paused ? "paused" : snapshot.lastError ? "error" : ""
              }
            />
            {s.paused
              ? "Processing paused"
              : snapshot.syncing
                ? "Checking your inboxes…"
                : snapshot.lastError
                  ? "Connection needs attention"
                  : desktop
                    ? "Watching your inboxes"
                    : "Tomo · Community preview"}
            {s.yolo && !s.paused && (
              <strong
                className="yolo-flag"
                title="Plans run without your approval"
              >
                YOLO
              </strong>
            )}
          </span>
          <div>
            <button
              className="text-button"
              disabled={busy || snapshot.syncing || s.paused}
              onClick={() => run("sync_now")}
              title="Check for new messages"
              aria-label="Check now"
            >
              <RefreshCw
                size={14}
                className={snapshot.syncing ? "spinning" : ""}
              />
            </button>
            <button
              className="button small"
              onClick={() =>
                run("set_processing_paused", { paused: !s.paused })
              }
            >
              {s.paused ? <Play size={12} /> : <Pause size={12} />}{" "}
              {s.paused ? "Resume" : "Pause"}
            </button>
          </div>
        </div>
        {snapshot.lastError && (
          <div className="connection-banner" role="status">
            {snapshot.lastError}
          </div>
        )}
        {page === "queue" ? (
          <Queue />
        ) : page === "activity" ? (
          <ActivityView />
        ) : (
          <SettingsView />
        )}
      </div>
      {errorView}
    </div>
  );
}
