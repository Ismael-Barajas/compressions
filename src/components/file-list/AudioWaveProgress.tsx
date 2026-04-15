const BAR_COUNT = 24;

export function AudioWaveProgress({ progress }: { progress: number }) {
  const activeCount = progress > 0 ? Math.ceil((progress / 100) * BAR_COUNT) : 0;
  const isIndeterminate = progress <= 0;

  return (
    <div className="mt-2.5">
      <div className="flex items-end gap-[2px]" style={{ height: 14 }}>
        {Array.from({ length: BAR_COUNT }, (_, i) => {
          const isActive = isIndeterminate || i < activeCount;
          return (
            <span
              key={i}
              className="audio-wave-bar flex-1"
              style={{
                backgroundColor: isActive ? "var(--accent)" : "var(--bg-tertiary)",
                boxShadow: isActive ? "0 0 6px var(--accent-glow)" : "none",
                animationDelay: `${i * 0.07}s`,
              }}
            />
          );
        })}
      </div>
      <span className="font-data mt-1 block text-right" style={{ color: "var(--text-muted)" }}>
        {progress > 0 ? `${Math.round(progress)}%` : "Processing\u2026"}
      </span>
    </div>
  );
}
