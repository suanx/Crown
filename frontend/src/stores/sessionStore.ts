import { create } from "zustand";
import { agentClient, type ThreadSummary } from "@/api";

interface SessionState {
  threads: ThreadSummary[];
  loading: boolean;
  error: string | null;
  loadThreads: () => Promise<void>;
}

export const useSessionStore = create<SessionState>((set) => ({
  threads: [],
  loading: false,
  error: null,
  loadThreads: async () => {
    set({ loading: true, error: null });
    try {
      const list = await agentClient.listThreads();
      set({ threads: list, loading: false });
    } catch (err) {
      set({
        loading: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },
}));
