// Deterministic galaxy layout guides real memory nodes into readable spiral communities.

// One full turn in radians.
const TAU = Math.PI * 2;

// The angle that spaces successive points most evenly around a disc, which is
// why sunflower seeds use it -- here it keeps cluster members from lining up.
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

// Radius of the empty core the arms wind out from. This describes the SHAPE of
// the galaxy; how far it actually extends is derived from how much room the
// groups need, not hardcoded.
const CORE_RADIUS = 92;

// How fast an arm sweeps outward per radian of winding.
const ARM_GROWTH_PER_RADIAN = 60;

// Clear space demanded between neighbouring cluster edges, as a multiple of
// their combined radii. Above 1.0 leaves a visible lane between clusters.
const CLUSTER_CLEARANCE = 1.25;

// How far the disc is flattened along y. A galaxy reads as a disc rather than a
// ball because it is wider than it is tall, so every in-plane position is
// squashed by this factor -- which also means an arc budgeted as if the disc
// were circular shrinks by up to this much where the ellipse is tightest.
const DISC_FLATTENING = 0.64;

// A group holding at least this share of the graph is not drawn as a cluster.
//
// Real instances are dominated by one or two bookkeeping categories -- session
// auto-captures alone are ~63% of production nodes. Packing 15k nodes into a
// single cluster produces exactly the dense ball this layout exists to avoid,
// and no amount of spacing fixes it, because the cluster is then wider than the
// galaxy. Oversized groups instead become the diffuse disc the arms sit in,
// which is both the honest shape of the data and what a galaxy actually looks
// like: dust everywhere, bright knots where structure exists.
const DIFFUSE_GROUP_FRACTION = 0.05;

// Floor for the above, so small graphs never turn a modest group into dust.
const DIFFUSE_GROUP_MIN_MEMBERS = 400;

// Angular thickness of a dust arm, in radians. Wide enough that the arm looks
// like a drift of material rather than a drawn line, narrow enough that the
// two arms stay distinguishable from each other.
const DUST_ARM_SCATTER = 0.42;

// How many turns the dust arms make from the core to the rim.
//
// Dust winds at its own rate rather than following the clusters' radius-per-
// radian: the arms are spaced by how much room each cluster needs, which at
// production scale works out to several turns, and a spiral that wraps that
// many times overlaps itself into a uniform wash. Around one and a quarter
// turns is what makes a spiral read as a spiral.
const DUST_ARM_TURNS = 1.25;

// Half-thickness of the disc. Deliberately not scaled with the arm radius: a
// galaxy reads as a galaxy because it is wide and thin, so widening the arms
// while holding thickness makes the shape more disc-like, not less.
const DISC_HALF_THICKNESS = 28;

// A community smaller than this is not a legible arm -- it is a handful of
// nodes. Production data is long-tailed (6,165 communities across ~25k nodes,
// median size 3), and honouring every one of them produced hundreds of mutually
// overlapping clusters that smeared the spiral into a featureless sphere. Small
// communities fall back to their category so each arm position carries a group
// a viewer can actually distinguish.
const MIN_COMMUNITY_MEMBERS = 20;

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

// GalaxyLayoutLink is the edge subset whose visible topology guides diffuse threads.
export interface GalaxyLayoutLink {
  source: string | { id: string };
  target: string | { id: string };
  weight?: number;
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
  // True when this node belongs to an oversized bookkeeping group drawn as
  // background dust rather than a cluster, so the renderer can let it recede.
  diffuse: boolean;
}

