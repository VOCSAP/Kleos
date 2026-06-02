import { request } from '$lib/http';
import type { ChiasmStats, Task } from '$lib/types';

// Fetch Chiasm task coordination statistics.
export const getChiasmStats = () => request<ChiasmStats>('/tasks/stats');

// Fetch Chiasm tasks with optional agent and project filters.
export async function listTasks(q: { agent?: string; project?: string } = {}): Promise<Task[]> {
  const params = new URLSearchParams();
  if (q.agent) {
    params.set('agent', q.agent);
  }
  if (q.project) {
    params.set('project', q.project);
  }
  const query = params.toString();
  return (await request<{ tasks: Task[] }>(`/tasks${query ? `?${query}` : ''}`)).tasks ?? [];
}

// Update one Chiasm task.
export const updateTask = (id: number, body: Record<string, unknown>) =>
  request<Task>(`/tasks/${id}`, { body, method: 'PATCH' });
