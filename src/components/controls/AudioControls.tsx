import { useCompressionStore } from "../../stores/compressionStore";
import type { AudioOutputFormat } from "../../types/compression";

const FORMATS: { value: AudioOutputFormat; label: string; description: string }[] = [
  { value: "Mp3", label: "MP3", description: "Universal compatibility" },
  { value: "Aac", label: "AAC", description: "Better quality per bit" },
  { value: "Opus", label: "Opus", description: "Best at low bitrates" },
  { value: "Flac", label: "FLAC", description: "Lossless" },
  { value: "Wav", label: "WAV", description: "Uncompressed" },
];

const BITRATES = ["64k", "96k", "128k", "192k", "256k", "320k"];

const SAMPLE_RATES = [
  { label: "Original", value: null },
  { label: "48000 Hz", value: 48000 },
  { label: "44100 Hz", value: 44100 },
  { label: "22050 Hz", value: 22050 },
];

export function AudioControls() {
  const options = useCompressionStore((s) => s.audioOptions);
  const setOptions = useCompressionStore((s) => s.setAudioOptions);

  const isLossless = options.format === "Flac" || options.format === "Wav";

  return (
    <div className="space-y-4">
      <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        Audio Extraction
      </h3>

      {/* Format */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Format
        </label>
        <div className="grid grid-cols-3 gap-1.5">
          {FORMATS.map((f) => (
            <button
              key={f.value}
              onClick={() => setOptions({ format: f.value })}
              className="rounded-md border px-2 py-1.5 text-center text-xs transition-colors"
              style={{
                borderColor: options.format === f.value ? "var(--accent)" : "var(--border)",
                backgroundColor: options.format === f.value ? "color-mix(in srgb, var(--accent) 10%, transparent)" : "transparent",
                color: options.format === f.value ? "var(--accent)" : "var(--text-secondary)",
              }}
            >
              <div className="font-medium">{f.label}</div>
            </button>
          ))}
        </div>
      </div>

      {/* Bitrate (only for lossy formats) */}
      {!isLossless && (
        <div>
          <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
            Bitrate
          </label>
          <select
            className="w-full rounded-md border px-2 py-1.5 text-sm"
            style={{
              borderColor: "var(--border)",
              backgroundColor: "var(--bg-primary)",
              color: "var(--text-primary)",
            }}
            value={options.bitrate ?? "192k"}
            onChange={(e) => setOptions({ bitrate: e.target.value })}
          >
            {BITRATES.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </select>
        </div>
      )}

      {/* Sample Rate */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Sample Rate
        </label>
        <select
          className="w-full rounded-md border px-2 py-1.5 text-sm"
          style={{
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            color: "var(--text-primary)",
          }}
          value={options.sampleRate ?? "original"}
          onChange={(e) =>
            setOptions({
              sampleRate: e.target.value === "original" ? null : Number(e.target.value),
            })
          }
        >
          {SAMPLE_RATES.map((s) => (
            <option key={s.label} value={s.value ?? "original"}>
              {s.label}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
