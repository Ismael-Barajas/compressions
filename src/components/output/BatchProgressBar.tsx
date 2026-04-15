import { useCompressionStore } from "../../stores/compressionStore";

export function BatchProgressBar() {
  const files = useCompressionStore((s) => s.files);

  const totalFiles = files.length;
  const completedCount = files.filter(
    (f) => f.status === "complete" || f.status === "error",
  ).length;
  const processingProgress = files
    .filter((f) => f.status === "processing")
    .reduce((sum, f) => sum + f.progress, 0);
  const totalProgress =
    totalFiles > 0 ? (completedCount * 100 + processingProgress) / totalFiles : 0;

  return (
    <div
      className="mt-4 border p-4"
      style={{
        borderColor: "var(--border)",
        backgroundColor: "var(--bg-secondary)",
        borderLeft: "2px solid var(--accent)",
      }}
    >
      <h3
        className="mb-3 text-[11px] font-semibold uppercase tracking-widest"
        style={{ color: "var(--text-muted)" }}
      >
        Progress
      </h3>

      {/* Progress bar */}
      <div
        className="mb-3 overflow-hidden"
        style={{ height: 4, backgroundColor: "var(--bg-tertiary)" }}
      >
        <div
          style={{
            height: "100%",
            width: `${totalProgress}%`,
            backgroundColor: "var(--accent)",
            transition: "width 0.3s ease",
          }}
        />
      </div>

      <div
        className="font-data text-center text-[13px]"
        style={{ color: "var(--text-secondary)" }}
      >
        Compressing {completedCount}/{totalFiles} files — {totalProgress.toFixed(1)}%
      </div>
    </div>
  );
}
