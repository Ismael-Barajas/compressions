import { useState } from "react";
import { Link, Unlink } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { ImageFormat } from "../../types/compression";

const FORMATS: { value: ImageFormat; label: string; description: string }[] = [
  { value: "Jpeg", label: "JPEG", description: "Best compatibility" },
  { value: "Png", label: "PNG", description: "Lossless" },
  { value: "WebP", label: "WebP", description: "Modern, great compression" },
  { value: "Avif", label: "AVIF", description: "Best compression" },
];

export function ImageControls() {
  const options = useCompressionStore((s) => s.imageOptions);
  const setOptions = useCompressionStore((s) => s.setImageOptions);
  const [aspectLocked, setAspectLocked] = useState(true);
  const [enableResize, setEnableResize] = useState(false);

  return (
    <div className="space-y-4">
      <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        Image Settings
      </h3>

      {/* Format */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Output Format
        </label>
        <div className="grid grid-cols-4 gap-1.5">
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
              {f.label}
            </button>
          ))}
        </div>
      </div>

      {/* Quality */}
      {options.format !== "Png" && (
        <div>
          <div className="mb-1.5 flex items-center justify-between">
            <label className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
              Quality
            </label>
            <span className="font-mono text-xs" style={{ color: "var(--text-muted)" }}>
              {options.quality}
            </span>
          </div>
          <input
            type="range"
            min={1}
            max={100}
            value={options.quality}
            onChange={(e) => setOptions({ quality: Number(e.target.value) })}
            className="w-full accent-[var(--accent)]"
          />
          <div className="mt-0.5 flex justify-between text-[10px]" style={{ color: "var(--text-muted)" }}>
            <span>Smaller file</span>
            <span>Higher quality</span>
          </div>
        </div>
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
            className="accent-[var(--accent)]"
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
              className="w-full rounded-md border px-2 py-1.5 text-sm"
              style={{
                borderColor: "var(--border)",
                backgroundColor: "var(--bg-primary)",
                color: "var(--text-primary)",
              }}
            />
            <button
              onClick={() => setAspectLocked(!aspectLocked)}
              className="rounded p-1"
              title={aspectLocked ? "Unlock aspect ratio" : "Lock aspect ratio"}
            >
              {aspectLocked ? (
                <Link size={14} style={{ color: "var(--accent)" }} />
              ) : (
                <Unlink size={14} style={{ color: "var(--text-muted)" }} />
              )}
            </button>
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
              className="w-full rounded-md border px-2 py-1.5 text-sm"
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
          className="accent-[var(--accent)]"
        />
        <label htmlFor="strip-metadata" className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Strip metadata (EXIF, XMP)
        </label>
      </div>
    </div>
  );
}
