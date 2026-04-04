import { FolderOpen, Info } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { OutputMode } from "../../types/compression";

const OUTPUT_MODES: { value: OutputMode; label: string }[] = [
  { value: "sameDir", label: "Same directory" },
  { value: "subfolder", label: "Subfolder" },
  { value: "customDir", label: "Custom folder" },
];

export function OutputSettings() {
  const outputDir = useCompressionStore((s) => s.outputDir);
  const outputMode = useCompressionStore((s) => s.outputMode);
  const subfolderName = useCompressionStore((s) => s.subfolderName);
  const outputTemplate = useCompressionStore((s) => s.outputTemplate);
  const setOutputDir = useCompressionStore((s) => s.setOutputDir);
  const setOutputMode = useCompressionStore((s) => s.setOutputMode);
  const setSubfolderName = useCompressionStore((s) => s.setSubfolderName);
  const setOutputTemplate = useCompressionStore((s) => s.setOutputTemplate);

  const handleChooseFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true });
      if (selected) {
        setOutputDir(selected as string);
        setOutputMode("customDir");
      }
    } catch {
      // cancelled
    }
  };

  const previewName = outputTemplate
    .replace(/\{name\}/g, "photo")
    .replace(/\{date\}/g, "2026-04-03")
    .replace(/\{time\}/g, "14-30-00") + ".jpg";

  return (
    <div className="mt-auto border-t pt-4" style={{ borderColor: "var(--border)" }}>
      <h3 className="mb-2 text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        Output
      </h3>

      {/* Output mode radios */}
      <div className="mb-3 space-y-1.5">
        {OUTPUT_MODES.map((mode) => (
          <label key={mode.value} className="flex cursor-pointer items-center gap-2">
            <div
              className="flex h-4 w-4 items-center justify-center rounded-full border"
              style={{
                borderColor: outputMode === mode.value ? "var(--accent)" : "var(--border)",
              }}
            >
              {outputMode === mode.value && (
                <div
                  className="h-2 w-2 rounded-full"
                  style={{ backgroundColor: "var(--accent)" }}
                />
              )}
            </div>
            <span className="text-xs" style={{ color: "var(--text-secondary)" }}>
              {mode.label}
            </span>
          </label>
        ))}
      </div>

      {/* Subfolder name input */}
      {outputMode === "subfolder" && (
        <div className="mb-2">
          <input
            type="text"
            value={subfolderName}
            onChange={(e) => setSubfolderName(e.target.value)}
            placeholder="compressed"
            className="w-full rounded border px-2 py-1 text-xs"
            style={{
              borderColor: "var(--border)",
              backgroundColor: "var(--bg-tertiary)",
              color: "var(--text-primary)",
            }}
          />
          <p className="mt-0.5 text-[10px]" style={{ color: "var(--text-muted)" }}>
            Created inside each source file's directory
          </p>
        </div>
      )}

      {/* Custom folder picker */}
      {outputMode === "customDir" && (
        <button
          onClick={handleChooseFolder}
          className="btn-secondary mb-2 flex w-full items-center gap-2 text-xs"
        >
          <FolderOpen size={14} />
          <span className="truncate">
            {outputDir || "Choose output folder..."}
          </span>
        </button>
      )}

      {/* Output name template */}
      <div className="mt-2 border-t pt-2" style={{ borderColor: "var(--border)" }}>
        <div className="mb-1 flex items-center gap-1">
          <span className="text-[10px] font-medium" style={{ color: "var(--text-secondary)" }}>
            File name format
          </span>
          <span
            title="Tokens: {name} = original name, {date} = YYYY-MM-DD, {time} = HH-MM-SS"
            className="cursor-help"
          >
            <Info size={10} style={{ color: "var(--text-muted)" }} />
          </span>
        </div>
        <input
          type="text"
          value={outputTemplate}
          onChange={(e) => setOutputTemplate(e.target.value)}
          placeholder="{name}_compressed"
          className="w-full rounded border px-2 py-1 text-xs font-mono"
          style={{
            borderColor: "var(--border)",
            backgroundColor: "var(--bg-tertiary)",
            color: "var(--text-primary)",
          }}
        />
        <p className="mt-0.5 text-[10px]" style={{ color: "var(--text-muted)" }}>
          Preview: <code>{previewName}</code>
        </p>
      </div>
    </div>
  );
}
