// Deterministic galaxy layout guides real memory nodes into readable spiral communities.

const TAU = Math.PI * 2;
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

// GalaxyLayoutNode is the immutable subset required to derive one guide target.
export interface GalaxyLayoutNode {
  id: string;
  category?: string;
  community_id?: number;
  importance?: number;
  x?: number;
  y?: number;
  z?: number;
}

// GalaxyTarget stores a node position plus its community anchor for diagnostics and tests.
export interface GalaxyTarget {
  x: number;
  y: number;
  z: number;
  clusterX: number;
  clusterY: number;
  clusterZ: number;
  groupKey: string;
  arm: number;
}

// Hash a string into a stable unsigned integer without runtime randomness.
function stableHash(value: string): number {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index++) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

// Convert a stable hash into a repeatable value between zero and one.
function stableUnit(value: string): number {
  return stableHash(value) / 0xffffffff;
}

// Resolve the strongest available semantic grouping for one memory node.
function groupKey(node: GalaxyLayoutNode): string {
  if (node.community_id != null) return `community:${node.community_id}`;
  return `category:${node.category || 'general'}`;
}

// Clamp an importance value to the supported one-through-ten range.
function normalizedImportance(node: GalaxyLayoutNode): number {
  const value = Number.isFinite(node.importance) ? Number(node.importance) : 5;
  return Math.max(1, Math.min(10, value));
}

// Build stable spiral targets without mutating the supplied graph nodes.
export function buildGalaxyTargets(nodes: readonly GalaxyLayoutNode[]): Map<string, GalaxyTarget> {
  const targets = new Map<string, GalaxyTarget>();
  if (!nodes.length) return targets;

  const groups = new Map<string, GalaxyLayoutNode[]>();
  for (const node of nodes) {
    const key = groupKey(node);
    const members = groups.get(key) ?? [];
    members.push(node);
    groups.set(key, members);
  }

  const keys = [...groups.keys()].sort((left, right) => {
    const hashOrder = stableHash(left) - stableHash(right);
    return hashOrder || left.localeCompare(right);
  });
  const armSteps = Math.max(1, Math.ceil(keys.length / 2));

  keys.forEach((key, groupIndex) => {
    const arm = groupIndex % 2;
    const step = Math.floor(groupIndex / 2);
    const progress = (step + 1) / (armSteps + 1);
    const orbitRadius = 92 + Math.sqrt(progress) * 270;
    const orbitAngle = 0.55 + progress * TAU * 1.18 + arm * Math.PI;
    const clusterX = Math.cos(orbitAngle) * orbitRadius;
    const clusterY = Math.sin(orbitAngle) * orbitRadius * 0.64;
    const clusterZ = Math.sin(orbitAngle * 0.55 + arm) * 28;
    const members = [...(groups.get(key) ?? [])].sort((left, right) => left.id.localeCompare(right.id));
    const spread = Math.min(68, 18 + Math.sqrt(members.length) * 4.5);
    const depth = Math.min(112, 26 + Math.sqrt(members.length) * 5);
    const groupRotation = stableUnit(`${key}:rotation`) * TAU;

    members.forEach((node, localIndex) => {
      const density = members.length === 1 ? 0 : Math.sqrt((localIndex + 0.5) / members.length);
      const localAngle = groupRotation + localIndex * GOLDEN_ANGLE;
      const importancePull = 1 - ((normalizedImportance(node) - 1) / 9) * 0.42;
      const localRadius = density * spread * importancePull;
      const localDepth = (stableUnit(`${node.id}:depth`) - 0.5) * depth * importancePull;
      targets.set(node.id, {
        x:clusterX + Math.cos(localAngle) * localRadius,
        y:clusterY + Math.sin(localAngle) * localRadius * 0.7,
        z:clusterZ + localDepth,
        clusterX,
        clusterY,
        clusterZ,
        groupKey:key,
        arm
      });
    });
  });

  return targets;
}

// Seed only missing simulation coordinates so pinned or restored positions survive.
export function seedGalaxyPositions(nodes: GalaxyLayoutNode[], targets: ReadonlyMap<string, GalaxyTarget>): void {
  for (const node of nodes) {
    const target = targets.get(node.id);
    if (!target) continue;
    if (!Number.isFinite(node.x)) node.x = target.x;
    if (!Number.isFinite(node.y)) node.y = target.y;
    if (!Number.isFinite(node.z)) node.z = target.z;
  }
}
