import { useEffect, useMemo } from "react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { CompressTab as CompressTabId } from "../../stores/compressionStore";
import { PresetSelector } from "./PresetSelector";
import { VideoControls } from "./VideoControls";
import { ImageControls } from "./ImageControls";
import { PdfControls } from "./PdfControls";
import { OutputSettings } from "../output/OutputSettings";

const TAB_LABELS: Record<CompressTabId, string> = {
  video: "Video",
  image: "Image",
  pdf: "PDF",
};

export function CompressTab() {
  const files = useCompressionStore((s) => s.files);
  const activeCompressTab = useCompressionStore((s) => s.activeCompressTab);
  const setActiveCompressTab = useCompressionStore((s) => s.setActiveCompressTab);

  const availableTabs = useMemo<CompressTabId[]>(() => {
    const present = new Set<CompressTabId>();
    for (const f of files) {
      if (f.mediaType === "video") present.add("video");
      else if (f.mediaType === "image") present.add("image");
      else if (f.mediaType === "pdf") present.add("pdf");
    }
    const order: CompressTabId[] = ["video", "image", "pdf"];
    return order.filter((t) => present.has(t));
  }, [files]);

  useEffect(() => {
    if (availableTabs.length === 0) {
      if (activeCompressTab !== null) setActiveCompressTab(null);
      return;
    }
    if (!activeCompressTab || !availableTabs.includes(activeCompressTab)) {
      setActiveCompressTab(availableTabs[0]);
    }
  }, [availableTabs, activeCompressTab, setActiveCompressTab]);

  const currentTab = activeCompressTab && availableTabs.includes(activeCompressTab)
    ? activeCompressTab
    : availableTabs[0] ?? null;

  return (
    <div className="flex flex-1 flex-col gap-4 overflow-hidden">
      {/* Media-type sub-tabs — pill/chip style */}
      {availableTabs.length > 0 && (
        <div className="flex gap-1" role="tablist" aria-label="Media type">
          {availableTabs.map((tab) => {
            const isActive = currentTab === tab;
            return (
              <button
                key={tab}
                role="tab"
                aria-selected={isActive}
                onClick={() => setActiveCompressTab(tab)}
                className="px-3 py-1 text-xs font-semibold tracking-tight transition-all duration-100"
                style={{
                  backgroundColor: isActive ? "var(--accent)" : "transparent",
                  color: isActive ? "var(--accent-fg)" : "var(--text-secondary)",
                  border: `1px solid ${isActive ? "var(--accent)" : "var(--border)"}`,
                }}
              >
                {TAB_LABELS[tab]}
              </button>
            );
          })}
        </div>
      )}

      {/* Active media-type controls (scrollable) */}
      <div className="-mr-2 flex flex-1 flex-col gap-4 overflow-y-auto pr-2">
        {currentTab === "video" && (
          <>
            <PresetSelector mediaType="video" />
            <VideoControls />
          </>
        )}
        {currentTab === "image" && (
          <>
            <PresetSelector mediaType="image" />
            <ImageControls />
          </>
        )}
        {currentTab === "pdf" && <PdfControls />}
      </div>

      {/* Output settings pinned at bottom */}
      <OutputSettings />
    </div>
  );
}
