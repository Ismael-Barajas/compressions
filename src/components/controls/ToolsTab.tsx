import { Music, Film } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { useCompression } from "../../hooks/useCompression";
import { AudioControls } from "./AudioControls";
import { GifControls } from "./GifControls";

export function ToolsTab() {
  const files = useCompressionStore((s) => s.files);
  const isCompressing = useCompressionStore((s) => s.isCompressing);
  const { extractAudioFromAll, convertAllToGif } = useCompression();

  const hasQueuedVideos = files.some((f) => f.mediaType === "video" && f.status === "queued");

  return (
    <div className="flex flex-col gap-5">
      <p className="text-[11px]" style={{ color: "var(--text-muted)" }}>
        Right-click a video for single file, or use the buttons below for all queued videos.
      </p>
      <AudioControls />
      {hasQueuedVideos && !isCompressing && (
        <button
          className="btn-primary flex items-center justify-center gap-2 py-2 text-[13px]"
          onClick={extractAudioFromAll}
        >
          <Music size={14} />
          Extract Audio from All Videos
        </button>
      )}
      <div className="border-t" style={{ borderColor: "var(--border)" }} />
      <GifControls />
      {hasQueuedVideos && !isCompressing && (
        <button
          className="btn-primary flex items-center justify-center gap-2 py-2 text-[13px]"
          onClick={convertAllToGif}
        >
          <Film size={14} />
          Convert All Videos to GIF
        </button>
      )}
    </div>
  );
}
