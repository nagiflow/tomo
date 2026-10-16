import { useState } from "react";
import {
  CalendarDays,
  Sheet,
  Presentation,
  Terminal,
  Files,
  FileText,
  Users,
  Mail,
  ArrowUp,
  Check,
  LoaderCircle,
  AlertCircle,
  MessageCircle,
  Hash,
  Trash2,
} from "lucide-react";
import type { Approval, PlanAction } from "../types";
import { toolLabels, providerLabels } from "../types";
import { useApp } from "../stores/app";
export function actionsFor(item: Approval): PlanAction[] {
  return item.plan?.length
    ? item.plan
    : [
        {
          id: `mail-${item.id}`,
          kind: "mail",
          operation: "reply",
          title: `Reply to ${item.message.sender.name.split(" ")[0]}`,
          summary: "Sends from your own account",
          detail: item.draft,
          params: { body: item.draft },
          status: "pending",
        },
      ];
}
export function actionApp(action: PlanAction, item: Approval) {
  return action.kind === "mail"
    ? providerLabels[item.message.provider]
    : // plans saved before a tool was retired (Google Tasks) still have to render
      toolLabels[action.kind] || action.kind;
}
export function actionRoute(item: Approval) {
  return actionsFor(item)
    .map((a) =>
      a.kind === "mail"
        ? `${providerLabels[item.message.provider]}${actionsFor(item).length === 1 ? " reply" : ""}`
        : (toolLabels[a.kind] || a.kind).replace("Google ", ""),
    )
    .join(" → ");
}
const attached = (action: PlanAction) =>
  Array.isArray(action.params.attachments)
    ? (action.params.attachments as string[])
    : [];
const icons = {
  mail: Mail,
  calendar: CalendarDays,
  drive: Files,
  docs: FileText,
  sheets: Sheet,
  slides: Presentation,
  codex: Terminal,
  contacts: Users,
  slack: Hash,
};
/**
 * What a Codex step produced, each openable in its default app. Its deliverables (the outbox)
 * come first; helper files it touched stay folded away.
 */
