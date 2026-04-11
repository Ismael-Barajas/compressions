import { Moon, Sun, Clock, Terminal } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { useHistoryStore } from "../../stores/historyStore";
import { useLogStore } from "../../stores/logStore";

function CompressionMark() {
  return (
    <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
      <rect x="1" y="1" width="16" height="4" fill="var(--accent)" />
      <rect x="3" y="7" width="12" height="4" fill="var(--accent)" opacity="0.65" />
      <rect x="5" y="13" width="8" height="4" fill="var(--accent)" opacity="0.35" />
    </svg>
  );
}

export function Header() {
  const theme = useCompressionStore((s) => s.theme);
  const toggleTheme = useCompressionStore((s) => s.toggleTheme);
  const openHistory = useHistoryStore((s) => s.open);
  const openLogs = useLogStore((s) => s.open);

  return (
    <header
      className="flex h-11 items-center justify-between border-b px-4"
      style={{ borderColor: "var(--border)", backgroundColor: "var(--bg-primary)" }}
      data-tauri-drag-region
    >
      <div className="flex items-center gap-2.5">
        <CompressionMark />
        <h1
          className="text-[13px] font-semibold tracking-tight"
          style={{ color: "var(--text-primary)" }}
        >
          Compressions
        </h1>
      </div>
      <div className="flex items-center gap-0.5">
        <HeaderButton onClick={openLogs} title="Application logs" aria-label="Application logs">
          <Terminal size={15} />
        </HeaderButton>
        <HeaderButton onClick={openHistory} title="Compression history" aria-label="Compression history">
          <Clock size={15} />
        </HeaderButton>
        <HeaderButton
          onClick={toggleTheme}
          title={`Switch to ${theme === "light" ? "dark" : "light"} mode`}
          aria-label={`Switch to ${theme === "light" ? "dark" : "light"} mode`}
        >
          {theme === "light" ? <Moon size={15} /> : <Sun size={15} />}
        </HeaderButton>
      </div>
    </header>
  );
}

function HeaderButton({
  children,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      className="p-1.5 transition-colors duration-100"
      style={{ color: "var(--text-muted)" }}
      onMouseEnter={(e) => {
        e.currentTarget.style.color = "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.color = "var(--text-muted)";
      }}
      {...props}
    >
      {children}
    </button>
  );
}
