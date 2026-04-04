import { Plus, Trash2, Play, FolderOpen } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { useCompression } from "../../hooks/useCompression";
import { scanPaths } from "../../lib/commands";
import { getMediaType, getFileName } from "../../lib/fileUtils";
import { FileItem } from "./FileItem";

function addResolvedPaths(paths: string[]) {
  const newFiles = paths
    .map((path) => {
      const mediaType = getMediaType(path);
      if (!mediaType) return null;
      return {
        id: crypto.randomUUID(),
        path,
        name: getFileName(path),
        size: 0,
        mediaType,
        status: "queued" as const,
        progress: 0,
      };
    })
    .filter((f): f is NonNullable<typeof f> => f !== null);
  if (newFiles.length > 0) {
    useCompressionStore.getState().addFiles(newFiles);
  }
}

export function FileList() {
  const files = useCompressionStore((s) => s.files);
  const clearFiles = useCompressionStore((s) => s.clearFiles);
  const isCompressing = useCompressionStore((s) => s.isCompressing);
  const { startCompression } = useCompression();

  const handleAddMore = async () => {
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
        ],
      });
      if (selected) {
        const paths = Array.isArray(selected) ? selected : [selected];
        const resolved = await scanPaths(paths);
        addResolvedPaths(resolved);
      }
    } catch {
      // cancelled
    }
  };

  const handleAddFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true });
      if (selected) {
        const resolved = await scanPaths([selected as string]);
        addResolvedPaths(resolved);
      }
    } catch {
      // cancelled
    }
  };

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* Toolbar */}
      <div className="mb-3 flex items-center justify-between">
        <span className="text-sm font-medium" style={{ color: "var(--text-secondary)" }}>
          {files.length} file{files.length !== 1 ? "s" : ""} queued
        </span>
        <div className="flex gap-2">
          <button
            className="btn-secondary flex items-center gap-1.5 text-xs"
            onClick={handleAddMore}
          >
            <Plus size={14} />
            Add More
          </button>
          <button
            className="btn-secondary flex items-center gap-1.5 text-xs"
            onClick={handleAddFolder}
          >
            <FolderOpen size={14} />
            Add Folder
          </button>
          <button
            className="btn-secondary flex items-center gap-1.5 text-xs"
            onClick={clearFiles}
            style={{ color: "var(--error)" }}
          >
            <Trash2 size={14} />
            Clear All
          </button>
        </div>
      </div>

      {/* File list */}
      <div className="flex-1 space-y-2 overflow-y-auto">
        {files.map((file) => (
          <FileItem key={file.id} file={file} />
        ))}
      </div>

      {/* Compress button */}
      {!isCompressing && files.some((f) => f.status === "queued") && (
        <div className="mt-4 flex justify-center">
          <button
            className="btn-primary flex items-center gap-2 px-8 py-2.5 text-base"
            onClick={startCompression}
          >
            <Play size={18} />
            Start Compression
          </button>
        </div>
      )}
    </div>
  );
}
