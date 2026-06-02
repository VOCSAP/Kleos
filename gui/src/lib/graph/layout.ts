import type { GraphEdge, GraphNode } from '$lib/types';

// Minimum link distance for highly similar memories.
export const LINK_MIN = 24;

// Maximum link distance for weakly similar but still connected memories.
export const LINK_MAX = 240;

const STRENGTH_FLOOR = 0.08;
const CHARGE_BASE = 90;
const CHARGE_PER_SIZE = 6;

// Clamp a similarity score into the graph layout interval.
function clamp01(weight: number) {
  return Math.max(0, Math.min(1, weight));
}

// Convert real cosine similarity into shorter distance for stronger relationships.
export function linkDistance(weight: number) {
  return LINK_MAX - (LINK_MAX - LINK_MIN) * clamp01(weight);
}

// Convert real cosine similarity into monotonic edge pull with no zero-force links.
export function linkStrength(weight: number) {
  return STRENGTH_FLOOR + (1 - STRENGTH_FLOOR) * clamp01(weight);
}

// Convert node size into modest repulsion so large nodes separate without exploding the layout.
export function chargeStrength(node: Pick<GraphNode, 'size'>) {
  return -(CHARGE_BASE + CHARGE_PER_SIZE * (node.size ?? 1));
}

// Map semantic edge types to stable graph colors.
export const EDGE_COLOR: Record<GraphEdge['type'], string> = {
  association: '#3a5a5a',
  causal: '#f5a623',
  cite: '#3a3a5a',
  contradicts: '#ff5f5f',
  generalizes: '#a78bfa',
  has_fact: '#4a4a6a',
  mentions: '#3a4a6a',
  refines: '#38bdf8',
  resolves: '#00e5a0',
  temporal: '#5a5a3a'
};
