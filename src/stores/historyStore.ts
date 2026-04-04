import { create } from "zustand";
import type { HistoryEntry } from "../types/compression";
import { getHistory, clearHistory as clearHistoryCmd } from "../lib/commands";

interface HistoryState {
  entries: HistoryEntry[];
  isOpen: boolean;
  searchQuery: string;
  isLoading: boolean;

  open: () => void;
  close: () => void;
  setSearchQuery: (query: string) => void;
  loadHistory: () => Promise<void>;
  clearHistory: () => Promise<void>;
}

export const useHistoryStore = create<HistoryState>((set) => ({
  entries: [],
  isOpen: false,
  searchQuery: "",
  isLoading: false,

  open: () => {
    set({ isOpen: true });
    // Load fresh data when opening
    useHistoryStore.getState().loadHistory();
  },

  close: () => set({ isOpen: false, searchQuery: "" }),

  setSearchQuery: (query) => set({ searchQuery: query }),

  loadHistory: async () => {
    set({ isLoading: true });
    try {
      const entries = await getHistory();
      set({ entries, isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },

  clearHistory: async () => {
    try {
      await clearHistoryCmd();
      set({ entries: [] });
    } catch {
      // silently fail
    }
  },
}));
