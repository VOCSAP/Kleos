import { describe, expect, it } from 'vitest';
import { buildGalaxyTargets, seedGalaxyPositions } from './galaxyLayout';

// Describe link fixtures accepted by the topology-aware galaxy target builder.
import type { GalaxyLayoutLink } from './galaxyLayout';

// Describe node fixtures accepted by the galaxy layout helpers.
import type { GalaxyLayoutNode } from './galaxyLayout';

// Return one compact fixture spanning community and category fallback groups.
function fixtureNodes(): GalaxyLayoutNode[] {
  return [
    { id: 'm1', category: 'decision', community_id: 7, importance: 10 },
    { id: 'm2', category: 'decision', community_id: 7, importance: 6 },
    { id: 'm3', category: 'decision', community_id: 7, importance: 2 },
    { id: 'm4', category: 'incident', community_id: 11, importance: 8 },
    { id: 'm5', category: 'incident', community_id: 11, importance: 4 },
    { id: 'm6', category: 'general', importance: 5 },
    { id: 'm7', category: 'general', importance: 3 }
  ];
}

// Calculate the distance between two three-dimensional guide positions.
function distance(a: { x: number; y: number; z: number }, b: { x: number; y: number; z: number }): number {
  return Math.hypot(a.x - b.x, a.y - b.y, a.z - b.z);
}

// Return a production-shaped graph with one oversized diffuse group and one compact anchor.
function threadedFixture(): { nodes: GalaxyLayoutNode[]; links: GalaxyLayoutLink[] } {
  const anchorNodes: GalaxyLayoutNode[] = Array.from({ length: 20 }, (_, index) => ({
    id: `anchor-${index}`,
    category: 'decision',
    importance: 8
  }));
  const dustNodes: GalaxyLayoutNode[] = Array.from({ length: 420 }, (_, index) => ({
    id: `dust-${index}`,
    category: 'session',
    importance: 3
  }));
  const links: GalaxyLayoutLink[] = [
    { source: 'anchor-0', target: 'dust-0', weight: 1 },
    ...dustNodes.slice(1).map((node, index) => ({
      source: `dust-${index}`,
      target: node.id,
      weight: 0.99 - index / 10000
    }))
  ];
  return { nodes: [...anchorNodes, ...dustNodes], links };
}