function CodexFiles({ item, files }: { item: Approval; files: string[] }) {
  const { run, busy } = useApp();
  const outbox = files.filter((f) => f.includes("/outbox/"));
  const others = files.filter((f) => !outbox.includes(f));
  const row = (file: string) => (
    <li key={file}>
      <FileText size={13} />
      <span className="grow" title={file}>
        {file.split("/").pop()}
      </span>
      <button
        className="text-button"
        disabled={busy}
        onClick={() => run("open_codex_file", { id: item.id, file })}
      >
        Open
      </button>
    </li>
  );
  return (
    <div className="codex-files">
      <span className="eyebrow">
        {outbox.length ? "Files Codex produced" : "Files Codex touched"}
      </span>
      <ul>{(outbox.length ? outbox : others).map(row)}</ul>
      {outbox.length > 0 && others.length > 0 && (
        <details>
          <summary>Other files Codex touched ({others.length})</summary>
          <ul>{others.map(row)}</ul>
        </details>
      )}
    </div>
  );
}
export function PlanSteps({
  item,
  actions,
  onChange,
  compact = false,
}: {
  item: Approval;
  actions: PlanAction[];
  onChange: (actions: PlanAction[]) => void;
  compact?: boolean;
}) {
  const [open, setOpen] = useState<number | null>(
    compact ? actions.length - 1 : null,
  );
  // Only files a finished Codex step reported can be attached; the backend enforces the same rule.
  const produced = actions.flatMap((a) =>
    a.kind === "codex" && Array.isArray(a.result?.files)
      ? (a.result.files as string[])
      : [],
  );
  return (
    <ol className={`plan-steps ${compact ? "compact-steps" : ""}`}>
      {actions.map((action, index) => {
        const Icon = icons[action.kind] || FileText;
        const expanded = open === null || open === index;
        // What Tomo proposed to attach is shown up front; everything else Codex touched (helper
        // scripts and the like) stays folded away but can still be ticked.
        const suggested = produced.filter((f) =>
          attached(item.plan?.at(-1) ?? action).includes(f),
        );
        const others = produced.filter((f) => !suggested.includes(f));
        const fileRow = (file: string) => (
          <label key={file}>
            <input
              type="checkbox"
              checked={attached(action).includes(file)}
              disabled={
                item.status !== "pending" || action.status !== "pending"
              }
              onChange={(e) =>
                onChange(
                  actions.map((a, i) =>
                    i === index
                      ? {
                          ...a,
                          params: {
                            ...a.params,
                            attachments: e.target.checked
                              ? [...attached(a), file]
                              : attached(a).filter((f) => f !== file),
                          },
                        }
                      : a,
                  ),
                )
              }
            />
            {/* scratch files come as full paths; the name is what matters here */}
            {file.startsWith("/") ? file.split("/").pop() : file}
          </label>
        );
        return (
          <li key={action.id}>
            <div className="step-rail">
              <span className="step-icon">
                {action.status === "completed" ? (
                  <Check size={16} />
                ) : action.status === "running" ? (
                  <LoaderCircle size={16} className="spinning" />
                ) : (
                  <Icon size={16} />
                )}
              </span>
              {index < actions.length - 1 && <i />}
            </div>
            <div className="step-body">
              <button
                className="step-heading"
                aria-expanded={expanded}
                onClick={() => setOpen(expanded ? -1 : index)}
              >
                <div className="grow">
                  <small>
                    {index + 1} · {actionApp(action, item)}
                  </small>
                  <strong>{action.title}</strong>
                  {(compact || !expanded) && <p>{action.summary}</p>}
                </div>
                <span className={`step-chevron ${expanded ? "open" : ""}`}>
                  ›
                </span>
              </button>
              {expanded && (
                <div className="step-content">
                  {action.kind === "mail" ? (
                    <>
                      <label className="sr-only" htmlFor={`reply-${action.id}`}>
                        {action.title}
                      </label>
                      <textarea
                        id={`reply-${action.id}`}
                        rows={compact ? 4 : 5}
                        value={String(action.params.body ?? action.detail)}
                        disabled={
                          item.status !== "pending" ||
                          action.status !== "pending"
                        }
                        onChange={(e) =>
                          onChange(
                            actions.map((a, i) =>
                              i === index
                                ? {
                                    ...a,
                                    detail: e.target.value,
                                    params: {
                                      ...a.params,
                                      body: e.target.value,
                                    },
                                  }
                                : a,
                            ),
                          )
                        }
                      />
                      {produced.length > 0 && (
                        <fieldset className="attachments">
                          <legend>Attach files Codex produced</legend>
                          {suggested.map(fileRow)}
                          {others.length > 0 && (
                            <details>
                              <summary>
                                {suggested.length ? "Other" : "Show"} files
                                Codex touched ({others.length})
                              </summary>
                              {others.map(fileRow)}
                            </details>
                          )}
                        </fieldset>
                      )}
                      {index > 0 && (
                        <small className="after-step">
                          Sends only after the earlier actions succeed
                        </small>
                      )}
                    </>
                  ) : (
                    <div className="tool-detail">
                      {action.kind === "codex" ? (
                        // exactly what will be handed to Codex, not the model's summary of it
                        <p className="codex-task">
                          {String(action.params.task ?? action.detail)}
                        </p>
                      ) : compact ? (
                        <p>{action.detail}</p>
                      ) : action.kind === "slack" ? (
                        // posted as Markdown, so its tables and line breaks are shown as written
                        <pre className="step-report">
                          {String(action.params.text ?? action.detail)}
                        </pre>
                      ) : action.kind === "calendar" && action.params.start ? (
                        <div className="event-changes">
                          {Boolean(action.params.originalStart) && (
                            <>
                              <span>From</span>
                              <del>{String(action.params.originalStart)}</del>
                            </>
                          )}
                          <span>To</span>
                          <strong>
                            {formatEventTime(
                              action.params.start,
                              action.params.end,
                            )}
                          </strong>
                          {Boolean(action.params.guests) && (
                            <>
                              <span>Guests</span>
                              <span>
                                {Array.isArray(action.params.guests)
                                  ? action.params.guests.join(", ")
                                  : String(action.params.guests)}
                              </span>
                            </>
                          )}
                        </div>
                      ) : (
                        <p>{action.detail}</p>
                      )}
                      {action.kind === "slack" &&
                        attached(action).length > 0 && (
                          <p className="muted">
                            Attaches{" "}
                            {attached(action)
                              .map((f) => f.split("/").pop())
                              .join(", ")}
                          </p>
                        )}
                      {!compact && action.summary && (
                        <div
                          className={`step-summary ${action.status === "completed" ? "success" : ""}`}
                        >
                          {action.status === "completed" && <Check size={13} />}
                          <span>
                            {action.status === "completed"
                              ? "Completed"
                              : action.summary}
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                  {typeof action.result?.summary === "string" && (
                    <pre className="step-report">{action.result.summary}</pre>
                  )}
                  {action.kind === "codex" &&
                    Array.isArray(action.result?.files) &&
                    action.result.files.length > 0 && (
                      <CodexFiles
                        item={item}
                        files={action.result.files as string[]}
                      />
                    )}
                  {action.error && (
                    <div className="inline-error">
                      <AlertCircle size={14} />
                      {action.error}
                    </div>
                  )}
                  {/* drops this step only; the rest of the plan stays as prepared */}
                  {!compact &&
                    actions.length > 1 &&
                    item.status === "pending" &&
                    action.status === "pending" && (
                      <button
                        className="text-button"
                        onClick={() => {
                          setOpen(null);
                          onChange(actions.filter((_, i) => i !== index));
                        }}
                      >
                        <Trash2 size={13} /> Remove this step
                      </button>
                    )}
                </div>
              )}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
function formatEventTime(start: unknown, end?: unknown) {
  const raw = (value: unknown) =>
    typeof value === "object" && value
      ? String((value as Record<string, unknown>).dateTime || "")
      : String(value ?? "");
  const from = new Date(raw(start));
  const to = new Date(raw(end));
  if (Number.isNaN(from.getTime())) return raw(start);
  const format = new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
  // formatRange collapses a same-day range to "Mon, Oct 12, 5:00 – 6:00 PM"
  return Number.isNaN(to.getTime())
    ? format.format(from)
    : format.formatRange(from, to);
}
export function InstructionInput({
  item,
  compact = false,
}: {
  item: Approval;
  compact?: boolean;
}) {
  const [note, setNote] = useState("");
  const { run, busy, snapshot } = useApp();
  // what Tomo or Codex is doing with this message right now, so a long wait does not look stuck
  const working = snapshot?.working?.[item.id];
  // a plan stopped by a failed tool step can be picked up again from here; an unconfirmed send cannot
  const stopped =
    item.status === "uncertain" &&
    (item.plan || []).some(
      (a) => a.kind !== "mail" && a.status === "uncertain",
    );
  const open = item.status === "pending" || stopped;
  const placeholder = compact
    ? "Tell Tomo…"
    : item.message.sender.name === "Sarah Chen"
      ? "Tell Tomo… e.g. “make it 5:30 and add a Meet link”"
      : stopped
        ? "Tell Tomo how to continue, e.g. “try again”"
        : "Tell Tomo what to change or add…";
  return (
    <>
      {working && (
        <p className="working-note" role="status">
          <LoaderCircle size={14} className="spinning" />
          <span>{working}…</span>
        </p>
      )}
      <form
        className="instruction-input"
        onSubmit={async (e) => {
          e.preventDefault();
          if (await run("revise_plan", { id: item.id, instruction: note }))
            setNote("");
        }}
      >
        <input
          aria-label="Instruction for Tomo"
          placeholder={placeholder}
          value={note}
          maxLength={2000}
          onChange={(e) => setNote(e.target.value)}
          disabled={busy || !open}
        />
        <button
          aria-label="Update plan"
          disabled={busy || !note.trim() || !open}
        >
          <ArrowUp size={16} />
        </button>
      </form>
    </>
  );
}
export function InstructionNote({ item }: { item: Approval }) {
  return item.instruction ? (
    <div className="instruction-note">
      <MessageCircle size={14} />
      <span>You: “{item.instruction}” · plan updated</span>
    </div>
  ) : null;
}
