import { beforeEach, expect, it, vi } from "vitest";
import { command } from "../services/api";
import { previewSnapshot } from "../services/preview";
import { useApp } from "./app";
vi.mock("../services/api", () => ({ command: vi.fn() }));
beforeEach(() => {
  vi.mocked(command).mockReset();
  useApp.setState({ snapshot: previewSnapshot(), error: null, busy: false });
});
it("a slow sync cannot block the processing pause control", async () => {
  let current = previewSnapshot();
  let complete!: () => void;
  const pending = new Promise<void>((resolve) => (complete = resolve));
  vi.mocked(command).mockImplementation(async (name, args = {}) => {
    if (name === "sync_now") {
      await pending;
      return undefined as never;
    }
    if (name === "set_processing_paused") {
      current = {
        ...current,
        settings: {
          ...current.settings,
          paused: args.paused as boolean,
          autoSend: false,
        },
      };
      return undefined as never;
    }
    return current as never;
  });
  const sync = useApp.getState().run("sync_now");
  expect(useApp.getState().busy).toBe(true);
  expect(
    await useApp.getState().run("set_processing_paused", { paused: true }),
  ).toBe(true);
  expect(useApp.getState().snapshot?.settings.paused).toBe(true);
  expect(useApp.getState().busy).toBe(true);
  complete();
  await sync;
  expect(useApp.getState().busy).toBe(false);
  expect(useApp.getState().snapshot?.settings.paused).toBe(true);
});
it("a failed send surfaces its error and releases the frontend action lock", async () => {
  vi.mocked(command).mockRejectedValueOnce(
    Error("Send outcome uncertain. Reconcile first."),
  );
  expect(
    await useApp
      .getState()
      .run("approve_reply", { id: "sarah", content: "Thanks." }),
  ).toBe(false);
  expect(useApp.getState().error).toContain("Reconcile first");
  expect(useApp.getState().busy).toBe(false);
});
