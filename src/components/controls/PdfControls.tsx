import { useCompressionStore } from "../../stores/compressionStore";
import type { PdfQuality } from "../../types/compression";
import { SectionLabel, FieldGroup, SelectInput } from "./VideoControls";

const QUALITY_PRESETS: { value: PdfQuality; label: string; description: string }[] = [
  { value: "screen", label: "Screen", description: "72 DPI — smallest" },
  { value: "ebook", label: "Ebook", description: "150 DPI — balanced" },
  { value: "printer", label: "Printer", description: "300 DPI — high" },
  { value: "prepress", label: "Prepress", description: "300 DPI — color" },
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
    <div className="space-y-5">
      <SectionLabel>PDF Compression</SectionLabel>

      {/* Quality preset */}
      <FieldGroup label="Quality">
        <div className="grid grid-cols-2 gap-1.5">
          {QUALITY_PRESETS.map((p) => {
            const isActive = options.quality === p.value;
            return (
              <button
                key={p.value}
                onClick={() => setOptions({ quality: p.value })}
                className="px-2 py-2 text-left text-xs transition-all duration-100"
                style={{
                  border: `1px solid ${isActive ? "var(--accent)" : "var(--border)"}`,
                  backgroundColor: isActive ? "var(--accent-glow)" : "transparent",
                  color: isActive ? "var(--accent)" : "var(--text-secondary)",
                }}
              >
                <div className="font-medium">{p.label}</div>
                <div className="mt-0.5 opacity-60" style={{ fontSize: "10px" }}>{p.description}</div>
              </button>
            );
          })}
        </div>
      </FieldGroup>

      {/* DPI override */}
      <FieldGroup label="Image DPI Override">
        <SelectInput
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
        </SelectInput>
      </FieldGroup>
    </div>
  );
}
