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

interface PresetSelectorProps {
  mediaType: "video" | "image";
}

export function PresetSelector({ mediaType }: PresetSelectorProps) {
  const activePreset = useCompressionStore((s) => s.activePreset);
  const applyPreset = useCompressionStore((s) => s.applyPreset);
  const setActivePreset = useCompressionStore((s) => s.setActivePreset);

  const visiblePresets = BUILTIN_PRESETS.filter((p) => p.mediaType === mediaType);

  const handleSelect = (preset: PresetDef) => {
    applyPreset(preset.id, {
      video: preset.videoOptions ? { ...DEFAULT_VIDEO_OPTIONS, ...preset.videoOptions } : undefined,
      image: preset.imageOptions ? { ...DEFAULT_IMAGE_OPTIONS, ...preset.imageOptions } : undefined,
    });
  };

  return (
    <div>
      <h3 className="mb-2 text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        Preset
      </h3>
      <div className="flex flex-wrap gap-1.5">
        {visiblePresets.map((preset) => {
          const isActive = activePreset === preset.id;
          return (
            <button
              key={preset.id}
              onClick={() => handleSelect(preset)}
              title={preset.description}
              className="rounded-md border px-2.5 py-1 text-xs font-medium transition-colors"
              style={{
                borderColor: isActive ? "var(--accent)" : "var(--border)",
                backgroundColor: isActive ? "color-mix(in srgb, var(--accent) 10%, transparent)" : "transparent",
                color: "var(--text-primary)",
              }}
            >
              {preset.name}
            </button>
          );
        })}
        <button
          onClick={() => setActivePreset(null)}
          className="rounded-md border px-2.5 py-1 text-xs font-medium transition-colors"
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
