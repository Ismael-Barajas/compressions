import { useState } from "react";
import { Lock, Unlock } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { ImageFormat } from "../../types/compression";
import { SectionLabel, FieldGroup, ChipButton } from "./VideoControls";

const FORMATS: { value: ImageFormat; label: string }[] = [
  { value: "Original", label: "Original" },
  { value: "Jpeg", label: "JPEG" },
  { value: "Png", label: "PNG" },
  { value: "WebP", label: "WebP" },
  { value: "Avif", label: "AVIF" },
];

export function ImageControls() {
  const options = useCompressionStore((s) => s.imageOptions);
  const setOptions = useCompressionStore((s) => s.setImageOptions);
  const [enableResize, setEnableResize] = useState(false);
  const [aspectLocked, setAspectLocked] = useState(true);
  const [fitDimension, setFitDimension] = useState<"width" | "height">("width");

  return (
    <div className="space-y-5">
      <SectionLabel>Image Settings</SectionLabel>

      {/* Format */}
      <FieldGroup label="Output Format">
        <div className="grid grid-cols-5 gap-1">
          {FORMATS.map((f) => (
            <ChipButton
              key={f.value}
              active={options.format === f.value}
              onClick={() => setOptions({ format: f.value })}
              className="!px-1 !text-[11px]"
            >
              {f.label}
            </ChipButton>
          ))}
        </div>
        {options.format === "Original" && (
          <p className="text-[11px] mt-1.5" style={{ color: "var(--text-muted)" }}>
            BMP and TIFF files will be saved as PNG since their original formats don't support compression.
          </p>
        )}
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
              if (!e.target.checked) setOptions({ resize: null, resizeMode: "fit" });
            }}
          />
          <label htmlFor="enable-resize" className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
            Resize
          </label>
        </div>
        {enableResize && (
          <div className="space-y-2">
            {/* Lock toggle */}
            <div className="flex items-center gap-2">
              <button
                onClick={() => {
                  const next = !aspectLocked;
                  setAspectLocked(next);
                  if (next) {
                    // switching to locked: reset to fit mode, clear the non-primary dimension
                    const val = fitDimension === "width"
                      ? (options.resize?.width ?? 0)
                      : (options.resize?.height ?? 0);
                    setOptions({
                      resize: fitDimension === "width" ? { width: val, height: 0 } : { width: 0, height: val },
                      resizeMode: "fit",
                    });
                  } else {
                    setOptions({ resizeMode: "exact" });
                  }
                }}
                className="flex items-center gap-1.5 text-xs transition-opacity hover:opacity-70"
                style={{ color: aspectLocked ? "var(--accent)" : "var(--text-muted)" }}
                title={aspectLocked ? "Aspect ratio locked" : "Aspect ratio unlocked"}
              >
                {aspectLocked ? <Lock size={12} /> : <Unlock size={12} />}
                <span>{aspectLocked ? "Locked" : "Free"}</span>
              </button>
            </div>

            {aspectLocked ? (
              /* Locked: pick one dimension */
              <div className="flex items-center gap-2">
                <div className="flex gap-1">
                  <ChipButton
                    active={fitDimension === "width"}
                    onClick={() => {
                      setFitDimension("width");
                      const val = options.resize?.width || options.resize?.height || 0;
                      setOptions({ resize: { width: val, height: 0 }, resizeMode: "fit" });
                    }}
                  >
                    W
                  </ChipButton>
                  <ChipButton
                    active={fitDimension === "height"}
                    onClick={() => {
                      setFitDimension("height");
                      const val = options.resize?.height || options.resize?.width || 0;
                      setOptions({ resize: { width: 0, height: val }, resizeMode: "fit" });
                    }}
                  >
                    H
                  </ChipButton>
                </div>
                <input
                  type="number"
                  placeholder={fitDimension === "width" ? "Width px" : "Height px"}
                  value={fitDimension === "width" ? (options.resize?.width || "") : (options.resize?.height || "")}
                  onChange={(e) => {
                    const val = Number(e.target.value) || 0;
                    setOptions({
                      resize: fitDimension === "width" ? { width: val, height: 0 } : { width: 0, height: val },
                      resizeMode: "fit",
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
            ) : (
              /* Unlocked: both dimensions, exact stretch */
              <div className="flex items-center gap-2">
                <input
                  type="number"
                  placeholder="Width"
                  value={options.resize?.width || ""}
                  onChange={(e) => {
                    const w = Number(e.target.value) || 0;
                    setOptions({ resize: { width: w, height: options.resize?.height ?? 0 }, resizeMode: "exact" });
                  }}
                  className="w-full border px-2 py-1.5 text-[13px]"
                  style={{
                    borderColor: "var(--border)",
                    backgroundColor: "var(--bg-primary)",
                    color: "var(--text-primary)",
                  }}
                />
                <span className="text-xs flex-shrink-0" style={{ color: "var(--text-muted)" }}>×</span>
                <input
                  type="number"
                  placeholder="Height"
                  value={options.resize?.height || ""}
                  onChange={(e) => {
                    const h = Number(e.target.value) || 0;
                    setOptions({ resize: { width: options.resize?.width ?? 0, height: h }, resizeMode: "exact" });
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
