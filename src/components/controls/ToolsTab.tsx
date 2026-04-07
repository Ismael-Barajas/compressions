import { AudioControls } from "./AudioControls";
import { GifControls } from "./GifControls";

export function ToolsTab() {
  return (
    <div className="flex flex-col gap-4">
      <p className="text-xs" style={{ color: "var(--text-muted)" }}>
        Right-click any video file to use these tools.
      </p>
      <AudioControls />
      <div className="border-t" style={{ borderColor: "var(--border)" }} />
      <GifControls />
    </div>
  );
}
