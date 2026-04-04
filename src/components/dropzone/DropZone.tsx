import { Upload, FolderOpen } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { getMediaType, getFileName } from "../../lib/fileUtils";
import { scanPaths } from "../../lib/commands";
import type { QueuedFile } from "../../types/compression";

async function addResolvedPaths(paths: string[]) {
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
    useCompressionStore.getState().addFiles(validFiles);
  }
}

export function DropZone({ isDragOver }: { isDragOver?: boolean }) {

  const handleBrowseFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true });
      if (selected) {
        addResolvedPaths([selected as string]);
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
              "pdf",
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
          {
            name: "PDF Files",
            extensions: ["pdf"],
          },
        ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        addResolvedPaths(paths);
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
        Supports MP4, MKV, AVI, MOV, WebM, JPG, PNG, WebP, AVIF, PDF, and more
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
