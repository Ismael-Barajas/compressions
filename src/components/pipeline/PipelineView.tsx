import { useEffect, useId } from "react";
import {
  Background,
  BackgroundVariant,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  useStore,
  type Edge,
  type Node,
} from "@xyflow/react";
import "@xyflow/react/dist/base.css";
import "./pipeline.css";
import { CheckNode, LaneNode, OutputNode, QueueNode, type LaneData } from "./nodes";
import { FlowEdge, type FlowEdgeData } from "./FlowEdge";
import type { MediaType } from "../../types/compression";

// Fixed layout (flow units). Sizes are set on the nodes so React Flow never has to
// measure them, and match the CSS (.pipeline-node fills the node box).
//
// The graph is kept compact (436 x 288) so it renders near 1:1 even in the smallest
// panel: at the 800x600 minimum window the canvas is ~448 x 300 while compressing,
// and fitting there must keep the 11px node text at >= 10px on screen. That is why
// Size check and Output share the last column (check above, output below) instead of
// adding a fourth column.
const LANE_H = 66;
const LANE_GAP = 8;
const LANES_H = 4 * LANE_H + 3 * LANE_GAP; // 288
const LANE_X = 120;
const LANE_W = 160;
const NODE_H = 72; // queue, size check, output
const END_X = 320; // size check + output column
const END_W = 116;
const END_GAP = 48; // vertical room for the check -> output arrow
const END_Y = (LANES_H - (2 * NODE_H + END_GAP)) / 2; // the pair is centered on the lanes

const LANES: { mediaType: MediaType; title: string; engine: string; emptyText: string }[] = [
  { mediaType: "video", title: "Video", engine: "FFmpeg · one at a time", emptyText: "No videos" },
  { mediaType: "image", title: "Image", engine: "Native · parallel", emptyText: "No images" },
  { mediaType: "pdf", title: "PDF", engine: "Ghostscript · parallel", emptyText: "No PDFs" },
  { mediaType: "audio", title: "Audio", engine: "FFmpeg · parallel", emptyText: "No audio" },
];

const nodes: Node[] = [
  { id: "queue", type: "queue", position: { x: 0, y: (LANES_H - NODE_H) / 2 }, width: 80, height: NODE_H, data: {} },
  ...LANES.map(
    (lane, i): Node<LaneData> => ({
      id: lane.mediaType,
      type: "lane",
      position: { x: LANE_X, y: i * (LANE_H + LANE_GAP) },
      width: LANE_W,
      height: LANE_H,
      data: lane,
    }),
  ),
  { id: "check", type: "check", position: { x: END_X, y: END_Y }, width: END_W, height: NODE_H, data: {} },
  {
    id: "output",
    type: "result",
    position: { x: END_X, y: END_Y + NODE_H + END_GAP },
    width: END_W,
    height: NODE_H,
    data: {},
  },
];

const edges: Edge<FlowEdgeData>[] = [
  ...LANES.flatMap(({ mediaType }): Edge<FlowEdgeData>[] => [
    { id: `queue-${mediaType}`, type: "flow", source: "queue", target: mediaType, data: { lane: mediaType, side: "in" } },
    { id: `${mediaType}-check`, type: "flow", source: mediaType, target: "check", data: { lane: mediaType, side: "out" } },
  ]),
  { id: "check-output", type: "flow", source: "check", target: "output", data: { side: "final" } },
];

// "result", not "output": React Flow styles its built-in input/output/default/group types.
const nodeTypes = { queue: QueueNode, lane: LaneNode, check: CheckNode, result: OutputNode };
const edgeTypes = { flow: FlowEdge };

// Pixel padding (not a fraction) so small panels don't lose extra room to it.
const FIT_VIEW_OPTIONS = { padding: "12px", duration: 0 } as const;
const PRO_OPTIONS = { hideAttribution: true };

/** Refits the graph whenever the panel resizes (sidebar toggle, window resize, drag-over banner). */
function AutoFit() {
  const { fitView } = useReactFlow();
  const domNode = useStore((s) => s.domNode);

  useEffect(() => {
    if (!domNode) return;
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => void fitView(FIT_VIEW_OPTIONS));
    });
    observer.observe(domNode);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [domNode, fitView]);

  return null;
}

export default function PipelineView() {
  const descriptionId = useId();

  return (
    <div className="relative min-h-0 flex-1" role="region" aria-label="Compression pipeline" aria-describedby={descriptionId}>
      <p id={descriptionId} className="sr-only">
        Queued files flow into one lane per media type. Videos encode one at a time with FFmpeg; images encode in
        parallel natively; PDFs in parallel with Ghostscript; audio in parallel with FFmpeg. Every result then passes a
        size check that keeps the original when the output isn't smaller, before it is written to the output folder.
      </p>
      <ReactFlowProvider>
        <ReactFlow
          className="pipeline"
          style={{ position: "absolute", inset: 0 }}
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          fitView
          fitViewOptions={FIT_VIEW_OPTIONS}
          minZoom={0.5}
          maxZoom={1.2}
          nodesDraggable={false}
          nodesConnectable={false}
          nodesFocusable={false}
          edgesFocusable={false}
          elementsSelectable={false}
          panOnDrag={false}
          panOnScroll={false}
          zoomOnScroll={false}
          zoomOnPinch={false}
          zoomOnDoubleClick={false}
          preventScrolling={false}
          deleteKeyCode={null}
          selectionKeyCode={null}
          multiSelectionKeyCode={null}
          // Defaults to Space, which would swallow the app's global Space-to-start shortcut.
          panActivationKeyCode={null}
          zoomActivationKeyCode={null}
          // The attribution badge is a link that would navigate the Tauri webview away.
          proOptions={PRO_OPTIONS}
        >
          <Background variant={BackgroundVariant.Dots} gap={16} size={1} color="var(--border)" />
          <AutoFit />
        </ReactFlow>
      </ReactFlowProvider>
    </div>
  );
}
