import { useCompressionStore } from "../../stores/compressionStore";
import { DEFAULT_VIDEO_OPTIONS, DEFAULT_IMAGE_OPTIONS } from "../../stores/compressionStore";
import type { VideoOptions, ImageOptions } from "../../types/compression";

interface PresetDef {
  id: string;
  name: string;
  description: string;
  mediaType: "video" | "image";
  videoOptions?: Partial<VideoOptions>;
  imageOptions?: Partial<ImageOptions>;
}

const BUILTIN_PRESETS: PresetDef[] = [
  {
    id: "video-web",
    name: "Web Optimized",
    description: "H.264, CRF 28, 720p",
    mediaType: "video",
    videoOptions: { codec: "H264", crf: 28, resolution: { width: 1280, height: 720 }, audioCodec: "AAC", audioBitrate: "128k" },
  },
  {
    id: "video-high",
    name: "High Quality",
    description: "H.265, CRF 20",
    mediaType: "video",
    videoOptions: { codec: "H265", crf: 20, resolution: null, audioCodec: "AAC", audioBitrate: "192k" },
  },
  {
    id: "video-small",
    name: "Small File Size",
    description: "H.265, CRF 32, 480p",
    mediaType: "video",
    videoOptions: { codec: "H265", crf: 32, resolution: { width: 854, height: 480 }, audioCodec: "AAC", audioBitrate: "96k" },
  },
  {
    id: "video-social",
    name: "Social Media",
    description: "H.264, CRF 23, 1080p",
    mediaType: "video",
    videoOptions: { codec: "H264", crf: 23, resolution: { width: 1920, height: 1080 }, audioCodec: "AAC", audioBitrate: "128k" },
  },
  {
    id: "image-web",
    name: "Web Optimized",
    description: "WebP, quality 80",
    mediaType: "image",
    imageOptions: { format: "WebP", quality: 80, stripMetadata: true },
  },
  {
    id: "image-high",
    name: "High Quality",
    description: "PNG lossless",
    mediaType: "image",
    imageOptions: { format: "Png", quality: 100, stripMetadata: false },
  },
  {
    id: "image-small",
    name: "Small File Size",
    description: "AVIF, quality 60",
    mediaType: "image",
    imageOptions: { format: "Avif", quality: 60, stripMetadata: true },
  },
  {
    id: "image-thumb",
    name: "Thumbnail",
    description: "JPEG, quality 70, 300px",
    mediaType: "image",
    imageOptions: { format: "Jpeg", quality: 70, resize: { width: 300, height: 300 }, stripMetadata: true },
  },
];

export function PresetSelector() {
  const files = useCompressionStore((s) => s.files);
  const activePreset = useCompressionStore((s) => s.activePreset);
  const setActivePreset = useCompressionStore((s) => s.setActivePreset);
  const setVideoOptions = useCompressionStore((s) => s.setVideoOptions);
  const setImageOptions = useCompressionStore((s) => s.setImageOptions);

  const hasVideos = files.some((f) => f.mediaType === "video");
  const hasImages = files.some((f) => f.mediaType === "image");

  const visiblePresets = BUILTIN_PRESETS.filter((p) => {
    if (p.mediaType === "video" && hasVideos) return true;
    if (p.mediaType === "image" && hasImages) return true;
    return false;
  });

  const handleSelect = (preset: PresetDef) => {
    setActivePreset(preset.id);
    if (preset.videoOptions) {
      setVideoOptions({ ...DEFAULT_VIDEO_OPTIONS, ...preset.videoOptions });
      // Re-set active preset since setVideoOptions clears it
      useCompressionStore.setState({ activePreset: preset.id });
    }
    if (preset.imageOptions) {
      setImageOptions({ ...DEFAULT_IMAGE_OPTIONS, ...preset.imageOptions });
      useCompressionStore.setState({ activePreset: preset.id });
    }
  };

  return (
    <div>
      <h3 className="mb-2 text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        Presets
      </h3>
      <div className="space-y-1">
        {visiblePresets.map((preset) => (
          <button
            key={preset.id}
            onClick={() => handleSelect(preset)}
            className="w-full rounded-md border px-3 py-2 text-left transition-colors"
            style={{
              borderColor: activePreset === preset.id ? "var(--accent)" : "var(--border)",
              backgroundColor: activePreset === preset.id ? "color-mix(in srgb, var(--accent) 10%, transparent)" : "transparent",
            }}
          >
            <div className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>
              {preset.name}
            </div>
            <div className="text-[10px]" style={{ color: "var(--text-muted)" }}>
              {preset.description}
            </div>
          </button>
        ))}
        <button
          onClick={() => setActivePreset(null)}
          className="w-full rounded-md border px-3 py-2 text-left text-xs transition-colors"
          style={{
            borderColor: activePreset === null ? "var(--accent)" : "var(--border)",
            backgroundColor: activePreset === null ? "color-mix(in srgb, var(--accent) 10%, transparent)" : "transparent",
            color: "var(--text-secondary)",
          }}
        >
          Custom
        </button>
      </div>
    </div>
  );
}
