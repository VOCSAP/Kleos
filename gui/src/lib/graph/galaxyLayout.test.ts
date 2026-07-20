import { describe, expect, it } from 'vitest';
import { buildGalaxyTargets, seedGalaxyPositions, type GalaxyLayoutNode } from './galaxyLayout';

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
      expect(Math.hypot(target.clusterX, target.clusterY)).toBeGreaterThanOrEqual(92);
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
      expect(Math.hypot(target.x, target.y)).toBeLessThan(520);
      expect(Math.abs(target.z)).toBeLessThan(150);
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
});
