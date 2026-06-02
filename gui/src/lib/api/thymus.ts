import { request } from '$lib/http';
import type { DriftEvent, Evaluation, Rubric, ThymusStats } from '$lib/types';

// Fetch Thymus evaluation and drift statistics.
export const getThymusStats = () => request<ThymusStats>('/thymus/stats');

// Fetch Thymus rubrics.
export async function listRubrics(): Promise<Rubric[]> {
  return (await request<{ rubrics: Rubric[] }>('/thymus/rubrics')).rubrics ?? [];
}

// Fetch Thymus evaluations with optional agent and limit filters.
export async function listEvaluations(q: { agent?: string; limit?: number } = {}): Promise<Evaluation[]> {
  const params = new URLSearchParams();
  if (q.agent) {
    params.set('agent', q.agent);
  }
  params.set('limit', String(q.limit ?? 50));
  return (await request<{ evaluations: Evaluation[] }>(`/thymus/evaluations?${params}`)).evaluations ?? [];
}

// Fetch Thymus drift events.
export async function listDrift(): Promise<DriftEvent[]> {
  return (await request<{ drift_events: DriftEvent[] }>('/thymus/drift-events')).drift_events ?? [];
}
