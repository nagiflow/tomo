import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import {
  ArrowUpRight,
  Moon,
  AlertCircle,
  PanelLeft,
  LoaderCircle,
  Clock3,
} from "lucide-react";
import { useApp } from "../stores/app";
import { desktop } from "../services/api";
import { categoryLabels, providerLabels } from "../types";
import type { Approval, PlanAction, UnifiedMessage } from "../types";
import { Empty, Mascot, time } from "./ui";
import {
  actionsFor,
  actionRoute,
  PlanSteps,
  InstructionInput,
  InstructionNote,
} from "./PlanView";
function usePlan(item: Approval) {
  const [actions, setActions] = useState<PlanAction[]>(() => actionsFor(item));
  useEffect(
    () => setActions(actionsFor(item)),
    [item.id, item.revision, item.draft, item.plan],
  );
  return [actions, setActions] as const;
}
// Names what this approval will actually run. Finished steps do not count, and a Codex task ends the
// run: its outcome can change the reply and whatever follows, so those come back for review.
function approveLabel(actions: PlanAction[]) {
  const todo = actions.filter((a) => a.status !== "completed");
  const codex = todo.findIndex((a) => a.kind === "codex");
  if (codex < 0 || codex === todo.length - 1)
    return todo.length > 1
      ? `Approve ${todo.length} actions`
      : todo[0]?.kind === "slack"
        ? "Post to Slack"
        : "Send reply";
  return codex === 0
    ? "Run Codex task, then review"
    : `Approve ${codex + 1} actions, then review`;
}
// Approves the plan; when a tool step stopped it, retries from that step instead.
function ApproveButton({
  item,
  actions,
}: {
  item: Approval;
  actions: PlanAction[];
}) {
  const { run, busy } = useApp();
  const retry =
    item.status === "uncertain" &&
    actions.some((a) => a.kind !== "mail" && a.status === "uncertain");
  return (
    <button
      className="button primary"
      disabled={
        busy ||
        (!retry &&
          (item.status !== "pending" ||
            actions.some(
              (a) => a.status === "running" || a.status === "uncertain",
            ) ||
            (actions.at(-1)?.kind === "mail" &&
              !String(actions.at(-1)?.params.body || "").trim())))
      }
      onClick={() =>
        retry
          ? run("retry_plan", { id: item.id })
          : run("approve_plan", {
              id: item.id,
              actions,
              revision: item.revision || 0,
            })
      }
    >
      {busy ? "Working…" : retry ? "Retry" : approveLabel(actions)}
    </button>
  );
}
// The model's short topic label ("Date change"), falling back to the category.
const tagFor = (item: Approval) =>
  item.classification.tag || categoryLabels[item.classification.category];
// With `tip`, the gauge stands alone and Tomo's reason appears on hover or keyboard focus.
function RiskBars({ item, tip = false }: { item: Approval; tip?: boolean }) {
  return (
    <span className={`risk ${tip ? "with-tip" : ""}`} tabIndex={tip ? 0 : -1}>
      <span
        className={`risk-bars ${item.classification.risk}`}
        aria-hidden="true"
      >
        <i />
        <i />
        <i />
      </span>
      <span>
        {item.classification.risk[0].toUpperCase() +
          item.classification.risk.slice(1)}{" "}
        risk
      </span>
      {tip && (
        <span className="risk-tip" role="tooltip">
          <span className="eyebrow">Why Tomo asked</span>
          {item.classification.reason}
        </span>
      )}
    </span>
  );
}
function block(item: Approval) {
  const state = useApp.getState();
  if (state.snapshot)
    void state.run("save_settings", {
      settings: {
        ...state.snapshot.settings,
        blockedSenders: [
          ...state.snapshot.settings.blockedSenders,
          item.message.sender.address || item.message.sender.id,
        ],
      },
    });
}
// What the provider's own page would add (scripts, remote images, tracking pixels) is all blocked.
const mailFrame = (html: string) =>
  `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:"><style>html,body{background:transparent}body{margin:0;font:13.5px/1.5 -apple-system,BlinkMacSystemFont,sans-serif;color:#1d1d1f;overflow-wrap:anywhere}blockquote{margin:8px 0;padding-left:10px;border-left:2px solid #d1d1d6;color:#6e6e73}a{color:#0062cc}img{max-width:100%;height:auto}table{max-width:100%}</style>${html}`;
