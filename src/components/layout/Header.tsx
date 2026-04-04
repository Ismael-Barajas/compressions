import { Moon, Sun, Minimize2, Clock } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { useHistoryStore } from "../../stores/historyStore";

export function Header() {
  const theme = useCompressionStore((s) => s.theme);
  const toggleTheme = useCompressionStore((s) => s.toggleTheme);
  const openHistory = useHistoryStore((s) => s.open);

  return (
    <header
      className="flex h-12 items-center justify-between border-b px-4"
      style={{ borderColor: "var(--border)", backgroundColor: "var(--bg-secondary)" }}
      data-tauri-drag-region
    >
      <div className="flex items-center gap-2">
        <Minimize2 size={18} style={{ color: "var(--accent)" }} />
        <h1 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
          Compressions
        </h1>
      </div>
      <div className="flex items-center gap-1">
        <button
          onClick={openHistory}
          className="rounded-md p-1.5 transition-colors hover:opacity-80"
          style={{ backgroundColor: "var(--bg-tertiary)" }}
          title="Compression history"
        >
          <Clock size={16} style={{ color: "var(--text-secondary)" }} />
        </button>
        <button
          onClick={toggleTheme}
          className="rounded-md p-1.5 transition-colors hover:opacity-80"
          style={{ backgroundColor: "var(--bg-tertiary)" }}
          title={`Switch to ${theme === "light" ? "dark" : "light"} mode`}
        >
          {theme === "light" ? (
            <Moon size={16} style={{ color: "var(--text-secondary)" }} />
          ) : (
            <Sun size={16} style={{ color: "var(--text-secondary)" }} />
          )}
        </button>
      </div>
    </header>
  );
}
