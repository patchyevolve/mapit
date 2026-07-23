
import { useMemo, useRef, useEffect, useState, useCallback } from "react";
import ForceGraph2D from "react-force-graph-2d";
import type { Node as MapitNode, Edge as MapitEdge } from "../types";

interface GraphViewProps {
  nodes: MapitNode[];
  edges?: MapitEdge[];
  onNodeClick?: (node: MapitNode) => void;
  onBackgroundClick?: () => void;
  highlightNodeId?: string;
}

// Exact node colors
const NODE_COLORS: Record<string, string> = {
  feature: "#d4a15d",
  file: "#7a9c6a",
  function: "#c75a4a",
  module: "#9b7bb8",
  type: "#d4964a",
  macro: "#c77a9a",
  global: "#6aab9e",
  external: "#6d5c4b",
};

// Helper to get group (directory for files/functions)
function getGroup(node: MapitNode): string {
  if (node.type === "file" && node.file_path) {
    return node.file_path.split("/").slice(0, -1).join("/") || "root";
  }
  if (node.type === "function" && node.file_path) {
    return node.file_path.split("/").slice(0, -1).join("/") || "root";
  }
  return node.type;
}

// Helper to hash node id for deterministic initial position (§2.1: no Math.random()!)
function hashStringToNumber(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash; // Convert to 32bit integer
  }
  return hash;
}

export function GraphView({ nodes, edges, onNodeClick, onBackgroundClick, highlightNodeId }: GraphViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [dimensions, setDimensions] = useState({ width: 800, height: 600 });

  // Resize observer
  useEffect(() => {
    if (!containerRef.current) return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) {
        setDimensions({
          width: entry.contentRect.width,
          height: entry.contentRect.height,
        });
      }
    });
    observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, []);

  const graphData = useMemo(() => {
    const nodeMap = new Map(nodes.map((n) => [n.id, n]));

    // Assign group positions (deterministic!)
    const groupPositions = new Map<string, { x: number; y: number }>();
    const groups = Array.from(new Set(nodes.map(getGroup)));
    const groupSpacing = 300;
    groups.forEach((group, index) => {
      // Use deterministic spiral instead of random
      const theta = Math.sqrt(index) * 2; // Golden spiral-like
      groupPositions.set(group, {
        x: groupSpacing * theta * Math.cos(theta),
        y: groupSpacing * theta * Math.sin(theta),
      });
    });

    const fgNodes = nodes.map((node) => {
      const group = getGroup(node);
      const groupPos = groupPositions.get(group) || { x: 0, y: 0 };
      // Use node id hash for deterministic jitter (§2.1)
      const hash = hashStringToNumber(node.id);
      const jitterX = ((hash % 1000) / 1000) * 100 - 50;
      const jitterY = (((hash / 1000) % 1000) / 1000) * 100 - 50;
      
      return {
        id: node.id,
        name: node.name,
        type: node.type,
        file_path: node.file_path,
        x: groupPos.x + jitterX,
        y: groupPos.y + jitterY,
        val: node.type === "feature" ? 8 : node.type === "file" ? 6 : 4,
      };
    });

    const fgEdges = (edges || [])
      .filter((e) => nodeMap.has(e.from_id) && nodeMap.has(e.to_id))
      .map((e) => ({
        id: e.id || `edge-${e.from_id}-${e.to_id}`,
        source: e.from_id,
        target: e.to_id,
        type: e.type,
        confidence: e.confidence,
      }));

    return { nodes: fgNodes, links: fgEdges };
  }, [nodes, edges]);

  // Node color function (§3.1)
  const nodeColor = useCallback((node: any) => {
    return NODE_COLORS[node.type] || NODE_COLORS.external;
  }, []);

  // Link color and width (§3.5)
  const linkColor = useCallback((link: any) => {
    switch (link.confidence) {
      case "exact": return "#d4a15d";
      case "probable": return "#9b8b7899";
      case "dynamic_unresolved": return "#9b8b784d";
      default: return "#9b8b78";
    }
  }, []);

  const linkWidth = useCallback((link: any) => {
    switch (link.confidence) {
      case "exact": return 2;
      case "probable": return 1;
      case "dynamic_unresolved": return 0.5;
      default: return 1;
    }
  }, []);

  return (
    <div ref={containerRef} className="w-full h-full relative z-0" style={{ background: "#1f1813" }}>
      <ForceGraph2D
        graphData={graphData}
        width={dimensions.width}
        height={dimensions.height}
        nodeLabel={(node: any) => `${node.name} (${node.type})${node.file_path ? "\n" + node.file_path : ""}`}
        nodeColor={nodeColor}
        nodeVal="val"
        nodeCanvasObject={(node: any, ctx: any, globalScale: number) => {
          const label = node.name;
          const fontSize = 12 / globalScale;
          ctx.font = `${fontSize}px sans-serif`;
          const textWidth = ctx.measureText(label).width;
          const bckgDimensions = [textWidth, fontSize].map(n => n + fontSize * 0.2);
          
          // Draw background rect for label readability
          ctx.fillStyle = 'rgba(20, 23, 31, 0.8)';
          ctx.fillRect(
            node.x - bckgDimensions[0] / 2,
            node.y - bckgDimensions[1] / 2 - (node.val * 1.5),
            bckgDimensions[0],
            bckgDimensions[1]
          );

          // Draw node circle
          ctx.beginPath();
          ctx.arc(node.x, node.y, node.val, 0, 2 * Math.PI);
          ctx.fillStyle = nodeColor(node);
          if (node.id === highlightNodeId) {
            ctx.strokeStyle = "#d4a15d";
            ctx.lineWidth = 3;
            ctx.stroke();
          }
          ctx.fill();

          // Draw label
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          ctx.fillStyle = '#e8ddd0';
          ctx.fillText(label, node.x, node.y - (node.val * 1.5));
        }}
        linkColor={linkColor}
        linkWidth={linkWidth}
        linkDirectionalParticles={0}
        onNodeClick={(node: any) => {
          const originalNode = nodes.find((n) => n.id === node.id);
          if (originalNode && onNodeClick) {
            onNodeClick(originalNode);
          }
        }}
        onBackgroundClick={onBackgroundClick}
        backgroundColor="#1f1813"
        d3AlphaDecay={0.01}
        d3VelocityDecay={0.2}
        warmupTicks={200}
        cooldownTicks={100}
      />
    </div>
  );
}
