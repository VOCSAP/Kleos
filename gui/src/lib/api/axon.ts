import { request } from '$lib/http';
import type { AxonEvent, AxonStats, Channel } from '$lib/types';

// Fetch Axon event-bus statistics.
export const getAxonStats = () => request<AxonStats>('/axon/stats');

// Fetch Axon events with optional channel and limit filters.
export async function listEvents(q: { channel?: string; limit?: number } = {}): Promise<AxonEvent[]> {
  const params = new URLSearchParams();
  if (q.channel) {
    params.set('channel', q.channel);
  }
  params.set('limit', String(q.limit ?? 100));
  return (await request<{ events: AxonEvent[] }>(`/axon/events?${params}`)).events ?? [];
}

// Fetch Axon channels.
export async function listChannels(): Promise<Channel[]> {
  return (await request<{ channels: Channel[] }>('/axon/channels')).channels ?? [];
}
