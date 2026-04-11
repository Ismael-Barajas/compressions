import { useEffect, useRef, useMemo } from "react";
import { X, Trash2, Search, RefreshCw } from "lucide-react";
import { useLogStore } from "../../stores/logStore";
import type { LogLevel } from "../../types/compression";

const LEVEL_COLORS: Record<string, string> = {
  ERROR: "var(--error)",
  WARN: "var(--warning)",
  INFO: "var(--accent)",
  DEBUG: "var(--text-muted)",
  TRACE: "var(--text-muted)",
};

const LEVELS: Array<LogLevel | "ALL"> = ["ALL", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

function formatTimestamp(raw: string): string {
  const d = new Date(raw);
  if (isNaN(d.getTime())) return raw;
  return d.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function LogViewer() {
  const isOpen = useLogStore((s) => s.isOpen);
  const close = useLogStore((s) => s.close);
  const entries = useLogStore((s) => s.entries);
  const searchQuery = useLogStore((s) => s.searchQuery);
  const setSearchQuery = useLogStore((s) => s.setSearchQuery);
  const filterLevel = useLogStore((s) => s.filterLevel);
  const setFilterLevel = useLogStore((s) => s.setFilterLevel);
  const clearLogs = useLogStore((s) => s.clearLogs);
  const loadLogs = useLogStore((s) => s.loadLogs);
  const isLoading = useLogStore((s) => s.isLoading);
  const backdropRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!isOpen) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [isOpen, close]);

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [entries]);

  const filtered = useMemo(() => {
    const q = searchQuery.toLowerCase();
    return entries.filter((e) => {
      if (filterLevel !== "ALL" && e.level !== filterLevel) return false;
      if (q && !e.message.toLowerCase().includes(q) && !(e.target?.toLowerCase().includes(q))) {
        return false;
      }
      return true;
    });
  }, [entries, searchQuery, filterLevel]);

  if (!isOpen) return null;

  return (
    <div
      ref={backdropRef}
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ backgroundColor: "rgba(0, 0, 0, 0.6)" }}
      onClick={(e) => {
        if (e.target === backdropRef.current) close();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Application Logs"
        className="flex max-h-[80vh] w-full max-w-3xl flex-col border"
        style={{
          borderColor: "var(--border)",
          backgroundColor: "var(--bg-primary)",
          boxShadow: "var(--shadow-lg)",
        }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between border-b px-4 py-3"
          style={{ borderColor: "var(--border)" }}
        >
          <h2
            className="text-[11px] font-semibold uppercase tracking-widest"
            style={{ color: "var(--text-muted)" }}
          >
            Application Logs
          </h2>
          <div className="flex items-center gap-2">
            <button
              onClick={loadLogs}
              className="p-1 transition-colors hover:opacity-80"
              style={{ color: "var(--text-muted)" }}
              title="Refresh logs"
            >
              <RefreshCw size={14} />
            </button>
            {entries.length > 0 && (
              <button
                onClick={clearLogs}
                className="flex items-center gap-1 px-2 py-1 text-xs transition-colors hover:opacity-80"
                style={{ color: "var(--error)" }}
                title="Clear logs"
              >
                <Trash2 size={13} />
                Clear
              </button>
            )}
            <button
              onClick={close}
              className="p-1 transition-colors hover:opacity-80"
              style={{ color: "var(--text-muted)" }}
            >
              <X size={15} />
            </button>
          </div>
        </div>

        {/* Filters */}
        <div className="flex items-center gap-2 border-b px-4 py-2" style={{ borderColor: "var(--border)" }}>
          <div
            className="flex flex-1 items-center gap-2 px-2 py-1.5"
            style={{ backgroundColor: "var(--bg-secondary)", border: "1px solid var(--border)" }}
          >
            <Search size={13} style={{ color: "var(--text-muted)" }} />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search logs..."
              className="w-full bg-transparent text-xs outline-none"
              style={{ color: "var(--text-primary)" }}
            />
          </div>
          <div className="flex gap-0.5">
            {LEVELS.map((level) => (
              <button
                key={level}
                onClick={() => setFilterLevel(level)}
                className="px-2 py-1 text-[11px] font-medium transition-all duration-100"
                style={{
                  backgroundColor: filterLevel === level ? "var(--accent)" : "transparent",
                  color: filterLevel === level ? "var(--accent-fg)" : "var(--text-muted)",
                  border: `1px solid ${filterLevel === level ? "var(--accent)" : "var(--border)"}`,
                }}
              >
                {level}
              </button>
            ))}
          </div>
        </div>

        {/* Log entries */}
        <div ref={listRef} className="flex-1 overflow-y-auto px-4 py-2 font-mono text-xs">
          {isLoading ? (
            <div className="py-8 text-center" style={{ color: "var(--text-muted)" }}>
              Loading...
            </div>
          ) : filtered.length === 0 ? (
            <div className="py-8 text-center" style={{ color: "var(--text-muted)" }}>
              {searchQuery || filterLevel !== "ALL" ? "No matching log entries" : "No logs yet"}
            </div>
          ) : (
            <div className="space-y-px">
              {filtered.map((entry, i) => (
                <div
                  key={i}
                  className="flex gap-2 px-2 py-1"
                  style={{ backgroundColor: "var(--bg-secondary)" }}
                >
                  <span style={{ color: "var(--text-muted)", flexShrink: 0 }}>
                    {formatTimestamp(entry.timestamp)}
                  </span>
                  <span
                    className="font-semibold"
                    style={{ color: LEVEL_COLORS[entry.level] ?? "var(--text-primary)", flexShrink: 0, width: "3rem" }}
                  >
                    {entry.level}
                  </span>
                  {entry.target && (
                    <span style={{ color: "var(--text-muted)", flexShrink: 0 }}>
                      {entry.target}
                    </span>
                  )}
                  <span style={{ color: "var(--text-primary)", wordBreak: "break-all" }}>
                    {entry.message}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          className="font-data border-t px-4 py-2"
          style={{ borderColor: "var(--border)", color: "var(--text-muted)" }}
        >
          {entries.length} {entries.length === 1 ? "entry" : "entries"}
          {(searchQuery || filterLevel !== "ALL") && ` · ${filtered.length} shown`}
        </div>
      </div>
    </div>
  );
}