// One cluster centre on the spiral, in world units.
interface ClusterCentre {
  x: number;
  y: number;
  z: number;
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

// Tally how many nodes belong to each community, so undersized ones can be
// folded into their category instead of claiming an arm position of their own.
function countCommunityMembers(nodes: readonly GalaxyLayoutNode[]): Map<number, number> {
  const counts = new Map<number, number>();
  for (const node of nodes) {
    if (node.community_id == null) continue;
    counts.set(node.community_id, (counts.get(node.community_id) ?? 0) + 1);
  }
  return counts;
}

// Resolve the strongest grouping for one node that is still large enough to read
// as a distinct cluster, falling back to category for community dust.
function groupKey(node: GalaxyLayoutNode, communitySizes: ReadonlyMap<number, number>): string {
  if (node.community_id != null && (communitySizes.get(node.community_id) ?? 0) >= MIN_COMMUNITY_MEMBERS) {
    return `community:${node.community_id}`;
  }
  return `category:${node.category || 'general'}`;
}

// Clamp an importance value to the supported one-through-ten range.
function normalizedImportance(node: GalaxyLayoutNode): number {
  const value = Number.isFinite(node.importance) ? Number(node.importance) : 5;
  return Math.max(1, Math.min(10, value));
}

// Radius of the cloud a group's members occupy around their cluster centre.
// Uncapped: a cluster that holds more nodes genuinely needs more room, and
// clamping it was what crushed large groups into an unreadable solid ball.
function groupSpread(memberCount: number): number {
  return 18 + Math.sqrt(memberCount) * 4.5;
}

// Depth of the cloud a group's members occupy along the view axis. Kept shallow
// relative to spread so clusters read as discs within the galactic plane.
function groupDepth(memberCount: number): number {
  return Math.min(112, 26 + Math.sqrt(memberCount) * 5);
}

// One placed cluster: where it sits and how much room it occupies.
interface PlacedCluster {
  centre: ClusterCentre;
  spread: number;
  arm: number;
}

// ForestNeighbour records one deterministic maximum-affinity forest connection.
interface ForestNeighbour {
  id: string;
  weight: number;
}

// Walk the compact groups outward along two spiral arms, giving each one exactly
// the arc it needs before the next begins.
//
// The original layout spaced groups by INDEX, which assumes every group is the
// same size. Real groups differ by orders of magnitude, so evenly spaced slots
// left big clusters overlapping their neighbours while small ones floated in
// dead space. Advancing by arc length instead makes non-overlap a property of
// the construction rather than something to correct for afterwards.
function placeClustersAlongArms(orderedSpreads: readonly number[]): PlacedCluster[] {
  // Winding state per arm; the two arms start half a turn apart.
  const arms = [
    { theta: 0, previousSpread: null as number | null },
    { theta: 0, previousSpread: null as number | null }
  ];

  return orderedSpreads.map((spread, groupIndex) => {
    const arm = groupIndex % 2;
    const state = arms[arm];
    let radius = CORE_RADIUS + ARM_GROWTH_PER_RADIAN * state.theta;

    if (state.previousSpread !== null) {
      // Advance far enough that this cluster's edge clears the previous one.
      // Spread is a RADIUS, so two clusters only separate once their centres
      // are the SUM of the radii apart -- the average leaves them overlapping.
      // Divided by the flattening so the lane survives where the ellipse is
      // tightest; budgeting in circular terms alone leaves clusters touching.
      const requiredArc = ((state.previousSpread + spread) * CLUSTER_CLEARANCE) / DISC_FLATTENING;
      state.theta += requiredArc / Math.max(radius, 1);
      radius = CORE_RADIUS + ARM_GROWTH_PER_RADIAN * state.theta;
    }
    state.previousSpread = spread;

    const angle = 0.55 + state.theta + arm * Math.PI;
    return {
      centre: {
        x: Math.cos(angle) * radius,
        y: Math.sin(angle) * radius * DISC_FLATTENING,
        z: Math.sin(angle * 0.55 + arm) * DISC_HALF_THICKNESS
      },
      spread,
      arm
    };
  });
}

// Resolve an edge endpoint before or after the force engine replaces ids with objects.
function endpointId(endpoint: GalaxyLayoutLink['source']): string | null {
  if (typeof endpoint === 'string') return endpoint;
  return typeof endpoint?.id === 'string' ? endpoint.id : null;
}

// Build a deterministic maximum spanning forest from the edges actually drawn.
function buildMaximumAffinityForest(
  nodes: readonly GalaxyLayoutNode[],
  links: readonly GalaxyLayoutLink[]
): Map<string, ForestNeighbour[]> {
  const nodeIds = new Set(nodes.map((node) => node.id));
  const forest = new Map<string, ForestNeighbour[]>();
  const parents = new Map<string, string>();
  for (const id of nodeIds) {
    forest.set(id, []);
    parents.set(id, id);
  }

  // Find the current disjoint-set root while compressing the traversed path.
  const findRoot = (id: string): string => {
    let root = id;
    while (parents.get(root) !== root) root = parents.get(root)!;
    let cursor = id;
    while (cursor !== root) {
      const next = parents.get(cursor)!;
      parents.set(cursor, root);
      cursor = next;
    }
    return root;
  };

  const ranked = links
    .map((link) => {
      const source = endpointId(link.source);
      const target = endpointId(link.target);
      const weight = Number.isFinite(link.weight) ? Number(link.weight) : 0;
      if (!source || !target || source === target || !nodeIds.has(source) || !nodeIds.has(target)) return null;
      return source < target ? { source, target, weight } : { source: target, target: source, weight };
    })
    .filter((link): link is { source: string; target: string; weight: number } => link !== null)
    .sort((left, right) =>
      right.weight - left.weight
      || left.source.localeCompare(right.source)
      || left.target.localeCompare(right.target)
    );

  for (const link of ranked) {
    const sourceRoot = findRoot(link.source);
    const targetRoot = findRoot(link.target);
    if (sourceRoot === targetRoot) continue;
    parents.set(targetRoot, sourceRoot);
    forest.get(link.source)!.push({ id: link.target, weight: link.weight });
    forest.get(link.target)!.push({ id: link.source, weight: link.weight });
  }

  forest.forEach((neighbours) => {
    neighbours.sort((left, right) => right.weight - left.weight || left.id.localeCompare(right.id));
  });
  return forest;
}

// Place one diffuse child beside its forest parent while keeping the galactic disc bounded.
function placeThreadChild(
  parent: GalaxyTarget,
  fallback: GalaxyTarget,
  parentId: string,
  childId: string,
  discRadius: number
): GalaxyTarget {
  const radialAngle = Math.atan2(parent.y / DISC_FLATTENING, parent.x);
  const windingDirection = parent.arm === 0 ? 1 : -1;
  const tangentAngle = radialAngle + windingDirection * Math.PI / 2;
  const branchScatter = (stableUnit(`${parentId}:${childId}:branch`) - 0.5) * 1.3;
  const angle = tangentAngle + branchScatter;
  const step = 14 + stableUnit(`${parentId}:${childId}:step`) * 20;
  let x = parent.x + Math.cos(angle) * step;
  let y = parent.y + Math.sin(angle) * step * DISC_FLATTENING;
  const zStep = (stableUnit(`${parentId}:${childId}:depth`) - 0.5) * 12;
  const z = Math.max(-DISC_HALF_THICKNESS * 2, Math.min(DISC_HALF_THICKNESS * 2, parent.z + zStep));

  // Preserve the luminous core and prevent long forest chains from escaping
  // beyond the decorative fallback disc that established the view bounds.
  const maxRadius = Math.max(CORE_RADIUS, discRadius * 1.08);
  const radius = Math.hypot(x, y / DISC_FLATTENING);
  if (radius < CORE_RADIUS || radius > maxRadius) {
    const boundedRadius = Math.max(CORE_RADIUS, Math.min(maxRadius, radius));
    const scale = boundedRadius / Math.max(radius, 1);
    x *= scale;
    y *= scale;
  }

  return {
    x,
    y,
    z,
    clusterX: parent.clusterX,
    clusterY: parent.clusterY,
    clusterZ: parent.clusterZ,
    groupKey: fallback.groupKey,
    arm: parent.arm,
    diffuse: true
  };
}

// Grow diffuse threads through the affinity forest from every compact cluster seed.
function placeDiffuseThreads(
  nodes: readonly GalaxyLayoutNode[],
  links: readonly GalaxyLayoutLink[],
  targets: Map<string, GalaxyTarget>,
  discRadius: number
): void {
  if (!links.length) return;
  const forest = buildMaximumAffinityForest(nodes, links);
  const visited = new Set<string>();
  const queue: string[] = [];
  const sortedIds = nodes.map((node) => node.id).sort((left, right) => left.localeCompare(right));

  // Every compact cluster is a fixed semantic seed. Multi-source traversal lets
  // a component grow from whichever real cluster reaches each branch first.
  for (const id of sortedIds) {
    if (targets.get(id)?.diffuse) continue;
    visited.add(id);
    queue.push(id);
  }

  // Traverse one queued forest root and place only diffuse descendants.
  const drainQueue = (): void => {
    let cursor = 0;
    while (cursor < queue.length) {
      const parentId = queue[cursor++];
      const parent = targets.get(parentId);
      if (!parent) continue;
      for (const neighbour of forest.get(parentId) ?? []) {
        if (visited.has(neighbour.id)) continue;
        const fallback = targets.get(neighbour.id);
        if (!fallback) continue;
        visited.add(neighbour.id);
        if (fallback.diffuse) {
          targets.set(neighbour.id, placeThreadChild(parent, fallback, parentId, neighbour.id, discRadius));
        }
        queue.push(neighbour.id);
      }
    }
  };

  drainQueue();

  // Components that never reach a compact cluster still represent real linked
  // memories. Keep one stable spiral fallback as their root, then grow the rest
  // locally so their edges retain meaning without inventing a semantic anchor.
  for (const id of sortedIds) {
    if (visited.has(id) || !targets.get(id)?.diffuse) continue;
    visited.add(id);
    queue.length = 0;
    queue.push(id);
    drainQueue();
  }
}

// Build stable spiral targets without mutating the supplied graph nodes.
export function buildGalaxyTargets(
  nodes: readonly GalaxyLayoutNode[],
  links: readonly GalaxyLayoutLink[] = []
): Map<string, GalaxyTarget> {
  const targets = new Map<string, GalaxyTarget>();
  if (!nodes.length) return targets;

  const communitySizes = countCommunityMembers(nodes);
  const groups = new Map<string, GalaxyLayoutNode[]>();
  for (const node of nodes) {
    const key = groupKey(node, communitySizes);
    const members = groups.get(key) ?? [];
    members.push(node);
    groups.set(key, members);
  }

  const allKeys = [...groups.keys()].sort((left, right) => {
    const hashOrder = stableHash(left) - stableHash(right);
    return hashOrder || left.localeCompare(right);
  });

  // Split the groups that are small enough to read as knots from the handful of
  // bookkeeping groups so large they can only be the disc the knots sit in.
  const diffuseThreshold = Math.max(DIFFUSE_GROUP_MIN_MEMBERS, nodes.length * DIFFUSE_GROUP_FRACTION);
  const compactKeys: string[] = [];
  const diffuseKeys: string[] = [];
  for (const key of allKeys) {
    const size = (groups.get(key) ?? []).length;
    (size >= diffuseThreshold ? diffuseKeys : compactKeys).push(key);
  }

  const placements = placeClustersAlongArms(
    compactKeys.map((key) => groupSpread((groups.get(key) ?? []).length))
  );

  // Members of a compact group orbit their cluster centre.
  compactKeys.forEach((key, groupIndex) => {
    const { centre, spread, arm } = placements[groupIndex];
    const members = [...(groups.get(key) ?? [])].sort((left, right) => left.id.localeCompare(right.id));
    const depth = groupDepth(members.length);
    const groupRotation = stableUnit(`${key}:rotation`) * TAU;

    members.forEach((node, localIndex) => {
      const density = members.length === 1 ? 0 : Math.sqrt((localIndex + 0.5) / members.length);
      const localAngle = groupRotation + localIndex * GOLDEN_ANGLE;
      const importancePull = 1 - ((normalizedImportance(node) - 1) / 9) * 0.42;
      const localRadius = density * spread * importancePull;
      const localDepth = (stableUnit(`${node.id}:depth`) - 0.5) * depth * importancePull;
      targets.set(node.id, {
        x: centre.x + Math.cos(localAngle) * localRadius,
        y: centre.y + Math.sin(localAngle) * localRadius * 0.7,
        z: centre.z + localDepth,
        clusterX: centre.x,
        clusterY: centre.y,
        clusterZ: centre.z,
        groupKey: key,
        arm,
        diffuse: false
      });
    });
  });

  // The disc spans whatever the arms ended up needing, so dust and structure
  // occupy the same galaxy rather than one floating inside or outside the other.
  const discRadius = placements.reduce(
    (widest, placement) => Math.max(widest, Math.hypot(placement.centre.x, placement.centre.y) + placement.spread),
    CORE_RADIUS * 2
  );

  // Exponent shaping how dust thins out from core to rim.
  //
  // A square root would spread motes evenly by AREA, which sounds right but
  // piles most of them into a hard ring at the rim, because that is where the
  // area is. Real galaxies are brightest in the middle and fade outward, so a
  // higher exponent pulls dust inward and softens the edge.
  const DUST_RADIAL_FALLOFF = 1.6;

  // Members of a diffuse group scatter across the disc as dust, but along the
  // SAME spiral the clusters follow rather than uniformly.
  //
  // Uniform dust is what kept the galaxy reading as a featureless oval: the
  // arms carry a minority of nodes, so an even wash of the majority buried
  // them. Real spiral galaxies are legible for exactly this reason -- the dust
  // traces the arms too. Each mote takes the arm angle implied by its radius,
  // plus a bounded scatter that thickens the arm without dissolving it.
  diffuseKeys.forEach((key) => {
    const members = [...(groups.get(key) ?? [])].sort((left, right) => left.id.localeCompare(right.id));

    members.forEach((node, localIndex) => {
      const radialUnit = stableUnit(`${node.id}:dust`);
      const radius = CORE_RADIUS + Math.pow(radialUnit, DUST_RADIAL_FALLOFF) * (discRadius - CORE_RADIUS);
      // Map the mote's radius onto a fixed number of turns, so the arms stay
      // legible no matter how far the clusters pushed the disc out.
      const discSpan = Math.max(1, discRadius - CORE_RADIUS);
      const windingAngle = ((radius - CORE_RADIUS) / discSpan) * TAU * DUST_ARM_TURNS;
      const arm = stableHash(`${node.id}:arm`) % 2;
      const scatter = (stableUnit(`${node.id}:scatter`) - 0.5) * DUST_ARM_SCATTER;
      const angle = 0.55 + windingAngle + arm * Math.PI + scatter;
      const x = Math.cos(angle) * radius;
      const y = Math.sin(angle) * radius * DISC_FLATTENING;
      const z = (stableUnit(`${node.id}:depth`) - 0.5) * DISC_HALF_THICKNESS * 2;
      targets.set(node.id, {
        x,
        y,
        z,
        // Dust has no meaningful centroid, so each mote anchors to itself and
        // the guide force simply holds it where it was scattered.
        clusterX: x,
        clusterY: y,
        clusterZ: z,
        groupKey: key,
        arm: localIndex % 2,
        diffuse: true
      });
    });
  });

  // Replace hash-only dust positions with short, topology-bearing filaments.
  // The original spiral scatter remains as the deterministic root and fallback
  // for isolated nodes; linked descendants are placed from real relationships.
  placeDiffuseThreads(nodes, links, targets, discRadius);

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
