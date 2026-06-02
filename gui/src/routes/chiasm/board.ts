import type { Task } from '$lib/types';

// Identifies one visible Chiasm board column.
export type ColumnKey = 'active' | 'queued' | 'paused' | 'blocked' | 'stale' | 'completed';

// Defines the Chiasm board display order and drop target status.
export const COLUMNS: { key: ColumnKey; label: string; setStatus: string }[] = [
  { key: 'active', label: 'LIVE', setStatus: 'active' },
  { key: 'queued', label: 'QUEUE', setStatus: 'queued' },
  { key: 'paused', label: 'STANDBY', setStatus: 'paused' },
  { key: 'blocked', label: 'ALERT', setStatus: 'blocked' },
  { key: 'stale', label: 'STALE', setStatus: 'stale' },
  { key: 'completed', label: 'DONE', setStatus: 'completed' }
];

const STATUS_COLUMN: Record<string, ColumnKey> = {
  active: 'active',
  blocked: 'blocked',
  blocked_on_human: 'blocked',
  completed: 'completed',
  paused: 'paused',
  queued: 'queued',
  stale: 'stale'
};

// Group tasks into stable board columns with unknown statuses kept visible.
export function groupByColumn(tasks: Task[]): Record<ColumnKey, Task[]> {
  const out: Record<ColumnKey, Task[]> = {
    active: [],
    blocked: [],
    completed: [],
    paused: [],
    queued: [],
    stale: []
  };

  for (const task of tasks) {
    out[STATUS_COLUMN[task.status] ?? 'active'].push(task);
  }

  return out;
}
