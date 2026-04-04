import { useEffect, useCallback, useState } from "react";
import { useCompressionStore } from "../../stores/compressionStore";
import { probeFile, scanPaths } from "../../lib/commands";
import { getMediaType, getFileName } from "../../lib/fileUtils";
import { useClipboardPaste } from "../../hooks/useClipboardPaste";
import { DropZone } from "../dropzone/DropZone";
import { FileList } from "../file-list/FileList";
import { VideoControls } from "../controls/VideoControls";
import { ImageControls } from "../controls/ImageControls";
import { PresetSelector } from "../controls/PresetSelector";
import { AudioControls } from "../controls/AudioControls";
import { GifControls } from "../controls/GifControls";
import { PdfControls } from "../controls/PdfControls";
import { OutputSettings } from "../output/OutputSettings";
import { ResultsSummary } from "../output/ResultsSummary";
import { HistoryPanel } from "../history/HistoryPanel";
import type { QueuedFile } from "../../types/compression";

export function AppShell() {
  const files = useCompressionStore((s) => s.files);
  const addFiles = useCompressionStore((s) => s.addFiles);
  const updateFileProbe = useCompressionStore((s) => s.updateFileProbe);
  const [isDragOver, setIsDragOver] = useState(false);

  const processFilePaths = useCallback(
    async (paths: string[]) => {
      try {
        const resolvedPaths = await scanPaths(paths);
        const validFiles: QueuedFile[] = [];
        for (const path of resolvedPaths) {
          const mediaType = getMediaType(path);
          if (mediaType) {
            validFiles.push({
              id: crypto.randomUUID(),
              path,
              name: getFileName(path),
              size: 0,
              mediaType,
              status: "queued",
              progress: 0,
            });
          }
        }
        if (validFiles.length > 0) {
          addFiles(validFiles);
        }
      } catch {
        // Fallback: process paths locally
        const validFiles: QueuedFile[] = [];
        for (const path of paths) {
          const mediaType = getMediaType(path);
          if (mediaType) {
            validFiles.push({
              id: crypto.randomUUID(),
              path,
              name: getFileName(path),
              size: 0,
              mediaType,
              status: "queued",
              progress: 0,
            });
          }
        }
        if (validFiles.length > 0) {
          addFiles(validFiles);
        }
      }
    },
    [addFiles],
  );

  // App-level clipboard paste listener — Ctrl+V / Cmd+V to add files
  useClipboardPaste(processFilePaths);

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
      cleanup.then((fn) => fn?.());
    };
  }, [processFilePaths]);

  // Probe newly added files to populate their size
  useEffect(() => {
    const unprobed = files.filter((f) => f.size === 0 && f.status === "queued");
    for (const file of unprobed) {
      probeFile(file.path)
        .then((info) => updateFileProbe(file.id, {
          size: info.size,
          resolution: info.resolution,
          duration: info.duration,
        }))
        .catch(() => {
          // Non-fatal — file is still compressible without size info
        });
    }
  }, [files, updateFileProbe]);
  const hasFiles = files.length > 0;
  const hasVideos = files.some((f) => f.mediaType === "video");
  const hasImages = files.some((f) => f.mediaType === "image");
  const hasPdfs = files.some((f) => f.mediaType === "pdf");
  const allComplete = hasFiles && files.every((f) => f.status === "complete" || f.status === "error");

  return (
    <>
    <HistoryPanel />
    <div className="flex flex-1 overflow-hidden">
      {/* Left panel: files */}
      <div className="flex flex-1 flex-col overflow-hidden p-4">
        {!hasFiles ? (
          <DropZone isDragOver={isDragOver} />
        ) : (
          <>
            {isDragOver && (
              <div
                className="mb-2 rounded-lg border-2 border-dashed p-3 text-center text-sm"
                style={{ borderColor: "var(--accent)", color: "var(--accent)", backgroundColor: "var(--bg-secondary)" }}
              >
                Drop files here to add to queue
              </div>
            )}
            <FileList />
            {allComplete && <ResultsSummary />}
          </>
        )}
      </div>

      {/* Right panel: controls */}
      {hasFiles && (
        <div
          className="flex w-80 flex-col gap-4 overflow-y-auto border-l p-4"
          style={{ borderColor: "var(--border)", backgroundColor: "var(--bg-secondary)" }}
        >
          <PresetSelector />
          {hasVideos && <VideoControls />}
          {hasImages && <ImageControls />}
          {hasVideos && <AudioControls />}
          {hasVideos && <GifControls />}
          {hasPdfs && <PdfControls />}
          <OutputSettings />
        </div>
      )}
    </div>
    </>
  );
}
