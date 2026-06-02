import { request } from '$lib/http';
import type { Entity, Health, Project, SearchResult } from '$lib/types';

// Fetch server health for memory overview panels.
export const getHealth = () => request<Health>('/health');

// Fetch global memory statistics.
export const getMemStats = () =>
  request<{ categories: Record<string, number>; entities: number; episodes: number; memories: number }>('/stats');

// Search memories by query text.
export async function search(query: string, limit = 20): Promise<SearchResult[]> {
  return (await request<{ results: SearchResult[] }>('/search', { body: { limit, query }, method: 'POST' })).results ?? [];
}

// Fetch latest memories for timeline-style views.
export async function listMemories(limit = 50): Promise<SearchResult[]> {
  return (await request<{ results: SearchResult[] }>(`/list?limit=${limit}`)).results ?? [];
}

// Fetch pending memory inbox items.
export async function getInbox(limit = 30): Promise<unknown[]> {
  return (await request<{ pending: unknown[] }>(`/inbox?limit=${limit}`)).pending ?? [];
}

// Fetch extracted memory entities.
export async function listEntities(limit = 50): Promise<Entity[]> {
  return (await request<{ entities: Entity[] }>(`/entities?limit=${limit}`)).entities ?? [];
}

// Fetch memory projects.
export async function listProjects(): Promise<Project[]> {
  return (await request<{ projects: Project[] }>('/projects')).projects ?? [];
}
