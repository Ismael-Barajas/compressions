import { useState, useRef, useEffect } from "react";
import { Moon, Sun, Clock, Terminal, Download, RefreshCw, Info } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { useHistoryStore } from "../../stores/historyStore";
import { useLogStore } from "../../stores/logStore";
import { useUpdateCheck } from "../../hooks/useUpdateCheck";

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
        <UpdateButton />
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

function UpdateButton() {
  const { updateAvailable, updateVersion, checking, checkForUpdate, installUpdate } =
    useUpdateCheck(false);
  const [popoverOpen, setPopoverOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close popover on outside click
  useEffect(() => {
    if (!popoverOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setPopoverOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [popoverOpen]);

  return (
    <div ref={ref} className="relative">
      <HeaderButton
        onClick={() => {
          if (!popoverOpen && !updateAvailable && !checking) {
            checkForUpdate();
          }
          setPopoverOpen((v) => !v);
        }}
        title={updateAvailable ? `Update to v${updateVersion}` : "Check for updates"}
        aria-label={updateAvailable ? `Update to v${updateVersion}` : "Check for updates"}
      >
        <div className="relative">
          <Info size={15} />
          {updateAvailable && (
            <span
              className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full"
              style={{ backgroundColor: "var(--accent)" }}
            />
          )}
        </div>
      </HeaderButton>

      {popoverOpen && (
        <div
          className="absolute right-0 top-full z-50 mt-1 w-52 rounded-lg border p-3 shadow-lg"
          style={{
            backgroundColor: "var(--bg-secondary)",
            borderColor: "var(--border)",
          }}
        >
          {checking ? (
            <div className="flex items-center gap-2">
              <RefreshCw
                size={13}
                className="animate-spin"
                style={{ color: "var(--text-muted)" }}
              />
              <span className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                Checking for updates...
              </span>
            </div>
          ) : updateAvailable ? (
            <div className="flex flex-col gap-2">
              <span className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                v{updateVersion} available
              </span>
              <button
                onClick={() => {
                  setPopoverOpen(false);
                  installUpdate();
                }}
                className="flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-medium transition-opacity hover:opacity-90"
                style={{ backgroundColor: "var(--accent)", color: "white" }}
              >
                <Download size={13} />
                Install update
              </button>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <span className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                Compressions v{__APP_VERSION__}
              </span>
              <button
                onClick={checkForUpdate}
                className="flex items-center justify-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors"
                style={{
                  borderColor: "var(--border)",
                  color: "var(--text-secondary)",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.color = "var(--text-primary)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.color = "var(--text-secondary)";
                }}
              >
                <RefreshCw size={13} />
                Check for updates
              </button>
            </div>
          )}
        </div>
      )}
    </div>
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
