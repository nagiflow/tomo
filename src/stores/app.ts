import { create } from "zustand";
import { command } from "../services/api";
import type { Snapshot } from "../types";
interface State {
  snapshot: Snapshot | null;
  selectedId: string | null;
  error: string | null;
  busy: boolean;
  load: () => Promise<void>;
  run: (name: string, args?: Record<string, unknown>) => Promise<boolean>;
  clearError: () => void;
}
export const useApp = create<State>((set, get) => ({
  snapshot: null,
  selectedId: null,
  error: null,
  busy: false,
  clearError: () => set({ error: null }),
  load: async () => {
    try {
      set({ snapshot: await command<Snapshot>("get_snapshot") });
    } catch (e) {
      set({ error: String(e) });
    }
  },
  run: async (name, args = {}) => {
    const settingsChange = [
      "save_settings",
      "set_processing_paused",
      "set_auto_send",
      // saved on blur: must not disable the button whose click caused the blur
      "save_writing_style",
      "save_rules",
    ].includes(name);
    if (get().busy && !settingsChange) return false;
    set(settingsChange ? { error: null } : { busy: true, error: null });
    try {
      await command(name, args);
      await get().load();
      return true;
    } catch (e) {
      set({ error: String(e) });
      return false;
    } finally {
      if (!settingsChange) set({ busy: false });
    }
  },
}));
