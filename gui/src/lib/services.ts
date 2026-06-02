// Identifies a coordination service rendered by the dashboard.
export type ServiceId = 'chiasm' | 'broca' | 'soma' | 'loom' | 'axon' | 'thymus';

// Describes the route, stats endpoint, and realtime channel for a service.
export interface ServiceDef {
  id: ServiceId;
  label: string;
  route: string;
  statsPath: string;
  channel: string;
}

// Lists the six coordination services in dashboard display order.
export const SERVICES: ServiceDef[] = [
  { id: 'chiasm', label: 'Chiasm', route: '/chiasm', statsPath: '/tasks/stats', channel: 'chiasm' },
  { id: 'broca', label: 'Broca', route: '/broca', statsPath: '/broca/stats', channel: 'broca' },
  { id: 'soma', label: 'Soma', route: '/soma', statsPath: '/soma/stats', channel: 'soma' },
  { id: 'loom', label: 'Loom', route: '/loom', statsPath: '/loom/stats', channel: 'loom' },
  { id: 'axon', label: 'Axon', route: '/axon', statsPath: '/axon/stats', channel: 'axon' },
  { id: 'thymus', label: 'Thymus', route: '/thymus', statsPath: '/thymus/stats', channel: 'thymus' }
];
