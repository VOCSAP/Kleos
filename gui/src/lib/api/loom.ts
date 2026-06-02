import { request } from '$lib/http';
import type { LoomStats, Run, Step, Workflow } from '$lib/types';

// Fetch Loom workflow-run statistics.
export const getLoomStats = () => request<LoomStats>('/loom/stats');

// Fetch Loom workflow definitions.
export async function listWorkflows(): Promise<Workflow[]> {
  return (await request<{ workflows: Workflow[] }>('/loom/workflows')).workflows ?? [];
}

// Fetch Loom workflow runs.
export async function listRuns(): Promise<Run[]> {
  return (await request<{ runs: Run[] }>('/loom/runs')).runs ?? [];
}

// Fetch steps for one Loom run.
export async function getSteps(runId: number): Promise<Step[]> {
  return (await request<{ steps: Step[] }>(`/loom/runs/${runId}/steps`)).steps ?? [];
}
