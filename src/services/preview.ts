import type { Snapshot, Approval, Activity, PlanAction } from "../types";
const timestamp = (h: string) => {
  const d = new Date();
  const [hr, min] = h.split(":");
  d.setHours(+hr, +min, 0, 0);
  return d.toISOString();
};
export function previewSnapshot(): Snapshot {
  const approvals: Approval[] = [
    {
      id: "sarah",
      message: {
        id: "sarah",
        provider: "gmail",
        accountId: "demo-gmail",
        conversationId: "sarah-thread",
        sender: {
          id: "sarah",
          name: "Sarah Chen",
          address: "sarah@example.com",
        },
        content:
          "Something came up on the client side. Can we move the Q4 design review from Thursday to next Monday? I need to tell them by end of day.",
        timestamp: timestamp("10:42"),
        isDirectMessage: true,
        isMention: false,
        requiresProcessing: false,
        metadata: {},
      },
      subject: "Can we move the Q4 design review?",
      classification: {
        category: "needs_decision",
        confidence: 0.91,
        reason:
          "She wants to move a date. Tomo never agrees to schedule changes for you.",
        suggestedAction: "require_approval",
        risk: "medium",
      },
      draft:
        "Hi Sarah — thanks for the heads-up. I need to check my availability before confirming a new date.",
      context: [
        {
          id: "prior",
          provider: "gmail",
          accountId: "demo-gmail",
          conversationId: "sarah-thread",
          sender: { id: "you", name: "You" },
          content: "Thursday 2pm works — I’ll send the deck Wednesday.",
          timestamp: timestamp("9:00"),
          isDirectMessage: true,
          isMention: false,
          requiresProcessing: false,
          metadata: {},
        },
      ],
      status: "pending",
    },
    {
      id: "engineering",
      message: {
        id: "engineering",
        provider: "outlook",
        accountId: "demo-outlook",
        conversationId: "incident",
        sender: {
          id: "eng",
          name: "Engineering Team",
          address: "engineering@example.com",
        },
        content:
          "Checkout is throwing errors since the 10:05 deploy. Can you take a look? Do we roll back?",
        timestamp: timestamp("10:15"),
        isDirectMessage: true,
        isMention: false,
        requiresProcessing: false,
        metadata: {},
      },
      subject: "Checkout errors after the 10:05 deploy",
      classification: {
        category: "urgent",
        confidence: 0.98,
        reason:
          "This is incident response. Tomo can draft it, but never sends these on its own.",
        suggestedAction: "require_approval",
        risk: "high",
      },
      draft:
        "Thanks for flagging this. What errors are you seeing, and is there an incident thread with more context?",
      context: [],
      status: "pending",
    },
    {
      id: "northline",
      message: {
        id: "northline",
        provider: "gmail",
        accountId: "demo-gmail",
        conversationId: "renewal",
        sender: {
          id: "northline",
          name: "Northline",
          address: "client@northline.example",
        },
        content:
          "Before we sign, can you confirm clause 7 allows termination with 30 days’ notice?",
        timestamp: timestamp("9:58"),
        isDirectMessage: true,
        isMention: false,
        requiresProcessing: false,
        metadata: {},
      },
      subject: "Question on clause 7 of the renewal",
      classification: {
        category: "sensitive",
        confidence: 0.99,
        reason:
          "Contract terms are sensitive. You should read this one yourself.",
        suggestedAction: "require_approval",
        risk: "high",
      },
      draft:
        "Thanks for checking. I need to review the wording of clause 7 before confirming.",
      context: [],
      status: "pending",
    },
  ];
  const calendar: PlanAction = {
    id: "calendar-sarah",
    kind: "calendar",
    operation: "move",
    title: "Move “Q4 design review”",
    summary: "No conflicts on your calendar",
    detail:
      "Moves the meeting and sends updated invitations to the existing guests.",
    params: {
      eventId: "demo-q4",
      originalStart: "Thu, Oct 15 · 2:00–3:00 PM",
      start: { dateTime: "2026-10-12T17:00:00+09:00" },
      end: { dateTime: "2026-10-12T18:00:00+09:00" },
      guests: ["Sarah Chen", "Dev Patel", "Maya Ortiz", "Leo Brandt"],
    },
    status: "pending",
  };
  approvals[0].message.content =
    "Something came up on the client side. Can we move the Q4 design review from Thursday? Monday would work best for us.";
  ["Date change", "Urgent", "Legal"].forEach(
    (tag, i) => (approvals[i].classification.tag = tag),
  );
  // one sample arrives as HTML mail, to show how formatted messages are displayed
  approvals[0].message.metadata = {
    html: '<div style="font-family:Georgia,serif"><p>Hi,</p><p>Something came up on the client side. Can we move the <b>Q4 design review</b> from Thursday? <span style="color:#0b57d0">Monday would work best for us.</span></p><p>Thanks,<br>Sarah</p><img src="https://example.com/tracking.gif" width="1" height="1"><p style="color:#888;font-size:11px">A very long link that used to overflow: https://example.com/a/really/long/path/that/keeps/going/and/going/without/any/spaces/at/all/for/quite/a/while/longer/than/the/box</p></div>',
  };
  approvals[0].instruction = "reschedule the meeting to tomorrow 5pm";
  approvals[0].revision = 1;
  approvals[0].classification.reason =
    "It changes your calendar. Tomo never moves meetings without you.";
  approvals[0].draft =
    "Hi Sarah — done. I’ve moved the Q4 design review to tomorrow (Mon) at 5:00 PM. The updated invite is on its way.\n\nThanks for the heads-up.";
  const mail = (a: Approval): PlanAction => ({
    id: "mail-" + a.id,
    kind: "mail",
    operation: "reply",
    title: "Reply to " + a.message.sender.name.split(" ")[0],
    summary:
      "Sends from your " +
      (a.message.provider === "gmail" ? "Gmail" : "Outlook"),
    detail: a.draft,
    params: { body: a.draft },
    status: "pending",
  });
  approvals[0].plan = [calendar, mail(approvals[0])];
  approvals[1].plan = [mail(approvals[1])];
  approvals[2].plan = [
    {
      id: "calendar-northline",
      kind: "calendar",
      operation: "create",
      title: "Block “Review clause 7 with legal”",
      summary: "Tomorrow, 10:00–10:30 AM",
      detail:
        "New event on your calendar\nBusy · no guests\nLinks back to this email.",
      params: {
        summary: "Review clause 7 with legal",
        start: { dateTime: "2026-10-12T10:00:00+09:00" },
        end: { dateTime: "2026-10-12T10:30:00+09:00" },
      },
      status: "pending",
    },
    mail(approvals[2]),
  ];
  const activity: Activity[] = approvals.map((a) => ({
    id: `event-${a.id}`,
    messageId: a.id,
    accountId: a.message.accountId,
    provider: a.message.provider,
    sender: a.message.sender.name,
    subject: a.subject,
    category: a.classification.category,
    action: "held",
    reason: a.classification.reason,
    timestamp: a.message.timestamp,
    humanApproved: false,
  }));
  activity.push(
    {
      id: "maya",
      messageId: "maya",
      accountId: "demo-gmail",
      provider: "gmail",
      sender: "Maya Ortiz",
      subject: "Asked if the revised deck came through",
      category: "acknowledge",
      action: "auto_sent",
      reason:
        "Maya is a trusted teammate asking for a simple acknowledgment. No dates, approvals or sensitive topics.",
      timestamp: timestamp("10:31"),
      humanApproved: false,
      content: "Thanks, received.",
      sendResult: "Preview only",
    },
    {
      id: "priya",
      messageId: "priya",
      accountId: "demo-gmail",
      provider: "gmail",
      sender: "Priya Nair",
      subject: "Got the onboarding doc link",
      category: "routine_request",
      action: "approved",
      reason: "You reviewed and approved this reply.",
      timestamp: timestamp("9:52"),
      humanApproved: true,
      content: "Here’s the onboarding doc.",
      sendResult: "Preview only",
    },
    {
      id: "newsletter",
      messageId: "newsletter",
      accountId: "demo-gmail",
      provider: "gmail",
      sender: "Weekly newsletter",
      subject: "Product updates this week",
      category: "ignore",
      action: "ignored",
      reason: "Promotional content requires no action.",
      timestamp: timestamp("9:40"),
      humanApproved: false,
    },
  );
  return {
    settings: {
      onboarded: true,
      paused: false,
      autoSend: false,
      yolo: false,
      consent: false,
      notifications: true,
      pollSeconds: 120,
      workers: 1,
      tone: "Brief and warm",
      blockedSenders: [],
      blockedThreads: [],
      googleClientId: "",
      googleClientSecret: "",
      microsoftClientId: "",
      aiProvider: "siwc",
      model: "gpt-6.1-sol",
      tools: {
        calendar: "ask",
        drive: "off",
        docs: "off",
        sheets: "off",
        slides: "off",
        codex: "off",
        contacts: "off",
      },
      googleProjectId: "tomo-project-id",
      googleAccountId: "demo-gmail",
    },
    accounts: [
      {
        id: "demo-gmail",
        provider: "gmail",
        email: "you@gmail.com",
        status: "connected",
        failures: 0,
      },
      {
        id: "demo-outlook",
        provider: "outlook",
        email: "you@company.com",
        status: "connected",
        failures: 0,
      },
    ],
    approvals,
    activity,
    ai: {
      connected: false,
      provider: "siwc",
      status: "Preview · no AI account connected",
      models: [],
    },
    syncing: false,
    codexInstalled: true,
    style: {
      content:
        '# How I write\n\n## Tone\n- Warm but brief: 2–4 sentences, rarely more.\n- Open with "Hi Sarah —". Never "Dear".\n- Few exclamation marks, no emoji.\n\n## Habits\n- Quick yes: "Sounds good, thanks!"\n- When unsure, ask for the information I need.\n- Sign off "Thanks," to clients; no sign-off to teammates.\n\n## With different people\n- Teammates: casual, first names, short.\n- Clients: full sentences, confirm next steps.\n\n## Never\n- Promise dates or prices I haven’t confirmed.',
      sampleCount: 200,
      updatedAt: new Date().toISOString(),
      path: "style.md",
    },
  };
}
