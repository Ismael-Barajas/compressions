import { Upload, FolderOpen } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { pathsToQueuedFiles } from "../../lib/fileUtils";
import { scanPaths } from "../../lib/commands";

async function addResolvedPaths(paths: string[]) {
  const resolvedPaths = await scanPaths(paths);
  const newFiles = pathsToQueuedFiles(resolvedPaths);
  if (newFiles.length > 0) {
    useCompressionStore.getState().addFiles(newFiles);
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
              "mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma", "aiff", "ape", "alac", "ac3", "dts", "pcm", "amr",
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
            name: "Audio Files",
            extensions: ["mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma", "aiff", "ape", "alac", "ac3", "dts", "pcm", "amr"],
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
      {/* Geometric upload icon */}
      <div
        className="relative mb-6 flex items-center justify-center"
        style={{
          width: 72,
          height: 72,
          border: `1.5px solid ${isDragOver ? "var(--accent)" : "var(--border)"}`,
          transition: "border-color 0.2s, box-shadow 0.2s",
          boxShadow: isDragOver ? "0 0 24px var(--accent-glow)" : "none",
        }}
      >
        <Upload
          size={28}
          strokeWidth={1.5}
          style={{ color: isDragOver ? "var(--accent)" : "var(--text-muted)" }}
        />
      </div>

      <p className="text-base font-semibold tracking-tight" style={{ color: "var(--text-primary)" }}>
        Drop files or folders here
      </p>
      <p className="mt-1.5 text-sm" style={{ color: "var(--text-secondary)" }}>
        or click anywhere to browse
      </p>

      <div className="mt-6 font-mono text-[10px] tracking-wide" style={{ color: "var(--text-muted)" }}>
        MP4 / MKV / MOV / WebM / JPG / PNG / WebP / AVIF / GIF / MP3 / FLAC / WAV / OGG / AAC / PDF
      </div>

      <div className="relative z-10 mt-6 flex items-center gap-3">
        <button
          className="btn-primary flex items-center gap-2"
          onClick={(e) => {
            e.stopPropagation();
            handleBrowse();
          }}
        >
          <FolderOpen size={15} />
          Browse Files
        </button>
        <button
          className="btn-secondary flex items-center gap-2"
          onClick={(e) => {
            e.stopPropagation();
            handleBrowseFolder();
          }}
        >
          <FolderOpen size={15} />
          Browse Folder
        </button>
      </div>
    </div>
  );
}
