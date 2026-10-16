import { useEffect, useState } from "react";
import { useApp } from "../stores/app";
import { command } from "../services/api";
import { categoryLabels, providerLabels } from "../types";
import type { Activity, UnifiedMessage } from "../types";
import { Empty, Modal, time } from "./ui";
import { OriginalMessage } from "./ApprovalView";
import { PlanSteps } from "./PlanView";
// "Archived" is the design's word for mail Tomo filed without bothering you.
const quiet = ["ignored", "archived", "record_only"];
const labels: Record<string, string> = {
  auto_sent: "Sent by Tomo",
  approved: "You approved",
  held: "Held for you",
  ignored: "Archived",
  record_only: "Recorded",
  dismissed: "Dismissed",
  failed: "Send failed",
  uncertain: "Check outcome",
  corrected: "Corrected",
  plan_step: "Action completed",
  plan_failed: "Plan stopped",
  archived: "Archived",
  pending: "Pending",
  processing: "Processing",
};
/**
 * What an item was about and what Tomo did, as Inbox shows it: the message, then the plan's steps
 * with their outcomes. Mail filed without a plan is read from the local message store.
 */
function ActivityContext({ a }: { a: Activity }) {
  const { snapshot } = useApp();
  const item = snapshot?.approvals.find((x) => x.id === a.messageId);
  const [stored, setStored] = useState<UnifiedMessage | null>(null);
  useEffect(() => {
    if (!item)
      command<UnifiedMessage | null>("get_message", { id: a.messageId })
        .then(setStored)
        .catch(() => {});
  }, [item, a.messageId]);
  const message = item?.message ?? stored;
  return (
    <>
      {message && (
        <div className="activity-full activity-original">
          <OriginalMessage message={message} />
        </div>
      )}
      {item?.plan?.length ? (
        <div className="activity-full">
          <span className="eyebrow">What Tomo did</span>
          {/* read-only here: changes to a plan still waiting are made in Inbox */}
          <PlanSteps
            item={item}
            actions={item.plan}
            onChange={() => {}}
            compact
          />
        </div>
      ) : null}
    </>
  );
}
const waiting = (action: string) => ["pending", "processing"].includes(action);
export function ActivityView() {
  const { snapshot, run, busy } = useApp();
  const [filter, setFilter] = useState("all");
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [clear, setClear] = useState(false);
  const activity = [...(snapshot?.pending || []), ...(snapshot?.activity || [])]
    .filter(
      (a) =>
        (filter === "all" ||
          a.action === filter ||
          (filter === "archived" && quiet.includes(a.action))) &&
        `${a.sender} ${a.subject} ${a.reason} ${a.content || ""}`
          .toLowerCase()
          .includes(search.toLowerCase()),
    )
    // mail Tomo has not finished stays on top, whenever it arrived
    .sort(
      (a, b) =>
        +waiting(b.action) - +waiting(a.action) ||
        b.timestamp.localeCompare(a.timestamp),
    );
  return (
    <main className="page">
      <header className="page-header activity-header">
        <div className="grow">
          <h1>Activity</h1>
        </div>
        <div className="segmented" role="group" aria-label="Filter activity">
          {[
            ["all", "All"],
            ["auto_sent", "Sent by Tomo"],
            ["approved", "Approved"],
            ["held", "Held"],
            ["archived", "Archived"],
          ].map(([key, label]) => (
            <button
              key={key}
              aria-pressed={filter === key}
              className={filter === key ? "active" : ""}
              onClick={() => setFilter(key)}
            >
              {label}
            </button>
          ))}
        </div>
        <input
          className="search"
          type="search"
          aria-label="Search activity"
          placeholder="Search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </header>
      <section className={activity.length ? "activity-list" : ""}>
        {activity.length ? (
          activity.map((a, i) => (
            <div key={a.id}>
              {(i === 0 ||
                new Date(activity[i - 1].timestamp).toDateString() !==
                  new Date(a.timestamp).toDateString()) && (
                <h2 className="eyebrow">
                  {new Date(a.timestamp).toDateString() ===
                  new Date().toDateString()
                    ? "Today"
                    : new Intl.DateTimeFormat(undefined, {
                        dateStyle: "medium",
                      }).format(new Date(a.timestamp))}
                </h2>
              )}
              <article
                className={`activity-item ${selected === a.id ? "open" : ""}`}
              >
                <button
                  className="activity-row"
                  onClick={() => setSelected(selected === a.id ? null : a.id)}
                  aria-expanded={selected === a.id}
                >
                  <time>{time(a.timestamp)}</time>
                  <div className="grow">
                    <div>
                      <strong>{a.sender}</strong> <span>{a.subject}</span>
                    </div>
                    <small>
                      {providerLabels[a.provider]}
                      {!waiting(a.action) &&
                        ` · ${categoryLabels[a.category]}`}{" "}
                      {a.sendResult ? `· ${a.sendResult}` : ""}
                    </small>
                  </div>
                  <span className={`badge ${a.action}`}>
                    {labels[a.action] || a.action}
                  </span>
                </button>
                {selected === a.id && (
                  <div className="activity-expanded">
                    {!waiting(a.action) && <ActivityContext a={a} />}
                    <div>
                      <span className="eyebrow">
                        {a.action === "auto_sent"
                          ? "Why Tomo replied"
                          : waiting(a.action)
                            ? "Status"
                            : "Why Tomo acted"}
                      </span>
                      {a.reason}
                    </div>
                    {a.content &&
                      !snapshot?.approvals.some(
                        (x) => x.id === a.messageId && x.plan?.length,
                      ) && (
                        <div>
                          <span className="eyebrow">What it sent</span>“
                          {a.content}”
                        </div>
                      )}
                    {!waiting(a.action) && (
                      <div>
                        <span className="eyebrow">Checks</span>
                        All applicable safety checks ·{" "}
                        {a.humanApproved
                          ? "human reviewed"
                          : "locally evaluated"}
                        {a.action === "held" && " · waiting in Inbox"}
                      </div>
                    )}
                    {a.action === "auto_sent" && (
                      <button
                        className="button"
                        disabled={busy}
                        onClick={() =>
                          run("ask_next_time", { id: a.messageId })
                        }
                      >
                        Ask me next time for this sender
                      </button>
                    )}
                  </div>
                )}
              </article>
            </div>
          ))
        ) : (
          <Empty title={search ? "No matching activity" : "No activity yet"}>
            {search
              ? "Try a different sender or keyword."
              : "Processing decisions will appear here as your inboxes synchronize."}
          </Empty>
        )}
      </section>
      <footer className="page-footer">
        <span className="grow" />
        <button className="button" onClick={() => setClear(true)}>
          Clear History…
        </button>
      </footer>
      <Modal
        open={clear}
        onClose={() => setClear(false)}
        title="Clear processing history?"
      >
        <p>
          Delete local activity records. Pending drafts and send protection
          records remain.
        </p>
        <div className="modal-actions">
          <button className="button" onClick={() => setClear(false)}>
            Cancel
          </button>
          <button
            className="button destructive"
            disabled={busy}
            onClick={async () => {
              if (await run("clear_history")) setClear(false);
            }}
          >
            Clear history
          </button>
        </div>
      </Modal>
    </main>
  );
}
