import { useEffect, useCallback, useRef, useState } from "react";
import { useCompressionStore, type FileProbeInfo } from "../../stores/compressionStore";
import { probeFilesBatch, scanPaths } from "../../lib/commands";
import { pathsToQueuedFiles } from "../../lib/fileUtils";
import { loadSupportedMedia } from "../../lib/mediaTypes";
import { useClipboardPaste } from "../../hooks/useClipboardPaste";
import { useKeyboardShortcuts } from "../../hooks/useKeyboardShortcuts";
import { DropZone } from "../dropzone/DropZone";
import { FileList } from "../file-list/FileList";
import { Sidebar } from "./Sidebar";
import { ResultsSummary } from "../output/ResultsSummary";
import { BatchProgressBar } from "../output/BatchProgressBar";
import { HistoryPanel } from "../history/HistoryPanel";
import { LogViewer } from "../logs/LogViewer";

/** Flush probe results to the store at most this often (leading + trailing). */
const PROBE_FLUSH_INTERVAL_MS = 150;
/** ...or as soon as this many results are buffered. */
const PROBE_FLUSH_BATCH = 64;

export function AppShell() {
  const hasFiles = useCompressionStore((s) => s.summary.total > 0);
  const allComplete = useCompressionStore(
    (s) => s.summary.total > 0 && s.summary.complete + s.summary.error === s.summary.total,
  );
  const filesRevision = useCompressionStore((s) => s.filesRevision);
  const addFiles = useCompressionStore((s) => s.addFiles);
  const updateFileProbes = useCompressionStore((s) => s.updateFileProbes);
  const [isDragOver, setIsDragOver] = useState(false);
  const isCompressing = useCompressionStore((s) => s.isCompressing);

  // Pull the supported-extension list from the backend once at startup.
  useEffect(() => {
    loadSupportedMedia();
  }, []);

  const processFilePaths = useCallback(
    async (paths: string[]) => {
      try {
        const resolvedPaths = await scanPaths(paths);
        const newFiles = pathsToQueuedFiles(resolvedPaths);
        if (newFiles.length > 0) addFiles(newFiles);
      } catch {
        // Fallback: process paths locally without scanning
        const newFiles = pathsToQueuedFiles(paths);
        if (newFiles.length > 0) addFiles(newFiles);
      }
    },
    [addFiles],
  );

  // App-level clipboard paste listener — Ctrl+V / Cmd+V to add files
  useClipboardPaste(processFilePaths);

  // Global keyboard shortcuts — Space to start, Escape to cancel
  useKeyboardShortcuts();

  // App-level drag-drop listener — works even during compression
  useEffect(() => {
    let lastDropTime = 0;

    async function setupDragDrop() {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const unlisten = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "over") {
            setIsDragOver(true);
          } else if (event.payload.type === "leave") {
            setIsDragOver(false);
          } else if (event.payload.type === "drop") {
            const now = Date.now();
            if (now - lastDropTime < 100) return;
            lastDropTime = now;
            setIsDragOver(false);
            processFilePaths(event.payload.paths);
          }
        });
        return unlisten;
      } catch {
        return undefined;
      }
    }

    const cleanup = setupDragDrop();
    return () => {
      cleanup.then((fn) => fn?.()).catch(() => {});
    };
  }, [processFilePaths]);

  // Track which files have already been sent for probing to avoid re-probing
  const probedPathsRef = useRef(new Set<string>());
  const probeBufferRef = useRef<Array<{ id: string; info: FileProbeInfo }>>([]);
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastFlushRef = useRef(0);

  const flushProbes = useCallback(() => {
    if (flushTimerRef.current) {
      clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
    }
    lastFlushRef.current = Date.now();
    if (probeBufferRef.current.length > 0) {
      const batch = probeBufferRef.current;
      probeBufferRef.current = [];
      updateFileProbes(batch);
    }
  }, [updateFileProbes]);

  // Probe newly added files — batched with concurrency control.
  // Keyed on `filesRevision` (add/remove/clear) rather than the files array so
  // progress ticks do not re-run the O(n) scan.
  useEffect(() => {
    const files = useCompressionStore.getState().files;

    // Reset probe tracking when all files are cleared so re-adding works
    if (files.length === 0) {
      probedPathsRef.current.clear();
      probeBufferRef.current = [];
      if (flushTimerRef.current) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      return;
    }

    const unprobed = files.filter(
      (f) => f.size === 0 && f.status === "queued" && !probedPathsRef.current.has(f.path),
    );
    if (unprobed.length === 0) return;

    // Build path→id map for resolving probe results
    const pathToId = new Map(unprobed.map((f) => [f.path, f.id]));
    const paths = unprobed.map((f) => f.path);

    // Mark all as in-flight immediately to prevent re-probing
    for (const p of paths) {
      probedPathsRef.current.add(p);
    }

    probeFilesBatch(paths, (event) => {
      const id = pathToId.get(event.path);
      if (!id) return;

      probeBufferRef.current.push({
        id,
        info: {
          size: event.size,
          resolution: event.resolution,
          duration: event.duration,
        },
      });

      // Throttle (not debounce): a steady stream of results still reaches the
      // store every PROBE_FLUSH_INTERVAL_MS instead of only when the stream ends.
      const elapsed = Date.now() - lastFlushRef.current;
      if (
        probeBufferRef.current.length >= PROBE_FLUSH_BATCH ||
        elapsed >= PROBE_FLUSH_INTERVAL_MS
      ) {
        flushProbes();
      } else if (!flushTimerRef.current) {
        flushTimerRef.current = setTimeout(flushProbes, PROBE_FLUSH_INTERVAL_MS - elapsed);
      }
    })
      .then(flushProbes)
      .catch(() => {
        // Non-fatal — files are still compressible without size info
        flushProbes();
      });
  }, [filesRevision, flushProbes]);

  // Never leave a pending flush behind on unmount.
  useEffect(
    () => () => {
      if (flushTimerRef.current) clearTimeout(flushTimerRef.current);
    },
    [],
  );

  return (
    <>
    <HistoryPanel />
    <LogViewer />
    <div className="flex flex-1 overflow-hidden">
      {/* Left panel: files */}
      <div className="flex flex-1 flex-col overflow-hidden p-4">
        {!hasFiles ? (
          <DropZone isDragOver={isDragOver} />
        ) : (
          <>
            {isDragOver && (
              <div
                className="mb-2 border p-3 text-center text-[13px] font-medium"
                style={{
                  borderColor: "var(--accent)",
                  color: "var(--accent)",
                  backgroundColor: "var(--accent-glow)",
                }}
              >
                Drop files here to add to queue
              </div>
            )}
            <FileList />
            {isCompressing && !allComplete && <BatchProgressBar />}
            {allComplete && <ResultsSummary />}
          </>
        )}
      </div>

      {/* Right panel: controls */}
      {hasFiles && <Sidebar />}
    </div>
    </>
  );
}
