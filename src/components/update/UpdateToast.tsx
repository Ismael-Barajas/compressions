import { useEffect, useState } from "react";
import { X, Download, RefreshCw } from "lucide-react";

interface UpdateToastProps {
  version: string;
  downloading: boolean;
  downloadProgress: number;
  onInstall: () => void;
  onDismiss: () => void;
}

export function UpdateToast({
  version,
  downloading,
  downloadProgress,
  onInstall,
  onDismiss,
}: UpdateToastProps) {
  const [visible, setVisible] = useState(false);
  const [autoDismiss, setAutoDismiss] = useState(true);

  // Animate in on mount
  useEffect(() => {
    const t = requestAnimationFrame(() => setVisible(true));
    return () => cancelAnimationFrame(t);
  }, []);

  // Auto-dismiss after 15s unless downloading or user interacted
  useEffect(() => {
    if (!autoDismiss || downloading) return;
    const t = setTimeout(() => {
      setVisible(false);
      setTimeout(onDismiss, 200);
    }, 15000);
    return () => clearTimeout(t);
  }, [autoDismiss, downloading, onDismiss]);

  const handleInstall = () => {
    setAutoDismiss(false);
    onInstall();
  };

  return (
    <div
      className="fixed bottom-4 right-4 z-50 flex max-w-xs flex-col gap-2 rounded-lg border p-3 shadow-lg transition-all duration-200"
      style={{
        backgroundColor: "var(--bg-secondary)",
        borderColor: "var(--border)",
        opacity: visible ? 1 : 0,
        transform: visible ? "translateY(0)" : "translateY(8px)",
      }}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex flex-col gap-0.5">
          <span
            className="text-[13px] font-semibold"
            style={{ color: "var(--text-primary)" }}
          >
            Update available
          </span>
          <span
            className="text-[12px]"
            style={{ color: "var(--text-secondary)" }}
          >
            Compressions v{version} is ready
          </span>
        </div>
        {!downloading && (
          <button
            onClick={() => {
              setVisible(false);
              setTimeout(onDismiss, 200);
            }}
            className="shrink-0 p-0.5 transition-colors"
            style={{ color: "var(--text-muted)" }}
            onMouseEnter={(e) => {
              e.currentTarget.style.color = "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.color = "var(--text-muted)";
            }}
            aria-label="Dismiss update notification"
          >
            <X size={14} />
          </button>
        )}
      </div>

      {downloading ? (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center gap-2">
            <RefreshCw
              size={13}
              className="animate-spin"
              style={{ color: "var(--accent)" }}
            />
            <span
              className="text-[12px]"
              style={{ color: "var(--text-secondary)" }}
            >
              {downloadProgress < 100
                ? `Downloading... ${downloadProgress}%`
                : "Installing..."}
            </span>
          </div>
          <div
            className="h-1 w-full overflow-hidden rounded-full"
            style={{ backgroundColor: "var(--border)" }}
          >
            <div
              className="h-full rounded-full transition-all duration-300"
              style={{
                width: `${downloadProgress}%`,
                backgroundColor: "var(--accent)",
              }}
            />
          </div>
        </div>
      ) : (
        <button
          onClick={handleInstall}
          className="flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-medium transition-opacity hover:opacity-90"
          style={{
            backgroundColor: "var(--accent)",
            color: "white",
          }}
        >
          <Download size={13} />
          Update now
        </button>
      )}
    </div>
  );
}
