import { memo } from "react";
import { BaseEdge, getBezierPath, Position, type Edge, type EdgeProps } from "@xyflow/react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { MediaType } from "../../types/compression";

export type FlowEdgeData = { lane?: MediaType; side: "in" | "out" | "final" };
export type FlowEdgeType = Edge<FlowEdgeData, "flow">;

const NOTCH_HALF_LENGTH = 5;

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

  // No arrowhead: the layout already shows the direction (left to right, then down
  // into output), and heads cluttered the spots where several lines meet a node.
  // Instead a short thick notch sits on the node's edge where the line lands,
  // across the line: upright on a left handle, flat on a top handle.
  const intoTop = targetPosition === Position.Top;
  const notch = intoTop
    ? `M ${targetX - NOTCH_HALF_LENGTH},${targetY} L ${targetX + NOTCH_HALF_LENGTH},${targetY}`
    : `M ${targetX},${targetY - NOTCH_HALF_LENGTH} L ${targetX},${targetY + NOTCH_HALF_LENGTH}`;

  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  const stateClass = active ? "is-active" : "";

  return (
    <>
      <BaseEdge path={path} interactionWidth={0} className={`pipeline-edge ${stateClass}`} />
      <path d={notch} className={`pipeline-edge-notch ${stateClass}`} />
    </>
  );
});
