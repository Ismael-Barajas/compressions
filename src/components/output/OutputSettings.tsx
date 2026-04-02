import { FolderOpen, Check } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";

export function OutputSettings() {
  const outputDir = useCompressionStore((s) => s.outputDir);
  const sameAsSource = useCompressionStore((s) => s.sameAsSource);
  const setOutputDir = useCompressionStore((s) => s.setOutputDir);
  const setSameAsSource = useCompressionStore((s) => s.setSameAsSource);

  const handleChooseFolder = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true });
      if (selected) {
        setOutputDir(selected as string);
        setSameAsSource(false);
      }
    } catch {
      // cancelled
    }
  };

  return (
    <div className="mt-auto border-t pt-4" style={{ borderColor: "var(--border)" }}>
      <h3 className="mb-2 text-sm font-semibold" style={{ color: "var(--text-primary)" }}>
        Output
      </h3>

      {/* Same as source */}
      <label className="mb-2 flex cursor-pointer items-center gap-2">
        <div
          className="flex h-4 w-4 items-center justify-center rounded border"
          style={{
            borderColor: sameAsSource ? "var(--accent)" : "var(--border)",
            backgroundColor: sameAsSource ? "var(--accent)" : "transparent",
          }}
        >
          {sameAsSource && <Check size={12} color="white" />}
        </div>
        <span className="text-xs" style={{ color: "var(--text-secondary)" }}>
          Save next to source files
        </span>
      </label>

      {/* Custom folder picker */}
      {!sameAsSource && (
        <button
          onClick={handleChooseFolder}
          className="btn-secondary flex w-full items-center gap-2 text-xs"
        >
          <FolderOpen size={14} />
          <span className="truncate">
            {outputDir || "Choose output folder..."}
          </span>
        </button>
      )}

      <p className="mt-2 text-[10px]" style={{ color: "var(--text-muted)" }}>
        Files will be saved as <code>filename_compressed.ext</code>
      </p>
    </div>
  );
}
