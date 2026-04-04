import { useCompressionStore } from "../../stores/compressionStore";
import type { PdfQuality } from "../../types/compression";

const QUALITY_PRESETS: { value: PdfQuality; label: string; description: string }[] = [
  { value: "screen", label: "Screen", description: "72 DPI — smallest size" },
  { value: "ebook", label: "Ebook", description: "150 DPI — good balance" },
  { value: "printer", label: "Printer", description: "300 DPI — high quality" },
  { value: "prepress", label: "Prepress", description: "300 DPI — color preserving" },
];

const DPI_OPTIONS = [
  { label: "Default", value: null },
  { label: "72 DPI", value: 72 },
  { label: "150 DPI", value: 150 },
  { label: "200 DPI", value: 200 },
  { label: "300 DPI", value: 300 },
];

export function PdfControls() {
  const options = useCompressionStore((s) => s.pdfOptions);
  const setOptions = useCompressionStore((s) => s.setPdfOptions);

  return (
    <div className="space-y-4">
      <h3 className="text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        PDF Compression
      </h3>

      {/* Quality preset */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Quality
        </label>
        <div className="grid grid-cols-2 gap-1.5">
          {QUALITY_PRESETS.map((p) => (
            <button
              key={p.value}
              onClick={() => setOptions({ quality: p.value })}
              className="rounded-md border px-2 py-1.5 text-left text-xs transition-colors"
              style={{
                borderColor: options.quality === p.value ? "var(--accent)" : "var(--border)",
                backgroundColor: options.quality === p.value ? "color-mix(in srgb, var(--accent) 10%, transparent)" : "transparent",
                color: options.quality === p.value ? "var(--accent)" : "var(--text-secondary)",
              }}
            >
              <div className="font-medium">{p.label}</div>
              <div className="mt-0.5 opacity-70">{p.description}</div>
            </button>
          ))}
        </div>
      </div>

      {/* DPI override */}
      <div>
        <label className="mb-1.5 block text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          Image DPI Override
        </label>
        <select
          className="w-full rounded-md border px-2 py-1.5 text-sm"
          style={{
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-primary)",
            color: "var(--text-primary)",
          }}
          value={options.dpi ?? "default"}
          onChange={(e) =>
            setOptions({
              dpi: e.target.value === "default" ? null : Number(e.target.value),
            })
          }
        >
          {DPI_OPTIONS.map((d) => (
            <option key={d.label} value={d.value ?? "default"}>
              {d.label}
            </option>
          ))}
        </select>
      </div>
    </div>
  );
}
