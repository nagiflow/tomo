import { describe, expect, it } from "vitest";
import { previewSnapshot } from "./preview";
describe("preview safety", () => {
  it("starts with auto-send disabled and no AI authorization", () => {
    const s = previewSnapshot();
    expect(s.settings.autoSend).toBe(false);
    expect(s.settings.consent).toBe(false);
    expect(s.ai.connected).toBe(false);
  });
  it("never promises a schedule change or incident response action in sample drafts", () => {
    const s = previewSnapshot();
    // Sarah's reschedule is only promised after the user's own instruction revised the plan
    expect(s.approvals[0].instruction).toBeTruthy();
    expect(s.approvals[0].plan?.[0].kind).toBe("calendar");
    expect(s.approvals[1].draft).toContain("more context");
    expect(s.approvals[2].classification.risk).toBe("high");
  });
});
