import { useCompressionStore } from "../../stores/compressionStore";
import type { VideoCodec, AudioCodec } from "../../types/compression";

const CODECS: { value: VideoCodec; label: string; description: string }[] = [
  { value: "H264", label: "H.264", description: "Best compatibility" },
  { value: "H265", label: "H.265 / HEVC", description: "Better compression" },
  { value: "AV1", label: "AV1", description: "Best compression, slower" },
];

const RESOLUTIONS = [
  { label: "Original", value: null },
  { label: "4K (2160p)", value: { width: 3840, height: 2160 } },
  { label: "1080p", value: { width: 1920, height: 1080 } },
  { label: "720p", value: { width: 1280, height: 720 } },
  { label: "480p", value: { width: 854, height: 480 } },
];

const FRAMERATES = [
  { label: "Original", value: null },
  { label: "60 fps", value: 60 },
  { label: "30 fps", value: 30 },
  { label: "24 fps", value: 24 },
  { label: "15 fps", value: 15 },
];

const AUDIO_CODECS: { value: AudioCodec; label: string }[] = [
  { value: "AAC", label: "AAC" },
  { value: "Opus", label: "Opus" },
  { value: "Copy", label: "Copy Original" },
  { value: "None", label: "No Audio" },
];

const AUDIO_BITRATES = ["64k", "96k", "128k", "192k", "256k", "320k"];

export function VideoControls() {
  const options = useCompressionStore((s) => s.videoOptions);
  const setOptions = useCompressionStore((s) => s.setVideoOptions);

  return (
    <div className="space-y-4">
      <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        Video Settings
      </h3>

      {/* Codec */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Codec
        </label>
        <div className="grid grid-cols-3 gap-1.5">
          {CODECS.map((c) => (
            <button
              key={c.value}
              onClick={() => setOptions({ codec: c.value })}
              className="rounded-md border px-2 py-1.5 text-center text-xs transition-colors"
              style={{
                borderColor: options.codec === c.value ? "var(--accent)" : "var(--border)",
                backgroundColor: options.codec === c.value ? "color-mix(in srgb, var(--accent) 10%, transparent)" : "transparent",
                color: options.codec === c.value ? "var(--accent)" : "var(--text-secondary)",
              }}
            >
              <div className="font-medium">{c.label}</div>
            </button>
          ))}
        </div>
      </div>

      {/* CRF / Quality */}
      <div>
        <div className="mb-1.5 flex items-center justify-between">
          <label className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
            Quality (CRF)
          </label>
          <span className="text-xs font-mono" style={{ color: "var(--text-muted)" }}>
            {options.crf}
          </span>
        </div>
        <input
          type="range"
          min={0}
          max={51}
          value={options.crf}
          onChange={(e) => setOptions({ crf: Number(e.target.value) })}
          className="w-full accent-[var(--accent)]"
        />
        <div className="mt-0.5 flex justify-between text-[10px]" style={{ color: "var(--text-muted)" }}>
          <span>Higher quality</span>
          <span>Smaller file</span>
        </div>
      </div>

      {/* Resolution */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Resolution
        </label>
        <select
          className="w-full rounded-md border px-2 py-1.5 text-sm"
          style={{
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            color: "var(--text-primary)",
          }}
          value={options.resolution ? `${options.resolution.width}x${options.resolution.height}` : "original"}
          onChange={(e) => {
            if (e.target.value === "original") {
              setOptions({ resolution: null });
            } else {
              const res = RESOLUTIONS.find(
                (r) => r.value && `${r.value.width}x${r.value.height}` === e.target.value,
              );
              if (res?.value) setOptions({ resolution: res.value });
            }
          }}
        >
          {RESOLUTIONS.map((r) => (
            <option key={r.label} value={r.value ? `${r.value.width}x${r.value.height}` : "original"}>
              {r.label}
            </option>
          ))}
        </select>
      </div>

      {/* Frame Rate */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Frame Rate
        </label>
        <select
          className="w-full rounded-md border px-2 py-1.5 text-sm"
          style={{
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            color: "var(--text-primary)",
          }}
          value={options.framerate ?? "original"}
          onChange={(e) =>
            setOptions({
              framerate: e.target.value === "original" ? null : Number(e.target.value),
            })
          }
        >
          {FRAMERATES.map((f) => (
            <option key={f.label} value={f.value ?? "original"}>
              {f.label}
            </option>
          ))}
        </select>
      </div>

      {/* Audio */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Audio Codec
        </label>
        <select
          className="w-full rounded-md border px-2 py-1.5 text-sm"
          style={{
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            color: "var(--text-primary)",
          }}
          value={options.audioCodec}
          onChange={(e) => setOptions({ audioCodec: e.target.value as AudioCodec })}
        >
          {AUDIO_CODECS.map((a) => (
            <option key={a.value} value={a.value}>
              {a.label}
            </option>
          ))}
        </select>
      </div>

      {options.audioCodec !== "None" && options.audioCodec !== "Copy" && (
        <div>
          <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
            Audio Bitrate
          </label>
          <select
            className="w-full rounded-md border px-2 py-1.5 text-sm"
            style={{
              borderColor: "var(--border)",
              backgroundColor: "var(--bg-primary)",
              color: "var(--text-primary)",
            }}
            value={options.audioBitrate ?? "128k"}
            onChange={(e) => setOptions({ audioBitrate: e.target.value })}
          >
            {AUDIO_BITRATES.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        </div>
      )}
    </div>
  );
}
