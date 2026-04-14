import { useState, useRef, useEffect } from "react";
import { createPortal } from "react-dom";
import { Video, Image, FileText, X, CheckCircle, AlertCircle, Loader2, StopCircle, RotateCcw, Music, Film } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { QueuedFile } from "../../types/compression";
import { formatFileSize, getSavingsPercent } from "../../lib/fileUtils";
import { useCompressionStore } from "../../stores/compressionStore";
import { useCompression } from "../../hooks/useCompression";

interface FileItemProps {
  file: QueuedFile;
  showThumbnails: boolean;
}

const STATUS_ACCENT: Record<string, string> = {
  processing: "status-accent-processing",
  complete: "status-accent-complete",
  error: "status-accent-error",
};

function MediaIcon({ mediaType, size }: { mediaType: string; size: number }) {
  const style = { color: "var(--text-muted)" };
  if (mediaType === "video") return <Video size={size} style={style} />;
  if (mediaType === "pdf") return <FileText size={size} style={style} />;
  return <Image size={size} style={style} />;
}

export function FileItem({ file, showThumbnails }: FileItemProps) {
  const removeFile = useCompressionStore((s) => s.removeFile);
  const retryFile = useCompressionStore((s) => s.retryFile);
  const isCompressing = useCompressionStore((s) => s.isCompressing);
  const { cancelFile, extractAudioFromFile, convertToGif } = useCompression();

  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const thumbnailSrc = showThumbnails && file.thumbnailPath
    ? convertFileSrc(file.thumbnailPath)
    : null;

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
    processing: <Loader2 size={14} className="animate-spin" style={{ color: "var(--accent)" }} />,
    complete: <CheckCircle size={14} style={{ color: "var(--success)" }} />,
    error: <AlertCircle size={14} style={{ color: "var(--error)" }} />,
  }[file.status];

  const accentClass = STATUS_ACCENT[file.status] ?? "";

  const metaLine = (
    <div className="font-data mt-0.5 flex items-center gap-2" style={{ color: "var(--text-muted)" }}>
      {file.size > 0 && <span>{formatFileSize(file.size)}</span>}
      {file.resolution && (
        <span>{file.resolution.width}x{file.resolution.height}</span>
      )}
      {file.duration != null && file.duration > 0 && (
        <span>{Math.floor(file.duration / 60)}:{String(Math.floor(file.duration % 60)).padStart(2, "0")}</span>
      )}
      {file.result && file.result.success && (() => {
        const ext = file.result.outputPath.split(".").pop()?.toLowerCase();
        const isAudio = ["mp3", "m4a", "flac", "opus", "wav"].includes(ext ?? "");
        const isGif = ext === "gif" && file.mediaType === "video";
        const isConversion = isAudio || isGif;

        if (isConversion) {
          return (
            <span style={{ color: "var(--success)" }}>
              → {formatFileSize(file.result.outputSize)} ({isAudio ? "audio" : "GIF"})
            </span>
          );
        }
        return file.result.outputSize >= file.result.inputSize ? (
          <span style={{ color: "var(--text-muted)" }}>Already optimized</span>
        ) : (
          <span style={{ color: "var(--success)" }}>
            → {formatFileSize(file.result.outputSize)} ({getSavingsPercent(file.result.inputSize, file.result.outputSize)}%)
          </span>
        );
      })()}
      {file.error && (
        <span style={{ color: "var(--error)" }}>{file.error}</span>
      )}
    </div>
  );

  const actions = file.status === "processing" ? (
    <ActionButton onClick={() => cancelFile(file.id)} title="Cancel" color="var(--error)">
      <StopCircle size={14} />
    </ActionButton>
  ) : file.status === "error" ? (
    <div className="flex items-center gap-0.5">
      <ActionButton onClick={() => retryFile(file.id)} title="Retry" color="var(--accent)">
        <RotateCcw size={14} />
      </ActionButton>
      <ActionButton onClick={() => removeFile(file.id)} title="Remove" color="var(--text-muted)">
        <X size={14} />
      </ActionButton>
    </div>
  ) : (
    !isCompressing && (
      <ActionButton onClick={() => removeFile(file.id)} title="Remove" color="var(--text-muted)">
        <X size={14} />
      </ActionButton>
    )
  );

  const progressBar = file.status === "processing" && (
    <div className="mt-2.5">
      <div className="h-[3px] overflow-hidden" style={{ backgroundColor: "var(--bg-tertiary)" }}>
        {file.progress > 0 ? (
          <div
            className="h-full transition-all duration-300"
            style={{
              width: `${file.progress}%`,
              backgroundColor: "var(--accent)",
              boxShadow: "0 0 8px var(--accent-glow)",
            }}
          />
        ) : (
          <div
            className="progress-indeterminate h-full"
            style={{
              backgroundColor: "var(--accent)",
              boxShadow: "0 0 8px var(--accent-glow)",
            }}
          />
        )}
      </div>
      <span className="font-data mt-1 block text-right" style={{ color: "var(--text-muted)" }}>
        {file.progress > 0 ? `${Math.round(file.progress)}%` : "Processing…"}
      </span>
    </div>
  );

  return (
    <>
      <div
        className={`border p-3 transition-all duration-150 ${accentClass}`}
        onContextMenu={handleContextMenu}
        style={{
          borderColor: "var(--border)",
          backgroundColor: "var(--bg-secondary)",
        }}
      >
        <div className="flex items-center gap-3">
          {/* Thumbnail or icon */}
          {showThumbnails ? (
            <div
              className="flex flex-shrink-0 items-center justify-center overflow-hidden"
              style={{
                backgroundColor: "var(--bg-tertiary)",
                width: 96,
                height: 72,
                ...(thumbnailSrc
                  ? {
                      backgroundImage: `url(${thumbnailSrc})`,
                      backgroundSize: "cover",
                      backgroundPosition: "center",
                      animation: "fade-in 0.2s ease",
                    }
                  : {}),
              }}
            >
              {!thumbnailSrc && <MediaIcon mediaType={file.mediaType} size={24} />}
            </div>
          ) : (
            <div
              className="flex h-8 w-8 flex-shrink-0 items-center justify-center"
              style={{ backgroundColor: "var(--bg-tertiary)" }}
            >
              <MediaIcon mediaType={file.mediaType} size={15} />
            </div>
          )}

          {/* File info */}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-2">
              <span
                className="truncate text-[13px] font-medium"
                style={{ color: "var(--text-primary)" }}
                title={file.path}
              >
                {file.name}
              </span>
              {statusIcon}
            </div>
            {metaLine}
          </div>

          {/* Actions */}
          {actions}
        </div>

        {/* Progress bar */}
        {progressBar}
      </div>

      {/* Context menu — portalled to body to escape virtualizer transform context */}
      {contextMenu && createPortal(
        <div
          ref={menuRef}
          className="fixed z-50 border py-1"
          style={{
            left: contextMenu.x,
            top: contextMenu.y,
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            boxShadow: "var(--shadow-lg)",
            minWidth: 180,
          }}
        >
          <ContextMenuItem onClick={handleExtractAudio}>
            <Music size={13} />
            Extract Audio
          </ContextMenuItem>
          <ContextMenuItem onClick={handleConvertToGif}>
            <Film size={13} />
            Convert to GIF
          </ContextMenuItem>
        </div>,
        document.body,
      )}
    </>
  );
}

function ActionButton({
  children,
  color,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { color: string }) {
  return (
    <button
      className="p-1 transition-opacity hover:opacity-70"
      style={{ color }}
      {...props}
    >
      {children}
    </button>
  );
}

function ContextMenuItem({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      className="flex w-full items-center gap-2 px-3 py-2 text-left text-[13px] transition-colors"
      style={{ color: "var(--text-primary)" }}
      onMouseEnter={(e) => (e.currentTarget.style.backgroundColor = "var(--bg-tertiary)")}
      onMouseLeave={(e) => (e.currentTarget.style.backgroundColor = "transparent")}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
