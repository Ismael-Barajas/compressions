import { useState, useRef, useEffect } from "react";
import { Video, Image, FileText, X, CheckCircle, AlertCircle, Loader2, StopCircle, RotateCcw, Music, Film } from "lucide-react";
import type { QueuedFile } from "../../types/compression";
import { formatFileSize, getSavingsPercent } from "../../lib/fileUtils";
import { useCompressionStore } from "../../stores/compressionStore";
import { useCompression } from "../../hooks/useCompression";

interface FileItemProps {
  file: QueuedFile;
}

export function FileItem({ file }: FileItemProps) {
  const removeFile = useCompressionStore((s) => s.removeFile);
  const retryFile = useCompressionStore((s) => s.retryFile);
  const isCompressing = useCompressionStore((s) => s.isCompressing);
  const { cancelFile, extractAudioFromFile, convertToGif } = useCompression();

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // Close context menu on outside click or escape
  useEffect(() => {
    if (!contextMenu) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setContextMenu(null);
      }
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setContextMenu(null);
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [contextMenu]);

  const handleContextMenu = (e: React.MouseEvent) => {
    if (file.mediaType !== "video") return;
    if (file.status === "processing") return;
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY });
  };

  const handleExtractAudio = () => {
    setContextMenu(null);
    extractAudioFromFile(file);
  };

  const handleConvertToGif = () => {
    setContextMenu(null);
    convertToGif(file);
  };

  const statusIcon = {
    queued: null,
    processing: <Loader2 size={16} className="animate-spin" style={{ color: "var(--accent)" }} />,
    complete: <CheckCircle size={16} style={{ color: "var(--success)" }} />,
    error: <AlertCircle size={16} style={{ color: "var(--error)" }} />,
  }[file.status];

  return (
    <>
      <div
        className="rounded-lg border p-3 transition-colors"
        style={{
          borderColor: "var(--border)",
          backgroundColor: "var(--bg-secondary)",
        }}
        onContextMenu={handleContextMenu}
      >
        <div className="flex items-center gap-3">
          {/* Media type icon */}
          {file.mediaType === "video" ? (
            <Video size={18} style={{ color: "var(--text-muted)" }} />
          ) : file.mediaType === "pdf" ? (
            <FileText size={18} style={{ color: "var(--text-muted)" }} />
          ) : (
            <Image size={18} style={{ color: "var(--text-muted)" }} />
          )}

          {/* File info */}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span
                className="truncate text-sm font-medium"
                style={{ color: "var(--text-primary)" }}
                title={file.path}
              >
                {file.name}
              </span>
              {statusIcon}
            </div>
            <div className="flex items-center gap-2 text-xs" style={{ color: "var(--text-muted)" }}>
              {file.size > 0 && <span>{formatFileSize(file.size)}</span>}
              {file.resolution && (
                <span>{file.resolution.width}x{file.resolution.height}</span>
              )}
              {file.duration != null && file.duration > 0 && (
                <span>{Math.floor(file.duration / 60)}:{String(Math.floor(file.duration % 60)).padStart(2, "0")}</span>
              )}
              {file.result && file.result.success && (
                file.result.outputSize >= file.result.inputSize ? (
                  <span style={{ color: "var(--text-muted)" }}>Already optimized</span>
                ) : (
                  <span style={{ color: "var(--success)" }}>
                    → {formatFileSize(file.result.outputSize)} (
                    {getSavingsPercent(file.result.inputSize, file.result.outputSize)}% saved)
                  </span>
                )
              )}
              {file.error && (
                <span style={{ color: "var(--error)" }}>{file.error}</span>
              )}
            </div>
          </div>

          {/* Cancel / Retry / Remove button */}
          {file.status === "processing" ? (
            <button
              onClick={() => cancelFile(file.id)}
              className="rounded p-1 transition-colors hover:opacity-70"
              title="Cancel compression"
              aria-label="Cancel compression"
            >
              <StopCircle size={16} style={{ color: "var(--error)" }} />
            </button>
          ) : file.status === "error" ? (
            <div className="flex items-center gap-1">
              <button
                onClick={() => retryFile(file.id)}
                className="rounded p-1 transition-colors hover:opacity-70"
                title="Retry compression"
                aria-label="Retry compression"
              >
                <RotateCcw size={16} style={{ color: "var(--accent)" }} />
              </button>
              <button
                onClick={() => removeFile(file.id)}
                className="rounded p-1 transition-colors hover:opacity-70"
                title="Remove file"
                aria-label="Remove file"
              >
                <X size={16} style={{ color: "var(--text-muted)" }} />
              </button>
            </div>
          ) : (
            !isCompressing && (
              <button
                onClick={() => removeFile(file.id)}
                className="rounded p-1 transition-colors hover:opacity-70"
                title="Remove file"
                aria-label="Remove file"
              >
                <X size={16} style={{ color: "var(--text-muted)" }} />
              </button>
            )
          )}
        </div>

        {/* Progress bar */}
        {file.status === "processing" && (
          <div className="mt-2">
            <div
              className="h-1.5 overflow-hidden rounded-full"
              style={{ backgroundColor: "var(--bg-tertiary)" }}
            >
              {file.progress > 0 ? (
                <div
                  className="h-full rounded-full transition-all duration-300"
                  style={{
                    width: `${file.progress}%`,
                    backgroundColor: "var(--accent)",
                  }}
                />
              ) : (
                <div
                  className="progress-indeterminate h-full rounded-full"
                  style={{ backgroundColor: "var(--accent)" }}
                />
              )}
            </div>
            <span className="mt-0.5 block text-right text-xs" style={{ color: "var(--text-muted)" }}>
              {file.progress > 0 ? `${Math.round(file.progress)}%` : "Processing…"}
            </span>
          </div>
        )}
      </div>

      {/* Context menu (portal to body to avoid clipping) */}
      {contextMenu && (
        <div
          ref={menuRef}
          className="fixed z-50 rounded-lg border py-1 shadow-lg"
          style={{
            left: contextMenu.x,
            top: contextMenu.y,
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            minWidth: 180,
          }}
        >
          <button
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors hover:opacity-80"
            style={{ color: "var(--text-primary)" }}
            onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--bg-tertiary)")}
            onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}
            onClick={handleExtractAudio}
          >
            <Music size={14} />
            Extract Audio
          </button>
          <button
            className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm transition-colors hover:opacity-80"
            style={{ color: "var(--text-primary)" }}
            onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--bg-tertiary)")}
            onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}
            onClick={handleConvertToGif}
          >
            <Film size={14} />
            Convert to GIF
          </button>
        </div>
      )}
    </>
  );
}
