import { memo } from "react";
import { BaseEdge, getBezierPath, Position, type Edge, type EdgeProps } from "@xyflow/react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { MediaType } from "../../types/compression";

export type FlowEdgeData = { lane?: MediaType; side: "in" | "out" | "final" };
export type FlowEdgeType = Edge<FlowEdgeData, "flow">;

export const FlowEdge = memo(function FlowEdge({
  data,
  sourceX,
  sourceY,
  sourcePosition,
  targetX,
  targetY,
  targetPosition,
}: EdgeProps<FlowEdgeType>) {
  const side = data?.side;
  const lane = data?.lane;

  // One boolean per edge, so progress ticks don't re-render edges.
  const active = useCompressionStore((s) => {
    if (side === "final") return s.summary.processing > 0;
    if (!lane) return false;
    const running = s.summary.lanes[lane].processing > 0;
    // Paused: in-flight work still drains out, but nothing new enters a lane.
    return side === "in" ? running && !s.isPaused : running;
  });

  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const stateClass = active ? "is-active" : "";

  // Arrowhead drawn inline rather than as an SVG marker: React Flow markers are
  // shared <defs> with fixed inline colors, so they can't follow this edge's class.
  // Points down into a top handle (size check -> output), otherwise right.
  const pointsDown = targetPosition === Position.Top;
  const arrow = pointsDown
    ? `M ${targetX - 4},${targetY - 5} L ${targetX},${targetY} L ${targetX + 4},${targetY - 5}`
    : `M ${targetX - 5},${targetY - 4} L ${targetX},${targetY} L ${targetX - 5},${targetY + 4}`;

  return (
    <>
      <BaseEdge path={path} interactionWidth={0} className={`pipeline-edge ${stateClass}`} />
      <path d={arrow} className={`pipeline-edge-arrow ${stateClass}`} />
    </>
  );
});
