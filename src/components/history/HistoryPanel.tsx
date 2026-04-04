import { useEffect, useRef, useMemo } from "react";
import { X, Trash2, Search, CheckCircle, XCircle } from "lucide-react";
import { useHistoryStore } from "../../stores/historyStore";
import { formatFileSize, getSavingsPercent } from "../../lib/fileUtils";

function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

function getFileName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path;
}

export function HistoryPanel() {
  const isOpen = useHistoryStore((s) => s.isOpen);
  const close = useHistoryStore((s) => s.close);
  const entries = useHistoryStore((s) => s.entries);
  const searchQuery = useHistoryStore((s) => s.searchQuery);
  const setSearchQuery = useHistoryStore((s) => s.setSearchQuery);
  const clearHistory = useHistoryStore((s) => s.clearHistory);
  const isLoading = useHistoryStore((s) => s.isLoading);
  const backdropRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [isOpen, close]);

  const filtered = useMemo(() => {
    const q = searchQuery.toLowerCase();
    const list = entries.filter(
      (e) =>
        !q ||
        getFileName(e.inputPath).toLowerCase().includes(q) ||
        e.mediaType.toLowerCase().includes(q),
    );
    // Most recent first
    return [...list].reverse();
  }, [entries, searchQuery]);

  if (!isOpen) return null;

  return (
    <div
      ref={backdropRef}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={(e) => {
        if (e.target === backdropRef.current) close();
      }}
    >
      <div
        className="flex max-h-[80vh] w-full max-w-2xl flex-col rounded-lg border shadow-xl"
        style={{ borderColor: "var(--border)", backgroundColor: "var(--bg-primary)" }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between border-b px-4 py-3"
          style={{ borderColor: "var(--border)" }}
        >
          <h2 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
            Compression History
          </h2>
          <div className="flex items-center gap-2">
            {entries.length > 0 && (
              <button
                onClick={clearHistory}
                className="flex items-center gap-1 rounded px-2 py-1 text-xs transition-colors hover:opacity-80"
                style={{ color: "var(--error)" }}
                title="Clear history"
              >
                <Trash2 size={14} />
                Clear
              </button>
            )}
            <button
              onClick={close}
              className="rounded p-1 transition-colors hover:opacity-80"
              style={{ color: "var(--text-secondary)" }}
            >
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Search */}
        <div className="border-b px-4 py-2" style={{ borderColor: "var(--border)" }}>
          <div
            className="flex items-center gap-2 rounded px-2 py-1"
            style={{ backgroundColor: "var(--bg-secondary)" }}
          >
            <Search size={14} style={{ color: "var(--text-muted)" }} />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search by filename or type..."
              className="w-full bg-transparent text-xs outline-none"
              style={{ color: "var(--text-primary)" }}
            />
          </div>
        </div>

        {/* List */}
        <div className="flex-1 overflow-y-auto px-4 py-2">
          {isLoading ? (
            <div className="py-8 text-center text-xs" style={{ color: "var(--text-muted)" }}>
              Loading...
            </div>
          ) : filtered.length === 0 ? (
            <div className="py-8 text-center text-xs" style={{ color: "var(--text-muted)" }}>
              {searchQuery ? "No matching entries" : "No compression history yet"}
            </div>
          ) : (
            <div className="space-y-1">
              {filtered.map((entry) => {
                const savings = entry.success
                  ? getSavingsPercent(entry.inputSize, entry.outputSize)
                  : 0;
                return (
                  <div
                    key={entry.id}
                    className="flex items-center gap-3 rounded px-2 py-2 transition-colors"
                    style={{ backgroundColor: "var(--bg-secondary)" }}
                  >
                    {/* Status icon */}
                    {entry.success ? (
                      <CheckCircle size={14} style={{ color: "var(--success)", flexShrink: 0 }} />
                    ) : (
                      <XCircle size={14} style={{ color: "var(--error)", flexShrink: 0 }} />
                    )}

                    {/* File info */}
                    <div className="min-w-0 flex-1">
                      <div
                        className="truncate text-xs font-medium"
                        style={{ color: "var(--text-primary)" }}
                        title={entry.inputPath}
                      >
                        {getFileName(entry.inputPath)}
                      </div>
                      <div className="flex items-center gap-2 text-xs" style={{ color: "var(--text-muted)" }}>
                        <span className="rounded px-1" style={{ backgroundColor: "var(--bg-tertiary)" }}>
                          {entry.mediaType}
                        </span>
                        <span>{formatTimestamp(entry.timestamp)}</span>
                      </div>
                    </div>

                    {/* Size / savings */}
                    <div className="text-right text-xs" style={{ flexShrink: 0 }}>
                      {entry.success ? (
                        <>
                          <div style={{ color: "var(--text-primary)" }}>
                            {formatFileSize(entry.inputSize)} → {formatFileSize(entry.outputSize)}
                          </div>
                          <div
                            style={{
                              color: savings > 0 ? "var(--success)" : "var(--text-muted)",
                            }}
                          >
                            {savings > 0 ? `-${savings}%` : "No change"} · {formatDuration(entry.durationMs)}
                          </div>
                        </>
                      ) : (
                        <div style={{ color: "var(--error)" }}>
                          {entry.error ?? "Failed"}
                        </div>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Footer */}
        {entries.length > 0 && (
          <div
            className="border-t px-4 py-2 text-xs"
            style={{ borderColor: "var(--border)", color: "var(--text-muted)" }}
          >
            {entries.length} {entries.length === 1 ? "entry" : "entries"}
            {searchQuery && ` · ${filtered.length} shown`}
          </div>
        )}
      </div>
    </div>
  );
}
