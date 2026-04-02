import { useCompressionStore } from "../../stores/compressionStore";
import { DropZone } from "../dropzone/DropZone";
import { FileList } from "../file-list/FileList";
import { VideoControls } from "../controls/VideoControls";
import { ImageControls } from "../controls/ImageControls";
import { PresetSelector } from "../controls/PresetSelector";
import { OutputSettings } from "../output/OutputSettings";
import { ResultsSummary } from "../output/ResultsSummary";

export function AppShell() {
  const files = useCompressionStore((s) => s.files);
  const hasFiles = files.length > 0;
  const hasVideos = files.some((f) => f.mediaType === "video");
  const hasImages = files.some((f) => f.mediaType === "image");
  const allComplete = hasFiles && files.every((f) => f.status === "complete" || f.status === "error");

  return (
    <div className="flex flex-1 overflow-hidden">
      {/* Left panel: files */}
      <div className="flex flex-1 flex-col overflow-hidden p-4">
        {!hasFiles ? (
          <DropZone />
        ) : (
          <>
            <FileList />
            {allComplete && <ResultsSummary />}
          </>
        )}
      </div>

      {/* Right panel: controls */}
      {hasFiles && (
        <div
          className="flex w-80 flex-col gap-4 overflow-y-auto border-l p-4"
          style={{ borderColor: "var(--border)", backgroundColor: "var(--bg-secondary)" }}
        >
          <PresetSelector />
          {hasVideos && <VideoControls />}
          {hasImages && <ImageControls />}
          <OutputSettings />
        </div>
      )}
    </div>
  );
}
