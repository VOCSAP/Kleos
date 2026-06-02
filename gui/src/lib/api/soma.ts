import { request } from '$lib/http';
import type { Agent, Group, SomaStats } from '$lib/types';

// Fetch Soma agent registry statistics.
export const getSomaStats = () => request<SomaStats>('/soma/stats');

// Fetch registered Soma agents.
export async function listAgents(): Promise<Agent[]> {
  return (await request<{ agents: Agent[] }>('/soma/agents')).agents ?? [];
}

// Fetch registered Soma groups.
export async function listGroups(): Promise<Group[]> {
  return (await request<{ groups: Group[] }>('/soma/groups')).groups ?? [];
}
