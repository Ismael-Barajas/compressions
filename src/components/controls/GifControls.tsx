import { useCompressionStore } from "../../stores/compressionStore";
import type { DitherMode } from "../../types/compression";

const DITHER_MODES: { value: DitherMode; label: string }[] = [
  { value: "floyd_steinberg", label: "Floyd-Steinberg" },
  { value: "bayer", label: "Bayer (ordered)" },
  { value: "none", label: "None" },
];

const WIDTH_PRESETS = [
  { label: "Original", value: null },
  { label: "640px", value: 640 },
  { label: "480px", value: 480 },
  { label: "320px", value: 320 },
  { label: "240px", value: 240 },
];

export function GifControls() {
  const options = useCompressionStore((s) => s.gifOptions);
  const setOptions = useCompressionStore((s) => s.setGifOptions);

  return (
    <div className="space-y-4">
      <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        GIF Conversion
      </h3>

      {/* FPS */}
      <div>
        <label className="mb-1.5 flex items-center justify-between text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          <span>Frame Rate</span>
          <span style={{ color: "var(--text-muted)" }}>{options.fps} fps</span>
        </label>
        <input
          type="range"
          min={5}
          max={30}
          step={1}
          value={options.fps}
          onChange={(e) => setOptions({ fps: Number(e.target.value) })}
          className="w-full"
          style={{ accentColor: "var(--accent)" }}
        />
      </div>

      {/* Max Width */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Max Width
        </label>
        <select
          className="w-full rounded-md border px-2 py-1.5 text-sm"
          style={{
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            color: "var(--text-primary)",
          }}
          value={options.width ?? "original"}
          onChange={(e) =>
            setOptions({
              width: e.target.value === "original" ? null : Number(e.target.value),
            })
          }
        >
          {WIDTH_PRESETS.map((p) => (
            <option key={p.label} value={p.value ?? "original"}>
              {p.label}
            </option>
          ))}
        </select>
      </div>

      {/* Max Colors */}
      <div>
        <label className="mb-1.5 flex items-center justify-between text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          <span>Colors</span>
          <span style={{ color: "var(--text-muted)" }}>{options.maxColors}</span>
        </label>
        <input
          type="range"
          min={16}
          max={256}
          step={16}
          value={options.maxColors}
          onChange={(e) => setOptions({ maxColors: Number(e.target.value) })}
          className="w-full"
          style={{ accentColor: "var(--accent)" }}
        />
      </div>

      {/* Dither Mode */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Dither
        </label>
        <div className="grid grid-cols-3 gap-1.5">
          {DITHER_MODES.map((d) => (
            <button
              key={d.value}
              onClick={() => setOptions({ dither: d.value })}
              className="rounded-md border px-2 py-1.5 text-center text-xs transition-colors"
              style={{
                borderColor: options.dither === d.value ? "var(--accent)" : "var(--border)",
                backgroundColor: options.dither === d.value ? "color-mix(in srgb, var(--accent) 10%, transparent)" : "transparent",
                color: options.dither === d.value ? "var(--accent)" : "var(--text-secondary)",
              }}
            >
              {d.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
