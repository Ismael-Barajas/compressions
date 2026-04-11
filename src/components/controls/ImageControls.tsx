import { useState } from "react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { ImageFormat } from "../../types/compression";
import { SectionLabel, FieldGroup, ChipButton } from "./VideoControls";

const FORMATS: { value: ImageFormat; label: string }[] = [
  { value: "Jpeg", label: "JPEG" },
  { value: "Png", label: "PNG" },
  { value: "WebP", label: "WebP" },
  { value: "Avif", label: "AVIF" },
];

export function ImageControls() {
  const options = useCompressionStore((s) => s.imageOptions);
  const setOptions = useCompressionStore((s) => s.setImageOptions);
  const [enableResize, setEnableResize] = useState(false);

  return (
    <div className="space-y-5">
      <SectionLabel>Image Settings</SectionLabel>

      {/* Format */}
      <FieldGroup label="Output Format">
        <div className="grid grid-cols-4 gap-1.5">
          {FORMATS.map((f) => (
            <ChipButton
              key={f.value}
              active={options.format === f.value}
              onClick={() => setOptions({ format: f.value })}
            >
              {f.label}
            </ChipButton>
          ))}
        </div>
      </FieldGroup>

      {/* Quality */}
      {options.format !== "Png" && (
        <FieldGroup label="Quality" trailing={<span className="font-data">{options.quality}</span>}>
          <input
            type="range"
            min={1}
            max={100}
            value={options.quality}
            onChange={(e) => setOptions({ quality: Number(e.target.value) })}
            className="w-full"
          />
          <div className="mt-1 flex justify-between text-[10px]" style={{ color: "var(--text-muted)" }}>
            <span>Smaller file</span>
            <span>Higher quality</span>
          </div>
        </FieldGroup>
      )}

      {/* Resize */}
      <div>
        <div className="mb-1.5 flex items-center gap-2">
          <input
            type="checkbox"
            id="enable-resize"
            checked={enableResize}
            onChange={(e) => {
              setEnableResize(e.target.checked);
              if (!e.target.checked) setOptions({ resize: null });
            }}
          />
          <label htmlFor="enable-resize" className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
            Resize
          </label>
        </div>
        {enableResize && (
          <div className="flex items-center gap-2">
            <input
              type="number"
              placeholder="Width"
              value={options.resize?.width ?? ""}
              onChange={(e) => {
                const w = Number(e.target.value) || 0;
                setOptions({
                  resize: { width: w, height: options.resize?.height ?? 0 },
                });
              }}
              className="w-full border px-2 py-1.5 text-[13px]"
              style={{
                borderColor: "var(--border)",
                backgroundColor: "var(--bg-primary)",
                color: "var(--text-primary)",
              }}
            />
            <span className="text-xs" style={{ color: "var(--text-muted)" }}>x</span>
            <input
              type="number"
              placeholder="Height"
              value={options.resize?.height ?? ""}
              onChange={(e) => {
                const h = Number(e.target.value) || 0;
                setOptions({
                  resize: { width: options.resize?.width ?? 0, height: h },
                });
              }}
              className="w-full border px-2 py-1.5 text-[13px]"
              style={{
                borderColor: "var(--border)",
                backgroundColor: "var(--bg-primary)",
                color: "var(--text-primary)",
              }}
            />
          </div>
        )}
      </div>

      {/* Strip metadata */}
      <div className="flex items-center gap-2">
        <input
          type="checkbox"
          id="strip-metadata"
          checked={options.stripMetadata}
          onChange={(e) => setOptions({ stripMetadata: e.target.checked })}
        />
        <label htmlFor="strip-metadata" className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Strip metadata (EXIF, XMP)
        </label>
      </div>
    </div>
  );
}
