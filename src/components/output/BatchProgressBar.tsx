import { useShallow } from "zustand/react/shallow";
import { useCompressionStore } from "../../stores/compressionStore";

export function BatchProgressBar() {
  // Subscribe to three numbers instead of the whole files array: this component
  // re-renders on every progress tick, so the derivation must be O(1).
  const { totalFiles, completedCount, processingProgress } = useCompressionStore(
    useShallow((s) => ({
      totalFiles: s.summary.total,
      completedCount: s.summary.complete + s.summary.error,
      processingProgress: s.summary.progressSum,
    })),
  );

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