describe('buildGalaxyTargets', () => {
  it('prevents graph fetch order from changing the visible galaxy', () => {
    const nodes = fixtureNodes();
    const forward = buildGalaxyTargets(nodes);
    const reverse = buildGalaxyTargets([...nodes].reverse());

    for (const node of nodes) {
      expect(reverse.get(node.id)).toEqual(forward.get(node.id));
    }
  });

  it('prevents community members from dissolving into unrelated graph dust', () => {
    const targets = buildGalaxyTargets(fixtureNodes());
    const community = ['m1', 'm2', 'm3'].map((id) => targets.get(id)!);

    community.forEach((target) => {
      expect(distance(target, { x: target.clusterX, y: target.clusterY, z: target.clusterZ })).toBeLessThan(72);
    });
  });

  it('prevents distinct communities from collapsing onto one unreadable centroid', () => {
    const targets = buildGalaxyTargets(fixtureNodes());
    const decision = targets.get('m1')!;
    const incident = targets.get('m4')!;
    const general = targets.get('m6')!;

    expect(distance(
      { x: decision.clusterX, y: decision.clusterY, z: decision.clusterZ },
      { x: incident.clusterX, y: incident.clusterY, z: incident.clusterZ }
    )).toBeGreaterThan(70);
    expect(distance(
      { x: incident.clusterX, y: incident.clusterY, z: incident.clusterZ },
      { x: general.clusterX, y: general.clusterY, z: general.clusterZ }
    )).toBeGreaterThan(70);
  });

  it('keeps a central void for the luminous galaxy core', () => {
    const targets = buildGalaxyTargets(fixtureNodes());

    targets.forEach((target) => {
      // The disc is an ellipse: y is flattened to 0.64 so the galaxy reads as
      // wide and thin. Measuring a circular radius would therefore understate
      // how far out a cluster sits, so the void is checked in the disc's own
      // geometry -- undo the flattening, then compare against the core radius.
      expect(Math.hypot(target.clusterX, target.clusterY / 0.64)).toBeGreaterThanOrEqual(92);
    });
  });

  it('returns finite bounded targets for empty, singleton, and large graphs', () => {
    expect(buildGalaxyTargets([]).size).toBe(0);

    const singleton = buildGalaxyTargets([{ id: 'only', category: 'general', importance: 5 }]).get('only')!;
    expect([singleton.x, singleton.y, singleton.z].every(Number.isFinite)).toBe(true);

    const large = Array.from({ length: 5000 }, (_, index) => ({
      id: `m${index}`,
      category: `category-${index % 24}`,
      community_id: index % 96,
      importance: (index % 10) + 1
    }));
    const targets = buildGalaxyTargets(large);
    expect(targets.size).toBe(large.length);
    targets.forEach((target) => {
      expect([target.x, target.y, target.z].every(Number.isFinite)).toBe(true);
      // Arms widen with group count to keep clusters apart, but the derived
      // scale is capped, so the galaxy stays within a bounded radius.
      expect(Math.hypot(target.x, target.y)).toBeLessThan(2300);
      expect(Math.abs(target.z)).toBeLessThan(150);
    });
  });

  it('folds undersized communities into their category instead of giving each an arm', () => {
    // Two nodes sharing a tiny community must not claim their own cluster: at
    // production scale that long tail produced hundreds of overlapping blobs.
    const nodes: GalaxyLayoutNode[] = [
      { id: 'a1', category: 'ops', community_id: 500 },
      { id: 'a2', category: 'ops', community_id: 501 },
      { id: 'a3', category: 'ops', community_id: 502 }
    ];
    const targets = buildGalaxyTargets(nodes);

    const groupKeys = new Set([...targets.values()].map((target) => target.groupKey));
    expect(groupKeys).toEqual(new Set(['category:ops']));
  });

  it('keeps a community that is large enough to read as its own cluster', () => {
    const nodes: GalaxyLayoutNode[] = Array.from({ length: 24 }, (_, index) => ({
      id: `b${index}`,
      category: 'ops',
      community_id: 900
    }));
    const targets = buildGalaxyTargets(nodes);

    const groupKeys = new Set([...targets.values()].map((target) => target.groupKey));
    expect(groupKeys).toEqual(new Set(['community:900']));
  });

  it('keeps neighbouring clusters further apart than they are wide', () => {
    // Mirrors the production shape that broke the original layout: many
    // distinct groups competing for room along the same two arms.
    const nodes: GalaxyLayoutNode[] = Array.from({ length: 900 }, (_, index) => ({
      id: `c${index}`,
      category: `category-${index % 90}`,
      importance: 5
    }));
    const targets = buildGalaxyTargets(nodes);

    const centres = new Map<string, { x: number; y: number; z: number }>();
    targets.forEach((target) => {
      centres.set(target.groupKey, { x: target.clusterX, y: target.clusterY, z: target.clusterZ });
    });
    const points = [...centres.values()];
    expect(points.length).toBeGreaterThan(50);

    // Every cluster must have breathing room: its nearest neighbour has to sit
    // further away than a single cluster's own radius (68 is the hard cap).
    points.forEach((point, index) => {
      const nearest = Math.min(
        ...points.filter((_, other) => other !== index).map((other) => distance(point, other))
      );
      expect(nearest).toBeGreaterThan(68);
    });
  });

  it('prevents initialization from overwriting restored simulation coordinates', () => {
    const nodes: GalaxyLayoutNode[] = [
      { id: 'existing', category: 'general', x: 1, y: 2, z: 3 },
      { id: 'missing', category: 'decision' }
    ];
    const targets = buildGalaxyTargets(nodes);

    seedGalaxyPositions(nodes, targets);

    expect(nodes[0]).toMatchObject({ x: 1, y: 2, z: 3 });
    expect([nodes[1].x, nodes[1].y, nodes[1].z].every(Number.isFinite)).toBe(true);
  });

  it('grows diffuse nodes into short threads from real graph links', () => {
    const { nodes, links } = threadedFixture();
    const targets = buildGalaxyTargets(nodes, links);
    const lengths = links.map((link) => distance(
      targets.get(typeof link.source === 'string' ? link.source : link.source.id)!,
      targets.get(typeof link.target === 'string' ? link.target : link.target.id)!
    ));
    const sorted = [...lengths].sort((left, right) => left - right);

    expect(sorted[Math.floor(sorted.length / 2)]).toBeLessThan(40);
    expect(Math.max(...lengths)).toBeLessThan(80);
  });

  it('keeps topology placement deterministic across node, edge, and endpoint forms', () => {
    const { nodes, links } = threadedFixture();
    const forward = buildGalaxyTargets(nodes, links);
    const reversed = buildGalaxyTargets(
      [...nodes].reverse(),
      [...links].reverse().map((link) => ({
        source: { id: typeof link.target === 'string' ? link.target : link.target.id },
        target: { id: typeof link.source === 'string' ? link.source : link.source.id },
        weight: link.weight
      }))
    );

    nodes.forEach((node) => expect(reversed.get(node.id)).toEqual(forward.get(node.id)));
  });

  it('bounds disconnected cycles and ignores invalid links', () => {
    const nodes: GalaxyLayoutNode[] = Array.from({ length: 420 }, (_, index) => ({
      id: `orphan-${index}`,
      category: 'session'
    }));
    const links: GalaxyLayoutLink[] = nodes.map((node, index) => ({
      source: node.id,
      target: nodes[(index + 1) % nodes.length].id,
      weight: index % 9 === 0 ? Number.NaN : 0.8
    }));
    links.push({ source: 'missing', target: 'orphan-0', weight: 1 });
    links.push({ source: 'orphan-0', target: 'orphan-0', weight: 1 });

    const targets = buildGalaxyTargets(nodes, links);
    expect(targets.size).toBe(nodes.length);
    targets.forEach((target) => {
      expect([target.x, target.y, target.z].every(Number.isFinite)).toBe(true);
      expect(Math.hypot(target.x, target.y)).toBeLessThan(500);
    });
  });
});
