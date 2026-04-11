import { useCompressionStore } from "../../stores/compressionStore";
import type { DitherMode } from "../../types/compression";
import { SectionLabel, FieldGroup, ChipButton, SelectInput } from "./VideoControls";

const DITHER_MODES: { value: DitherMode; label: string }[] = [
  { value: "floyd_steinberg", label: "Floyd-Steinberg" },
  { value: "bayer", label: "Bayer" },
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
    <div className="space-y-5">
      <SectionLabel>GIF Conversion</SectionLabel>

      {/* FPS */}
      <FieldGroup label="Frame Rate" trailing={<span className="font-data">{options.fps} fps</span>}>
        <input
          type="range"
          min={5}
          max={30}
          step={1}
          value={options.fps}
          onChange={(e) => setOptions({ fps: Number(e.target.value) })}
          className="w-full"
        />
      </FieldGroup>

      {/* Max Width */}
      <FieldGroup label="Max Width">
        <SelectInput
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
        </SelectInput>
      </FieldGroup>

      {/* Max Colors */}
      <FieldGroup label="Colors" trailing={<span className="font-data">{options.maxColors}</span>}>
        <input
          type="range"
          min={16}
          max={256}
          step={16}
          value={options.maxColors}
          onChange={(e) => setOptions({ maxColors: Number(e.target.value) })}
          className="w-full"
        />
      </FieldGroup>

      {/* Dither Mode */}
      <FieldGroup label="Dither">
        <div className="grid grid-cols-3 gap-1.5">
          {DITHER_MODES.map((d) => (
            <ChipButton
              key={d.value}
              active={options.dither === d.value}
              onClick={() => setOptions({ dither: d.value })}
            >
              {d.label}
            </ChipButton>
          ))}
        </div>
      </FieldGroup>
    </div>
  );
}
