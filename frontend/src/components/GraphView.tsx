"use client";

import { Background, Controls, MarkerType, MiniMap, ReactFlow, ReactFlowProvider, useReactFlow } from "@xyflow/react";
import type { Edge } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useMemo } from "react";
import { edgeSpecs, nodeSpecs } from "../lib/flow-graph.ts";
import type { LayoutResult } from "../lib/layout.ts";
import type { PairEdge, ReportModel } from "../lib/model.ts";
import { STATE_STYLE } from "../lib/theme.ts";
import { FileNode } from "./FileNode";
import type { FileFlowNode, FileNodeData } from "./FileNode";

const nodeTypes = { file: FileNode };
const LARGE_VIEW = 1500;

interface Props {
  model: ReportModel;
  visible: string[];
  edges: PairEdge[];
  layout: LayoutResult;
  selected: string | null;
  pathNodes: ReadonlySet<string>;
  pathEdges: ReadonlySet<string>;
  matches: ReadonlySet<string>;
  queryActive: boolean;
  onSelect: (path: string | null) => void;
}

function FitOnChange({ token }: { token: string }) {
  const rf = useReactFlow();
  useEffect(() => {
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const id = requestAnimationFrame(() => {
      void rf.fitView({ padding: 0.15, duration: reduce ? 0 : 250 });
    });
    return () => cancelAnimationFrame(id);
  }, [token, rf]);
  return null;
}

function Canvas({ model, visible, edges, layout, selected, pathNodes, pathEdges, matches, queryActive, onSelect }: Props) {
  const nodes: FileFlowNode[] = useMemo(
    () =>
      nodeSpecs({ model, visible, edges, layout, selected, pathNodes, pathEdges, matches, queryActive }).map((n) => ({
        id: n.id,
        type: "file" as const,
        position: n.position,
        data: n.data,
        draggable: false,
        ariaLabel: n.ariaLabel,
      })),
    [model, visible, edges, layout, selected, pathNodes, pathEdges, matches, queryActive],
  );

  const flowEdges: Edge[] = useMemo(
    () =>
      edgeSpecs({ edges, pathEdges }).map((e) => ({
        id: e.id,
        source: e.source,
        target: e.target,
        markerEnd: { type: MarkerType.ArrowClosed, color: e.stroke, width: 14, height: 14 },
        style: { stroke: e.stroke, strokeWidth: e.strokeWidth, strokeDasharray: e.dash, opacity: e.opacity },
        zIndex: e.zIndex,
        ariaLabel: e.ariaLabel,
      })),
    [edges, pathEdges],
  );

  const fitToken = `${visible.length}:${visible[0] ?? ""}:${visible[visible.length - 1] ?? ""}:${edges.length}`;

  return (
    <>
      <ReactFlow
        nodes={nodes}
        edges={flowEdges}
        nodeTypes={nodeTypes}
        onNodeClick={(_, node) => onSelect(node.id)}
        onPaneClick={() => onSelect(null)}
        nodesDraggable={false}
        nodesConnectable={false}
        minZoom={0.05}
        maxZoom={2}
        onlyRenderVisibleElements
        colorMode="dark"
        fitView
        fitViewOptions={{ padding: 0.15 }}
      >
        <Background gap={28} color="#1B2431" />
        <Controls showInteractive={false} />
        <MiniMap
          pannable
          zoomable
          maskColor="rgba(14,19,26,0.7)"
          style={{ background: "#141B25" }}
          nodeColor={(n) => STATE_STYLE[(n.data as FileNodeData).state].color}
        />
        <FitOnChange token={fitToken} />
      </ReactFlow>
      {visible.length === 0 && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center p-6 text-center text-sm text-ink-dim">
          <p className="max-w-sm">
            Nothing matches the current filters. Turn a file state back on, switch to “All files”, or clear the search.
          </p>
        </div>
      )}
      {visible.length > LARGE_VIEW && (
        <p className="pointer-events-none absolute left-1/2 top-3 -translate-x-1/2 rounded border border-ink-line bg-ink-panel px-3 py-1 text-xs text-ink-dim">
          Large view ({visible.length} files). “Impact only” or a search will be easier to read.
        </p>
      )}
    </>
  );
}

export function GraphView(props: Props) {
  return (
    <ReactFlowProvider>
      <Canvas {...props} />
    </ReactFlowProvider>
  );
}
