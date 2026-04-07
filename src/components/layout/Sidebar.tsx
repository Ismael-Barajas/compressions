import { useEffect } from "react";
import { useCompressionStore } from "../../stores/compressionStore";
import { CompressTab } from "../controls/CompressTab";
import { ToolsTab } from "../controls/ToolsTab";

export function Sidebar() {
  const files = useCompressionStore((s) => s.files);
  const activeSidebarTab = useCompressionStore((s) => s.activeSidebarTab);
  const setActiveSidebarTab = useCompressionStore((s) => s.setActiveSidebarTab);

  const hasVideos = files.some((f) => f.mediaType === "video");

  // If the user is on the Tools tab and the queue no longer has any videos,
  // bounce them back to Compress so they aren't staring at a disabled tab's contents.
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
      {/* Top-level tab strip */}
      <div
        className="flex gap-1 border-b p-2"
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
      <div className="flex flex-1 flex-col overflow-hidden p-4">
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
      className="flex-1 rounded-md px-3 py-1.5 text-sm font-medium transition-colors"
      style={{
        backgroundColor: isActive ? "var(--accent)" : "transparent",
        color: isActive ? "var(--accent-fg, white)" : disabled ? "var(--text-muted)" : "var(--text-secondary)",
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {label}
    </button>
  );
}
