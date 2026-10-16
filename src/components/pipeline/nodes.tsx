import { memo, useEffect, useRef, type ReactNode } from "react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import { Check, FileText, Image, Music, Video } from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useCompressionStore } from "../../stores/compressionStore";
import { formatFileSize } from "../../lib/fileUtils";
import type { MediaType } from "../../types/compression";

export type LaneData = { mediaType: MediaType; title: string; engine: string; emptyText: string };
export type LaneNodeType = Node<LaneData, "lane">;

// Handles only anchor edges; they are invisible and never connectable.
function InHandle({ position = Position.Left }: { position?: Position }) {
  return <Handle type="target" position={position} isConnectable={false} className="pipeline-handle" />;
}

function OutHandle({ position = Position.Right }: { position?: Position }) {
  return <Handle type="source" position={position} isConnectable={false} className="pipeline-handle" />;
}

function NodeLabel({ children }: { children: string }) {
  return <div className="pipeline-label truncate">{children}</div>;
}

export const QueueNode = memo(function QueueNode() {
  const queued = useCompressionStore((s) => s.summary.queued);
  // isPaused stays set if Pause lands just as the batch finishes; only show it mid-run.
  const isPaused = useCompressionStore((s) => s.isPaused && s.isCompressing);

  return (
    <div className="pipeline-node">
      <NodeLabel>Queue</NodeLabel>
      <div className={`pipeline-value font-data ${queued === 0 ? "is-muted" : ""}`}>{queued}</div>
      {isPaused ? (
        <span className="pipeline-chip">Paused</span>
      ) : (
        <div className="pipeline-caption truncate">waiting</div>
      )}
      <OutHandle />
    </div>
  );
});

const LANE_ICONS = { video: Video, image: Image, pdf: FileText, audio: Music };

export const LaneNode = memo(function LaneNode({ data }: NodeProps<LaneNodeType>) {
  const { mediaType, title, engine, emptyText } = data;
  const lane = useCompressionStore((s) => s.summary.lanes[mediaType]);
  const { queued, processing, complete, error, progressSum } = lane;

  const done = complete + error;
  const total = queued + processing + done;
  const isEmpty = total === 0;
  const isRunning = processing > 0;
  const isDone = !isEmpty && queued === 0 && !isRunning;
  const fraction = isEmpty ? 0 : (done + progressSum / 100) / total;
  const Icon = LANE_ICONS[mediaType];

  const stateClass = isEmpty ? "is-empty" : isRunning ? "is-running" : isDone ? "is-done" : "";

  let status: ReactNode;
  if (isEmpty) {
    status = <span className="truncate">{emptyText}</span>;
  } else if (isRunning && mediaType === "video") {
    status = <CurrentVideo fallback={`${processing} running`} />;
  } else if (isRunning) {
    status = <span className="truncate">{processing} running</span>;
  } else if (isDone) {
    status = (
      <span className="flex min-w-0 items-center gap-1">
        <Check size={12} strokeWidth={2.5} className="shrink-0" style={{ color: "var(--success)" }} />
        <span className="truncate">Done</span>
      </span>
    );
  } else {
    status = <span className="truncate">{queued} waiting</span>;
  }

  return (
    <div className={`pipeline-node ${stateClass}`}>
      <InHandle />
      <div className="flex items-center gap-1.5">
        <Icon size={13} className="shrink-0" style={{ color: "var(--text-muted)" }} />
        <span className="pipeline-title min-w-0 flex-1 truncate">{title}</span>
        {!isEmpty && (
          <span className="font-data shrink-0" style={{ color: "var(--text-secondary)" }}>
            {done}/{total}
          </span>
        )}
      </div>
      <div className="pipeline-caption mt-0.5 truncate">{engine}</div>
      <div className="pipeline-status mt-auto flex items-center">
        <div className="flex min-w-0 flex-1 items-center">{status}</div>
        {error > 0 && (
          <span className="font-data ml-2 shrink-0" style={{ color: "var(--error)" }}>
            {error} failed
          </span>
        )}
      </div>
      <ProgressBar fraction={fraction} success={isDone && error === 0} />
      <OutHandle />
    </div>
  );
});

/** Name and percent of the running video. Returns primitives so unrelated ticks don't re-render. */
function CurrentVideo({ fallback }: { fallback: string }) {
  const current = useCompressionStore(
    useShallow((s) => {
      const file = s.files.find((f) => f.mediaType === "video" && f.status === "processing");
      return { name: file?.name ?? null, percent: file ? Math.round(file.progress) : 0 };
    }),
  );

  if (current.name === null) return <span className="truncate">{fallback}</span>;
  return (
    <>
      <span className="min-w-0 truncate" title={current.name}>
        {current.name}
      </span>
      <span className="font-data ml-1.5 shrink-0" style={{ color: "var(--text-secondary)" }}>
        {current.percent}%
      </span>
    </>
  );
}

/**
 * Thin lane progress bar. It only animates when the value grows: drops (Cancel All
 * resets, a hardware-encoder fallback restarting a file at 0%, files added mid-run)
 * snap instantly instead of visibly sliding backwards.
 */
const ProgressBar = memo(function ProgressBar({ fraction, success }: { fraction: number; success: boolean }) {
  const previous = useRef(fraction);
  const grew = fraction > previous.current;
  useEffect(() => {
    previous.current = fraction;
  }, [fraction]);

  const clamped = Math.min(1, Math.max(0, fraction));
  return (
    <div className="pipeline-bar mt-1">
      <div
        className={`pipeline-bar-fill ${grew ? "is-growing" : ""} ${success ? "is-success" : ""}`}
        style={{ transform: `scaleX(${clamped})` }}
      />
    </div>
  );
});

export const CheckNode = memo(function CheckNode() {
  const keptOriginal = useCompressionStore((s) => s.summary.keptOriginal);

  return (
    <div className="pipeline-node">
      <InHandle />
      <NodeLabel>Size check</NodeLabel>
      <div className={`pipeline-value font-data ${keptOriginal === 0 ? "is-muted" : ""}`}>{keptOriginal}</div>
      <div className="pipeline-caption truncate">kept original</div>
      {/* Output sits directly below in the same column */}
      <OutHandle position={Position.Bottom} />
    </div>
  );
});

export const OutputNode = memo(function OutputNode() {
  const { files, savedIn, savedOut } = useCompressionStore(
    useShallow((s) => {
      const l = s.summary.lanes;
      return {
        files: l.video.complete + l.image.complete + l.pdf.complete + l.audio.complete,
        savedIn: s.summary.savedIn,
        savedOut: s.summary.savedOut,
      };
    }),
  );

  const saved = Math.max(0, savedIn - savedOut);
  const percent = savedIn > 0 ? Math.round((saved / savedIn) * 100) : 0;

  return (
    <div className="pipeline-node">
      <InHandle position={Position.Top} />
      <NodeLabel>Output</NodeLabel>
      <div className={`pipeline-value font-data truncate ${saved > 0 ? "is-success" : "is-muted"}`}>
        {files > 0 ? formatFileSize(saved) : "—"}
      </div>
      <div className="pipeline-caption flex items-center">
        <span className="min-w-0 flex-1 truncate">
          {files} {files === 1 ? "file" : "files"}
        </span>
        {percent > 0 && (
          <span className="font-data ml-1.5 shrink-0" style={{ color: "var(--success)" }}>
            −{percent}%
          </span>
        )}
      </div>
    </div>
  );
});
