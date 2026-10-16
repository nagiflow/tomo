// Regenerates the README screenshots and demo GIF from the browser preview (sample data only).
// Usage: pnpm dev (in another terminal), then `node scripts/readme-media.mjs`. Needs ffmpeg for the GIF.
import { chromium } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const url = process.env.TOMO_URL || "http://127.0.0.1:1420";
const out = "docs/media";
const size = { width: 1280, height: 800 };
mkdirSync(out, { recursive: true });

const browser = await chromium.launch();
const shot = async (page, name) =>
  page.screenshot({ path: join(out, `${name}.png`) });
// The sample mail carries a long-link line that guards a layout test; it is noise in a screenshot.
const tidy = async (page) => {
  await page.waitForTimeout(400);
  for (const frame of page.frames().slice(1))
    await frame
      .evaluate(() =>
        document
          .querySelectorAll("p")
          .forEach(
            (p) => p.textContent.includes("used to overflow") && p.remove(),
          ),
      )
      .catch(() => {});
};

// Stills, each page in a fresh tab so session-stored preview state never leaks between them.
{
  const page = await browser.newPage({
    viewport: size,
    deviceScaleFactor: 2,
  });
  await page.goto(url);
  await page.getByRole("heading", { name: "Inbox", exact: true }).waitFor();
  await page.getByRole("button", { name: /Sarah Chen.*Date change/ }).click();
  await tidy(page);
  await shot(page, "inbox");
  for (const name of ["Activity", "Settings"]) {
    await page.getByRole("button", { name, exact: true }).click();
    await page.waitForTimeout(400);
    await shot(page, name.toLowerCase());
  }
  await page.close();
}
const pop = await browser.newPage({
  viewport: { width: 420, height: 640 },
  deviceScaleFactor: 2,
});
await pop.goto(`${url}/#popover`);
await pop.getByRole("heading", { name: "Tomo", exact: true }).waitFor();
await pop.waitForTimeout(400);
await shot(pop, "popover");
await pop.close();

// Demo: review a plan, edit the reply, approve, then find it in Activity.
const videos = mkdtempSync(join(tmpdir(), "tomo-demo-"));
const context = await browser.newContext({
  viewport: size,
  recordVideo: { dir: videos, size },
});
const page = await context.newPage();
const pause = (ms = 900) => page.waitForTimeout(ms);
await page.goto(url);
await page.getByRole("heading", { name: "Inbox", exact: true }).waitFor();
await pause(1200);
await page.getByRole("button", { name: /Sarah Chen.*Date change/ }).click();
await tidy(page);
await pause(1400);
const reply = page.getByRole("textbox", { name: "Reply to Sarah" });
await reply.click();
await reply.fill("");
await reply.pressSequentially(
  "Thanks Sarah. Monday works, I've moved the review and the invite is updated.",
  { delay: 25 },
);
await pause(1200);
await page.getByRole("button", { name: "Approve 2 actions" }).click();
await pause(1500);
await page.getByRole("button", { name: "Activity", exact: true }).click();
await pause(800);
await page.getByRole("button", { name: /Sarah Chen.*You approved/ }).click();
await tidy(page);
await pause(2500);
await context.close();
await browser.close();

const video = join(videos, readdirSync(videos)[0]);
execFileSync("ffmpeg", [
  "-y",
  "-loglevel",
  "error",
  "-ss",
  "0.8",
  "-i",
  video,
  "-vf",
  "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4",
  join(out, "demo.gif"),
]);
rmSync(videos, { recursive: true });
console.log(`Wrote ${out}/`);
