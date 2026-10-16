import { test, expect } from "@playwright/test";
test("review, edit, approve a plan, and retain the outcome across reload", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Inbox", exact: true }),
  ).toBeVisible();
  // the inbox lists the oldest mail first, so Sarah's newer message is picked by hand
  await page
    .getByRole("button", {
      name: /Sarah Chen.*Date change.*Calendar → Gmail/,
    })
    .click();
  await page
    .getByRole("textbox", { name: "Reply to Sarah" })
    .fill("Thanks Sarah. I will check availability before confirming.");
  await page.getByRole("button", { name: "Approve 2 actions" }).click();
  await expect(
    page.getByRole("button", { name: "Inbox 2", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await page.getByRole("searchbox", { name: "Search activity" }).fill("Sarah");
  await page.getByRole("button", { name: /Sarah Chen.*You approved/ }).click();
  // the expanded item shows the plan as it ran, ending with the reply that was sent
  const opened = page.locator(".activity-item.open");
  await expect(opened.getByText("What Tomo did")).toBeVisible();
  await expect(
    opened.getByRole("textbox", { name: "Reply to Sarah" }),
  ).toHaveValue("Thanks Sarah. I will check availability before confirming.");
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Inbox 2", exact: true }),
  ).toBeVisible();
});
test("later rotates selection, dismiss resolves a plan, and blocking a sender persists", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Later", exact: true }).click();
  await expect(
    page.getByRole("heading", {
      name: "Checkout errors after the 10:05 deploy",
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Never automate Engineering" })
    .click();
  await expect(
    page.getByRole("button", { name: "Automation blocked" }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Dismiss", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Inbox 2", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(
    page.getByText("engineering@example.com", { exact: true }),
  ).toBeVisible();
});
test("history deletion requires confirmation and does not delete the approval queue", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await page.getByRole("button", { name: "Clear History…" }).click();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(
    page.getByRole("button", { name: /Sarah Chen.*Held for you/ }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Clear History…" }).click();
  await page
    .getByRole("button", { name: "Clear history", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "No activity yet" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Inbox 3", exact: true }),
  ).toBeVisible();
});
test("popover review and empty state work at the design width", async ({
  page,
}) => {
  await page.setViewportSize({ width: 420, height: 820 });
  await page.goto("/#popover");
  await expect(
    page.getByRole("heading", { name: "Tomo", exact: true }),
  ).toBeVisible();
  await expect(page.getByText("Date change", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Details", exact: true }).click();
  await expect(page.getByText("Why Tomo asked", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Plan", exact: true }).click();
  await page.getByRole("button", { name: "Approve 2 actions" }).click();
  await page.getByRole("button", { name: "Send reply" }).click();
  await page.getByRole("button", { name: "Approve 2 actions" }).click();
  await expect(
    page.getByRole("heading", { name: "All caught up" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Open Tomo window" }).click();
  await expect(
    page.getByRole("heading", { name: "Inbox", exact: true }),
  ).toBeVisible();
});
test("reset returns to onboarding and the four setup steps follow the design", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Reset Tomo…" }).click();
  await page.getByRole("button", { name: "Reset Tomo", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Hi, I’m Tomo" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Connect your Google account" }),
  ).toBeVisible();
  // six illustrated steps, including the one that prevents Google's 7-day sign-out
  const guide = page.getByRole("region", { name: "Google setup guide" });
  await expect(
    guide.getByRole("heading", { name: "Create a project" }),
  ).toBeVisible();
  for (let i = 0; i < 3; i++)
    await guide.getByRole("button", { name: "Next" }).click();
  await expect(
    guide.getByRole("heading", { name: "Publish the app" }),
  ).toBeVisible();
  await expect(
    guide.getByRole("link", { name: "Open this page in Google" }),
  ).toHaveAttribute("href", /auth\/audience$/);
  await page.getByRole("button", { name: "Choose File…" }).click();
  await expect(
    page.getByRole("heading", { name: "Choose what Tomo can use" }),
  ).toBeVisible();
  // the file alone does not sign in: Google is asked once, after the services are chosen
  await expect(page.getByText("Google not connected")).toBeVisible();
  await expect(
    page.getByRole("switch", { name: "Google Calendar" }),
  ).toHaveAttribute("aria-checked", "true");
  await expect(
    page.getByRole("switch", { name: "Google Drive" }),
  ).toHaveAttribute("aria-checked", "false");
  await page.getByRole("button", { name: "Sign In with Google" }).click();
  await expect(
    page.getByRole("heading", { name: "Here’s how you write" }),
  ).toBeVisible();
  // style.md saves when focus leaves it; Start Tomo must still work on the same click
  await page
    .getByRole("textbox", { name: "style.md" })
    .fill("# Mine\n- Short.");
  await page.getByRole("button", { name: "Start Tomo" }).click();
  await expect(
    page.getByRole("heading", { name: "All caught up" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "style.md" })).toHaveValue(
    "# Mine\n- Short.",
  );
});
test("setup can be finished without connecting Google", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Reset Tomo…" }).click();
  await page.getByRole("button", { name: "Reset Tomo", exact: true }).click();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.getByRole("button", { name: "Connect later" }).click();
  await expect(page.getByText("Google not connected")).toBeVisible();
  await page.getByRole("button", { name: "Continue", exact: true }).click();
  await page.getByRole("button", { name: "Start Tomo" }).click();
  await expect(
    page.getByRole("heading", { name: "All caught up" }),
  ).toBeVisible();
});
test("layout fits a small window and all primary screens remain reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  for (const name of ["Activity", "Settings"]) {
    await page.getByRole("button", { name, exact: true }).click();
    await expect(
      page.getByRole("heading", { name, exact: true }),
    ).toBeVisible();
    await expect
      .poll(() =>
        page.evaluate(
          () => document.documentElement.scrollWidth <= window.innerWidth,
        ),
      )
      .toBe(true);
  }
  await page.screenshot({
    path: "test-results/small-window.png",
    fullPage: true,
  });
});
test("inspect and search cached messages locally", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByRole("button", { name: "Show Stored Data", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Search cached messages" })
    .fill("clause");
  await expect(
    page.getByText("1 of 3 cached messages. Search runs on this device."),
  ).toBeVisible();
  await page.getByText("Northline · Gmail", { exact: false }).click();
  await expect(
    page.getByText(
      "Before we sign, can you confirm clause 7 allows termination with 30 days’ notice?",
      { exact: true },
    ),
  ).toBeVisible();
});
test("Tomo can send easy replies alone, with a per-sender opt-out", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page
    .getByLabel("Replies Tomo sends alone")
    .selectOption({ label: "Send the easy ones" });
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await page.getByRole("button", { name: /Maya Ortiz.*Sent by Tomo/ }).click();
  await page
    .getByRole("button", { name: "Ask me next time for this sender" })
    .click();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Tomo always asks first for" }),
  ).toBeVisible();
});
test("processing pause remains reachable and can be resumed", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  await expect(
    page.getByText("Processing paused", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Resume", exact: true }).click();
  await expect(
    page.getByText("Tomo · Community preview", { exact: true }),
  ).toBeVisible();
});
test("the Codex tool is off until a folder is chosen and can be turned off again", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Choose Folder…" }).click();
  await expect(
    page.getByText("Works only inside ~/Projects/site"),
  ).toBeVisible();
  await page.getByRole("button", { name: "Turn Off" }).click();
  await expect(
    page.getByRole("button", { name: "Choose Folder…" }),
  ).toBeVisible();
});
