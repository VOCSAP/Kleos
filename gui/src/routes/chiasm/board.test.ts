import { describe, expect, it } from 'vitest';
import { COLUMNS, groupByColumn } from './board';
import type { Task } from '$lib/types';

// Create a minimal Chiasm task for board grouping tests.
function task(id: number, status: string): Task {
  return {
    agent: 'agent',
    assigned: true,
    created_at: '',
    guardrail_retries: 0,
    heartbeat_interval: 300,
    id,
    project: 'project',
    status,
    title: `Task ${id}`,
    updated_at: '',
    user_id: 1
  };
}

describe('groupByColumn', () => {
  it('buckets tasks into the six board columns', () => {
    const groups = groupByColumn([task(1, 'active'), task(2, 'queued'), task(3, 'completed'), task(4, 'blocked')]);

    expect(groups.active.map((item) => item.id)).toEqual([1]);
    expect(groups.queued.map((item) => item.id)).toEqual([2]);
    expect(groups.completed.map((item) => item.id)).toEqual([3]);
    expect(groups.blocked.map((item) => item.id)).toEqual([4]);
  });

  it('maps stale and human-blocked tasks into their intended columns', () => {
    const groups = groupByColumn([task(5, 'stale'), task(6, 'blocked_on_human')]);

    expect(groups.stale.map((item) => item.id)).toEqual([5]);
    expect(groups.blocked.map((item) => item.id)).toEqual([6]);
  });

  it('exposes the display column order', () => {
    expect(COLUMNS.map((column) => column.key)).toEqual(['active', 'queued', 'paused', 'blocked', 'stale', 'completed']);
  });
});
