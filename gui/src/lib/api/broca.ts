import { request } from '$lib/http';
import type { ActionEntry, BrocaStats } from '$lib/types';

// Fetch Broca action-log statistics.
export const getBrocaStats = () => request<BrocaStats>('/broca/stats');

// Fetch the latest Broca feed entries.
export async function getFeed(limit = 50): Promise<ActionEntry[]> {
  return (await request<{ items: ActionEntry[] }>(`/broca/feed?limit=${limit}`)).items ?? [];
}

// Ask Broca to answer a question from its action history.
export const ask = (question: string) =>
  request<{ answer: string; raw: unknown[] }>('/broca/ask', { body: { question }, method: 'POST' });
