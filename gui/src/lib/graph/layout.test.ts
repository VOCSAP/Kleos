import { describe, expect, it } from 'vitest';
import { LINK_MAX, LINK_MIN, chargeStrength, linkDistance, linkStrength } from './layout';

describe('graph layout math', () => {
  it('shrinks link distance monotonically as similarity rises', () => {
    expect(linkDistance(1)).toBeCloseTo(LINK_MIN);
    expect(linkDistance(0)).toBeCloseTo(LINK_MAX);
    expect(linkDistance(0.8)).toBeLessThan(linkDistance(0.4));
  });

  it('keeps every edge pulling with no zero-force buckets', () => {
    expect(linkStrength(0)).toBeGreaterThan(0);
    expect(linkStrength(0.49)).toBeGreaterThan(0);
    expect(linkStrength(1)).toBeGreaterThan(linkStrength(0));
  });

  it('keeps charge modest while scaling with node size', () => {
    expect(chargeStrength({ size: 5 })).toBeLessThan(0);
    expect(chargeStrength({ size: 5 })).toBeGreaterThan(-400);
    expect(Math.abs(chargeStrength({ size: 20 }))).toBeGreaterThan(Math.abs(chargeStrength({ size: 2 })));
  });
});
