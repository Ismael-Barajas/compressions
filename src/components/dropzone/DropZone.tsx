import { useState, useCallback, useEffect } from "react";
import { Upload, FolderOpen } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { getMediaType, getFileName } from "../../lib/fileUtils";
import { scanPaths } from "../../lib/commands";
import type { QueuedFile } from "../../types/compression";

export function DropZone() {
  const [isDragOver, setIsDragOver] = useState(false);
  const addFiles = useCompressionStore((s) => s.addFiles);

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
              size: 0, // will be populated by probeFile
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
        // Fallback: process paths locally if scan_paths unavailable
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
            // Deduplicate Tauri double-fire bug
            if (now - lastDropTime < 100) return;
            lastDropTime = now;

            setIsDragOver(false);
            processFilePaths(event.payload.paths);
          }
        });
        return unlisten;
      } catch {
        // Not running in Tauri (e.g., dev in browser)
        return undefined;
      }
    }

    const cleanup = setupDragDrop();
    return () => {
      cleanup.then((fn) => fn?.());
    };
  }, [processFilePaths]);

  const handleBrowseFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true });
      if (selected) {
        processFilePaths([selected as string]);
      }
    } catch {
      // Dialog cancelled or not in Tauri
    }
  };

  const handleBrowse = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: true,
        filters: [
          {
            name: "Media Files",
            extensions: [
              "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v",
              "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "gif",
            ],
          },
          {
            name: "Video Files",
            extensions: ["mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v"],
          },
          {
            name: "Image Files",
            extensions: ["jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "gif"],
          },
        ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        processFilePaths(paths);
      }
    } catch {
      // Dialog cancelled or not in Tauri
    }
  };

  return (
    <div
      className={`drop-zone flex-1 cursor-pointer ${isDragOver ? "drag-over" : ""}`}
      onClick={handleBrowse}
    >
      <Upload
        size={48}
        strokeWidth={1.5}
        style={{ color: isDragOver ? "var(--accent)" : "var(--text-muted)" }}
        className="mb-4"
      />
      <p className="text-lg font-medium" style={{ color: "var(--text-primary)" }}>
        Drop files or folders here to compress
      </p>
      <p className="mt-1 text-sm" style={{ color: "var(--text-secondary)" }}>
        or click to browse
      </p>
      <p className="mt-4 text-xs" style={{ color: "var(--text-muted)" }}>
        Supports MP4, MKV, AVI, MOV, WebM, JPG, PNG, WebP, AVIF, and more
      </p>
      <div className="mt-4 flex items-center gap-2">
        <button
          className="btn-primary flex items-center gap-2"
          onClick={(e) => {
            e.stopPropagation();
            handleBrowse();
          }}
        >
          <FolderOpen size={16} />
          Browse Files
        </button>
        <button
          className="btn-secondary flex items-center gap-2"
          onClick={(e) => {
            e.stopPropagation();
            handleBrowseFolder();
          }}
        >
          <FolderOpen size={16} />
          Browse Folder
        </button>
      </div>
    </div>
  );
}
