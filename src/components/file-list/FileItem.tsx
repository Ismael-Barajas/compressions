import { Video, Image, X, CheckCircle, AlertCircle, Loader2 } from "lucide-react";
import type { QueuedFile } from "../../types/compression";
import { formatFileSize, getSavingsPercent } from "../../lib/fileUtils";
import { useCompressionStore } from "../../stores/compressionStore";

interface FileItemProps {
  file: QueuedFile;
}

export function FileItem({ file }: FileItemProps) {
  const removeFile = useCompressionStore((s) => s.removeFile);
  const isCompressing = useCompressionStore((s) => s.isCompressing);

  const statusIcon = {
    queued: null,
    processing: <Loader2 size={16} className="animate-spin" style={{ color: "var(--accent)" }} />,
    complete: <CheckCircle size={16} style={{ color: "var(--success)" }} />,
    error: <AlertCircle size={16} style={{ color: "var(--error)" }} />,
  }[file.status];

  return (
    <div
      className="rounded-lg border p-3 transition-colors"
      style={{
        borderColor: "var(--border)",
        backgroundColor: "var(--bg-secondary)",
      }}
    >
      <div className="flex items-center gap-3">
        {/* Media type icon */}
        {file.mediaType === "video" ? (
          <Video size={18} style={{ color: "var(--text-muted)" }} />
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
            {file.result && file.result.success && (
              <span style={{ color: "var(--success)" }}>
                → {formatFileSize(file.result.outputSize)} (
                {getSavingsPercent(file.result.inputSize, file.result.outputSize)}% saved)
              </span>
            )}
            {file.error && (
              <span style={{ color: "var(--error)" }}>{file.error}</span>
            )}
          </div>
        </div>

        {/* Remove button */}
        {!isCompressing && (
          <button
            onClick={() => removeFile(file.id)}
            className="rounded p-1 transition-colors hover:opacity-70"
            title="Remove file"
          >
            <X size={16} style={{ color: "var(--text-muted)" }} />
          </button>
        )}
      </div>

      {/* Progress bar */}
      {file.status === "processing" && (
        <div className="mt-2">
          <div
            className="h-1.5 overflow-hidden rounded-full"
            style={{ backgroundColor: "var(--bg-tertiary)" }}
          >
            <div
              className="h-full rounded-full transition-all duration-300"
              style={{
                width: `${file.progress}%`,
                backgroundColor: "var(--accent)",
              }}
            />
          </div>
          <span className="mt-0.5 block text-right text-xs" style={{ color: "var(--text-muted)" }}>
            {Math.round(file.progress)}%
          </span>
        </div>
      )}
    </div>
  );
}
