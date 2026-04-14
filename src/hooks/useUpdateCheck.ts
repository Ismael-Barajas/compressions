import { useState, useEffect, useCallback, useRef } from "react";
import { check, type Update, type DownloadEvent } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

interface UpdateState {
  updateAvailable: boolean;
  updateVersion: string | null;
  updateNotes: string | null;
  checking: boolean;
  downloading: boolean;
  downloadProgress: number;
  error: string | null;
}

const initialState: UpdateState = {
  updateAvailable: false,
  updateVersion: null,
  updateNotes: null,
  checking: false,
  downloading: false,
  downloadProgress: 0,
  error: null,
};

export function useUpdateCheck(autoCheck = true) {
  const [state, setState] = useState<UpdateState>(initialState);
  const updateRef = useRef<Update | null>(null);

  const checkForUpdate = useCallback(async () => {
    setState((prev) => ({ ...prev, checking: true, error: null }));
    try {
      const update = await check();
      if (update) {
        updateRef.current = update;
        setState((prev) => ({
          ...prev,
          checking: false,
          updateAvailable: true,
          updateVersion: update.version,
          updateNotes: update.body ?? null,
        }));
      } else {
        updateRef.current = null;
        setState((prev) => ({
          ...prev,
          checking: false,
          updateAvailable: false,
          updateVersion: null,
          updateNotes: null,
        }));
      }
    } catch (e) {
      setState((prev) => ({
        ...prev,
        checking: false,
        error: e instanceof Error ? e.message : String(e),
      }));
    }
  }, []);

  const installUpdate = useCallback(async () => {
    const update = updateRef.current;
    if (!update) return;

    setState((prev) => ({ ...prev, downloading: true, downloadProgress: 0, error: null }));

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
              setState((prev) => ({
                ...prev,
                downloadProgress: Math.round((downloadedBytes / totalBytes) * 100),
              }));
            }
            break;
          case "Finished":
            setState((prev) => ({ ...prev, downloadProgress: 100 }));
            break;
        }
      });
      await relaunch();
    } catch (e) {
      setState((prev) => ({
        ...prev,
        downloading: false,
        error: e instanceof Error ? e.message : String(e),
      }));
    }
  }, []);

  const dismiss = useCallback(() => {
    setState(initialState);
    if (updateRef.current) {
      updateRef.current.close();
      updateRef.current = null;
    }
  }, []);

  useEffect(() => {
    if (autoCheck) {
      checkForUpdate();
    }
  }, [autoCheck, checkForUpdate]);

  return { ...state, checkForUpdate, installUpdate, dismiss };
}
