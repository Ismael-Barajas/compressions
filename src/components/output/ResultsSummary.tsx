import { useCompressionStore } from "../../stores/compressionStore";
import { formatFileSize, getSavingsPercent } from "../../lib/fileUtils";

export function ResultsSummary() {
  const files = useCompressionStore((s) => s.files);
  const completedFiles = files.filter((f) => f.status === "complete" && f.result?.success);
  const errorFiles = files.filter((f) => f.status === "error");

  if (completedFiles.length === 0) return null;

  const totalInput = completedFiles.reduce((sum, f) => sum + (f.result?.inputSize ?? 0), 0);
  const totalOutput = completedFiles.reduce((sum, f) => sum + (f.result?.outputSize ?? 0), 0);
  const totalSavings = getSavingsPercent(totalInput, totalOutput);

  return (
    <div
      className="mt-4 border p-4"
      style={{
        borderColor: "var(--border)",
        backgroundColor: "var(--bg-secondary)",
        borderLeft: "2px solid var(--success)",
      }}
    >
      <h3
        className="mb-3 text-[11px] font-semibold uppercase tracking-widest"
        style={{ color: "var(--text-muted)" }}
      >
        Results
      </h3>

      <div className="grid grid-cols-3 gap-4 text-center">
        <div>
          <div className="font-data text-lg font-semibold" style={{ color: "var(--text-primary)" }}>
            {completedFiles.length}
          </div>
          <div className="text-[10px] uppercase tracking-wider" style={{ color: "var(--text-muted)" }}>
            Done
          </div>
        </div>
        <div>
          <div className="font-data text-lg font-semibold" style={{ color: "var(--success)" }}>
            {totalSavings}%
          </div>
          <div className="text-[10px] uppercase tracking-wider" style={{ color: "var(--text-muted)" }}>
            Saved
          </div>
        </div>
        <div>
          <div className="font-data text-lg font-semibold" style={{ color: "var(--text-primary)" }}>
            {formatFileSize(totalInput - totalOutput)}
          </div>
          <div className="text-[10px] uppercase tracking-wider" style={{ color: "var(--text-muted)" }}>
            Reduced
          </div>
        </div>
      </div>

      {totalInput > 0 && (
        <div className="font-data mt-3 text-center" style={{ color: "var(--text-secondary)" }}>
          {formatFileSize(totalInput)} → {formatFileSize(totalOutput)}
        </div>
      )}

      {errorFiles.length > 0 && (
        <div className="font-data mt-2 text-center" style={{ color: "var(--error)" }}>
          {errorFiles.length} file{errorFiles.length !== 1 ? "s" : ""} failed
        </div>
      )}
    </div>
  );
}
