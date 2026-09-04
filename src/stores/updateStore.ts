import { create } from "zustand";
import { check, type Update, type DownloadEvent } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/**
 * Single shared updater state. Previously `App` (auto-check) and the header button
 * each ran their own copy of the check hook, so the header never reflected the
 * automatic result and could trigger a second network check.
 */
interface UpdateState {
  updateAvailable: boolean;
  updateVersion: string | null;
  updateNotes: string | null;
  checking: boolean;
  downloading: boolean;
  downloadProgress: number;
  error: string | null;
  /** Set once the automatic startup check has run. */
  autoChecked: boolean;

  checkForUpdate: () => Promise<void>;
  installUpdate: () => Promise<void>;
  dismiss: () => void;
}

let pendingUpdate: Update | null = null;

export const useUpdateStore = create<UpdateState>((set, get) => ({
  updateAvailable: false,
  updateVersion: null,
  updateNotes: null,
  checking: false,
  downloading: false,
  downloadProgress: 0,
  error: null,
  autoChecked: false,

  checkForUpdate: async () => {
    if (get().checking) return;
    set({ checking: true, error: null });
    try {
      const update = await check();
      if (update) {
        pendingUpdate = update;
        set({
          checking: false,
          updateAvailable: true,
          updateVersion: update.version,
          updateNotes: update.body ?? null,
        });
      } else {
        pendingUpdate = null;
        set({
          checking: false,
          updateAvailable: false,
          updateVersion: null,
          updateNotes: null,
        });
      }
    } catch (e) {
      set({ checking: false, error: e instanceof Error ? e.message : String(e) });
    } finally {
      set({ autoChecked: true });
    }
  },

  installUpdate: async () => {
    const update = pendingUpdate;
    if (!update) return;

    set({ downloading: true, downloadProgress: 0, error: null });

    let totalBytes = 0;
    let downloadedBytes = 0;

    try {
      await update.downloadAndInstall((event: DownloadEvent) => {
        switch (event.event) {
          case "Started":
            totalBytes = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloadedBytes += event.data.chunkLength;
            if (totalBytes > 0) {
              set({ downloadProgress: Math.round((downloadedBytes / totalBytes) * 100) });
            }
            break;
          case "Finished":
            set({ downloadProgress: 100 });
            break;
        }
      });
      await relaunch();
    } catch (e) {
      set({ downloading: false, error: e instanceof Error ? e.message : String(e) });
    }
  },

  dismiss: () => {
    set({
      updateAvailable: false,
      updateVersion: null,
      updateNotes: null,
      checking: false,
      downloading: false,
      downloadProgress: 0,
      error: null,
    });
    if (pendingUpdate) {
      pendingUpdate.close().catch(() => {});
      pendingUpdate = null;
    }
  },
}));
