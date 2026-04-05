import { create } from "zustand";
import type { LogEntry, LogLevel } from "../types/compression";
import { readLogs, clearLogs as clearLogsCmd } from "../lib/commands";

interface LogState {
  entries: LogEntry[];
  isOpen: boolean;
  searchQuery: string;
  filterLevel: LogLevel | "ALL";
  isLoading: boolean;

  open: () => void;
  close: () => void;
  setSearchQuery: (query: string) => void;
  setFilterLevel: (level: LogLevel | "ALL") => void;
  loadLogs: () => Promise<void>;
  clearLogs: () => Promise<void>;
}

export const useLogStore = create<LogState>((set) => ({
  entries: [],
  isOpen: false,
  searchQuery: "",
  filterLevel: "ALL",
  isLoading: false,

  open: () => {
    set({ isOpen: true });
    useLogStore.getState().loadLogs();
  },

  close: () => set({ isOpen: false, searchQuery: "", filterLevel: "ALL" }),

  setSearchQuery: (query) => set({ searchQuery: query }),

  setFilterLevel: (level) => set({ filterLevel: level }),

  loadLogs: async () => {
    set({ isLoading: true });
    try {
      const entries = await readLogs(2000);
      set({ entries, isLoading: false });
    } catch {
      set({ isLoading: false });
    }
  },

  clearLogs: async () => {
    try {
      await clearLogsCmd();
      set({ entries: [] });
    } catch {
      // silently fail
    }
  },
}));