/** The incoming message, shown the way it was written when it came as HTML. */
export function OriginalMessage({ message }: { message: UnifiedMessage }) {
  const html = message.metadata.html;
  const [asText, setAsText] = useState(false);
  return (
    <>
      <span className="eyebrow">
        They wrote
        {typeof html === "string" && (
          <button className="text-button" onClick={() => setAsText(!asText)}>
            {asText ? "Show formatted" : "Show as text"}
          </button>
        )}
      </span>
      {typeof html === "string" && !asText ? (
        // An empty sandbox: the mail's markup can lay itself out, and nothing else. No scripts, no
        // forms, no access to this app. Its links do nothing; "Open in Gmail" is the way out.
        <iframe
          className="mail-frame"
          title="Original message"
          sandbox=""
          srcDoc={mailFrame(html)}
        />
      ) : (
        <p>{message.content}</p>
      )}
    </>
  );
}
export function ApprovalDetail({
  item,
  onLater,
}: {
  item: Approval;
  onLater: () => void;
}) {
  const [actions, setActions] = usePlan(item);
  const { run, busy, snapshot } = useApp();
  const name = item.message.sender.name;
  const blocked = snapshot?.settings.blockedSenders.includes(
    item.message.sender.address || item.message.sender.id,
  );
  return (
    <article className="detail plan-detail">
      <header className="message-heading">
        <span className="avatar">
          {name
            .split(" ")
            .map((v) => v[0])
            .slice(0, 2)
            .join("")}
        </span>
        <div className="grow">
          <h2>{item.subject}</h2>
          <p>
            {name} · {item.message.sender.address} ·{" "}
            {providerLabels[item.message.provider]} ·{" "}
            {time(item.message.timestamp)}
          </p>
        </div>
        <RiskBars item={item} tip />
        <button
          className="button"
          onClick={() => run("open_original", { id: item.id })}
        >
          Open in {providerLabels[item.message.provider]}
          <ArrowUpRight size={13} />
        </button>
      </header>
      <div className="plan-message-context">
        <section>
          <OriginalMessage message={item.message} />
        </section>
      </div>
      <section className="plan-section">
        <div className="plan-section-heading">
          <h3 className="eyebrow">
            Tomo’s plan ·{" "}
            {actions.length === 1
              ? "1 action"
              : `${actions.length} actions, in order`}
          </h3>
          <span>If a step fails, Tomo stops and tells you.</span>
        </div>
        <InstructionNote item={item} />
        <PlanSteps item={item} actions={actions} onChange={setActions} />
      </section>
      <InstructionInput item={item} />
      {item.sendError && (
        <div className="inline-error" role="alert">
          <AlertCircle size={15} />
          {item.sendError}
        </div>
      )}
      <div className="actions">
        <ApproveButton item={item} actions={actions} />
        <button className="button" onClick={onLater}>
          Later
        </button>
        <button
          className="button"
          disabled={busy || item.status !== "pending"}
          onClick={() => run("dismiss_reply", { id: item.id })}
        >
          Dismiss
        </button>
        <button
          className="text-button danger never-automate"
          disabled={blocked}
          onClick={() => block(item)}
        >
          {blocked
            ? "Automation blocked"
            : `Never automate ${name.split(" ")[0]}`}
        </button>
      </div>
      {!desktop && (
        <p className="preview-action-note">
          Preview only · no real actions or messages are sent.
        </p>
      )}
    </article>
  );
}
export function Queue() {
  const { snapshot, selectedId } = useApp();
  // Oldest first: new mail joins at the bottom, where the messages Tomo is still working on sit.
  const queue = (snapshot?.approvals || [])
    .filter((a) => ["pending", "executing", "uncertain"].includes(a.status))
    .sort((a, b) => a.message.timestamp.localeCompare(b.message.timestamp));
  const incoming = [...(snapshot?.pending || [])].sort(
    (a, b) =>
      +(b.action === "processing") - +(a.action === "processing") ||
      a.timestamp.localeCompare(b.timestamp),
  );
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => setSelected(selectedId), [selectedId]);
  const item = queue.find((a) => a.id === selected) || queue[0];
  const later = () => {
    if (item)
      setSelected(queue[(queue.indexOf(item) + 1) % queue.length]?.id || null);
  };
  return (
    <div className="queue-layout">
      <section className="queue-list">
        <header>
          <h1>Inbox</h1>
          <p>
            {queue.length} {queue.length === 1 ? "plan" : "plans"} waiting for
            your OK
            {incoming.length > 0 && ` · ${incoming.length} in progress`}
          </p>
        </header>
        {queue.map((a) => (
          <button
            className={`queue-card ${a.id === item?.id ? "selected" : ""}`}
            aria-pressed={a.id === item?.id}
            key={a.id}
            onClick={() => setSelected(a.id)}
          >
            <span className="card-heading">
              <strong>{a.message.sender.name}</strong>
              <time>{time(a.message.timestamp)}</time>
            </span>
            <span>{a.subject}</span>
            <small>
              <span className="danger">
                {a.status === "uncertain" ? "Check outcome" : tagFor(a)}
              </span>{" "}
              · {actionRoute(a)}
            </small>
          </button>
        ))}
        {incoming.map((p) => (
          <div className="queue-card incoming" key={p.id}>
            <span className="card-heading">
              <strong>{p.sender}</strong>
              <time>{time(p.timestamp)}</time>
            </span>
            <span>{p.subject}</span>
            <small role="status">
              {p.action === "processing" ? (
                <LoaderCircle size={12} className="spinning" />
              ) : (
                <Clock3 size={12} />
              )}
              {p.action === "processing" ? `${p.reason}…` : p.reason}
            </small>
          </div>
        ))}
      </section>
      {item ? (
        <ApprovalDetail item={item} onLater={later} />
      ) : (
        <article className="detail">
          <Empty
            title={incoming.length ? "Tomo is on it" : "All caught up"}
            paused={snapshot?.settings.paused}
          >
            {incoming.length
              ? "New mail is being read. Plans that need your OK will appear here."
              : snapshot?.accounts.length
                ? "Tomo will let you know when a plan needs your OK."
                : "Connect an inbox in Settings to get started."}
          </Empty>
        </article>
      )}
    </div>
  );
}
export function Popover() {
  const { snapshot, run } = useApp();
  const [index, setIndex] = useState(0);
  const queue = (snapshot?.approvals || []).filter((a) =>
    ["pending", "executing", "uncertain"].includes(a.status),
  );
  const item = queue[index % Math.max(1, queue.length)];
  const paused = !!snapshot?.settings.paused;
  useEffect(() => {
    if (desktop)
      void getCurrentWindow().setSize(
        new LogicalSize(360, item && actionsFor(item).length > 1 ? 736 : 556),
      );
  }, [item?.id, item?.revision, item?.plan?.length]);
  return (
    <div className={`popover-scene ${desktop ? "native" : ""}`}>
      <div
        className={`popover plan-popover ${item && actionsFor(item).length > 1 ? "multi-plan" : "single-plan"}`}
      >
        <header className="popover-heading">
          <div className="popover-mascot">
            <Mascot
              size={32}
              mood={paused ? "sleeping" : queue.length ? "waiting" : "working"}
            />
          </div>
          <div className="grow">
            <h1>Tomo</h1>
            <p>
              {paused
                ? "Napping · processing paused"
                : queue.length
                  ? `${queue.length} ${queue.length === 1 ? "plan" : "plans"} waiting for your OK`
                  : "Nothing needs you"}
            </p>
          </div>
          <button
            className="nap-button"
            aria-label="Open Tomo window"
            onClick={() => run("show_main", { id: item?.id })}
          >
            <PanelLeft size={16} />
          </button>
          <button
            className={`nap-button ${paused ? "active" : ""}`}
            aria-label={paused ? "Resume processing" : "Pause processing"}
            onClick={() => run("set_processing_paused", { paused: !paused })}
          >
            <Moon size={16} />
          </button>
        </header>
        {item ? (
          <PopoverPlan
            key={item.id}
            item={item}
            count={queue.length}
            later={() => setIndex(index + 1)}
          />
        ) : (
          <div className="popover-empty">
            <Empty title="All caught up" paused={paused}>
              Tomo will tap you on the shoulder when something needs you.
            </Empty>
          </div>
        )}
      </div>
      {!desktop && (
        <p className="popover-preview-label">
          Interactive preview · no real actions
        </p>
      )}
    </div>
  );
}
function PopoverPlan({
  item,
  count,
  later,
}: {
  item: Approval;
  count: number;
  later: () => void;
}) {
  const { run, snapshot } = useApp();
  const [actions, setActions] = usePlan(item);
  const [details, setDetails] = useState(false);
  const blocked = snapshot?.settings.blockedSenders.includes(
    item.message.sender.address || item.message.sender.id,
  );
  return (
    <>
      <section
        className={`review-stack ${count > 1 ? "has-more" : ""}`}
        aria-label="Plan waiting for approval"
      >
        <div className={`review-flipper ${details ? "flipped" : ""}`}>
          <article
            className="review-face review-front"
            aria-hidden={details}
            inert={details}
          >
            <header>
              <span className="avatar">
                {item.message.sender.name
                  .split(" ")
                  .map((v) => v[0])
                  .slice(0, 2)
                  .join("")}
              </span>
              <div className="grow">
                <strong>{item.message.sender.name}</strong>
                <p>
                  {providerLabels[item.message.provider]} ·{" "}
                  {time(item.message.timestamp)}
                </p>
              </div>
              <span className="badge warning">{tagFor(item)}</span>
            </header>
            <h2>{item.subject}</h2>
            <div className="plan-caption">
              <span className="eyebrow">Tomo’s plan</span>
              <small>
                {actions.length === 1
                  ? "1 action"
                  : `${actions.length} actions in order`}
              </small>
            </div>
            <InstructionNote item={item} />
            <div className="popover-plan-scroll">
              <PlanSteps
                item={item}
                actions={actions}
                onChange={setActions}
                compact
              />
            </div>
          </article>
          <article
            className="review-face review-back"
            aria-hidden={!details}
            inert={!details}
          >
            <section>
              <span className="eyebrow">Why Tomo asked</span>
              <p>{item.classification.reason}</p>
            </section>
            <RiskBars item={item} />
            <section className="popover-original">
              <span className="eyebrow">They wrote</span>
              <p>{item.message.content}</p>
            </section>
            <div className="popover-links">
              <button
                className="text-button"
                onClick={() => run("show_main", { id: item.id })}
              >
                Edit plan…
              </button>
              <button
                className="text-button"
                onClick={() => run("open_original", { id: item.id })}
              >
                Open in {providerLabels[item.message.provider]}
              </button>
              <button
                className="text-button danger"
                disabled={blocked}
                onClick={() => block(item)}
              >
                {blocked
                  ? "Automation blocked"
                  : `Never automate ${item.message.sender.name.split(" ")[0]}`}
              </button>
            </div>
          </article>
        </div>
      </section>
      {item.sendError && (
        <p className="inline-error" role="alert">
          {item.sendError}
        </p>
      )}
      <InstructionInput item={item} compact />
      <div className="popover-actions">
        <button className="button" onClick={later}>
          Later
        </button>
        <button className="button" onClick={() => setDetails(!details)}>
          {details ? "Plan" : "Details"}
        </button>
        <ApproveButton item={item} actions={actions} />
      </div>
    </>
  );
}
