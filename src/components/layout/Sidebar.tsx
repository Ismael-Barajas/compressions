import { useEffect } from "react";
import { useCompressionStore } from "../../stores/compressionStore";
import { CompressTab } from "../controls/CompressTab";
import { ToolsTab } from "../controls/ToolsTab";

export function Sidebar() {
  const files = useCompressionStore((s) => s.files);
  const activeSidebarTab = useCompressionStore((s) => s.activeSidebarTab);
  const setActiveSidebarTab = useCompressionStore((s) => s.setActiveSidebarTab);

  const hasVideos = files.some((f) => f.mediaType === "video");

  useEffect(() => {
    if (activeSidebarTab === "tools" && !hasVideos) {
      setActiveSidebarTab("compress");
    }
  }, [activeSidebarTab, hasVideos, setActiveSidebarTab]);

  return (
    <div
      className="flex w-80 flex-col overflow-hidden border-l"
      style={{ borderColor: "var(--border)", backgroundColor: "var(--bg-secondary)" }}
    >
      {/* Tab strip — underline style */}
      <div
        className="flex border-b"
        style={{ borderColor: "var(--border)" }}
        role="tablist"
        aria-label="Sidebar"
      >
        <SidebarTabButton
          label="Compress"
          isActive={activeSidebarTab === "compress"}
          disabled={false}
          onClick={() => setActiveSidebarTab("compress")}
        />
        <SidebarTabButton
          label="Tools"
          isActive={activeSidebarTab === "tools"}
          disabled={!hasVideos}
          title={hasVideos ? undefined : "Add a video to use Tools"}
          onClick={() => setActiveSidebarTab("tools")}
        />
      </div>

      {/* Tab content */}
      <div className="flex flex-1 flex-col overflow-y-auto p-4">
        {activeSidebarTab === "compress" && <CompressTab />}
        {activeSidebarTab === "tools" && hasVideos && <ToolsTab />}
      </div>
    </div>
  );
}

interface SidebarTabButtonProps {
  label: string;
  isActive: boolean;
  disabled: boolean;
  title?: string;
  onClick: () => void;
}

function SidebarTabButton({ label, isActive, disabled, title, onClick }: SidebarTabButtonProps) {
  return (
    <button
      role="tab"
      aria-selected={isActive}
      aria-disabled={disabled}
      disabled={disabled}
      title={title}
      onClick={onClick}
      className="relative flex-1 px-4 py-2.5 text-[13px] font-semibold tracking-tight transition-colors"
      style={{
        color: isActive
          ? "var(--accent)"
          : disabled
            ? "var(--text-muted)"
            : "var(--text-secondary)",
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.4 : 1,
      }}
    >
      {label}
      {/* Active underline indicator */}
      {isActive && (
        <span
          className="absolute bottom-0 left-4 right-4 h-[2px]"
          style={{ backgroundColor: "var(--accent)" }}
        />
      )}
    </button>
  );
}
