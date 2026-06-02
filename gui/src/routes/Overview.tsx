import { getAxonStats } from '$lib/api/axon';
import { getBrocaStats } from '$lib/api/broca';
import { getChiasmStats } from '$lib/api/chiasm';
import { getLoomStats } from '$lib/api/loom';
import { getSomaStats } from '$lib/api/soma';
import { getThymusStats } from '$lib/api/thymus';
import { useLive } from '$lib/realtime';
import { SERVICES, type ServiceDef, type ServiceId } from '$lib/services';
import type {
  AxonStats,
  BrocaStats,
  ChiasmStats,
  LoomStats,
  SomaStats,
  ThymusStats
} from '$lib/types';
import { useMemo } from 'react';
import { Badge } from '../ui/Badge';
import { Card } from '../ui/Card';
import { StatPill } from '../ui/StatPill';

const STAT_FETCHERS: Record<ServiceId, () => Promise<unknown>> = {
  axon: getAxonStats,
  broca: getBrocaStats,
  chiasm: getChiasmStats,
  loom: getLoomStats,
  soma: getSomaStats,
  thymus: getThymusStats
};

// Describes the compact metric shown for one service card.
interface ServiceMetric {
  detail: string;
  label: string;
  value: string;
}

// Render the six-service operational overview.
export function Overview() {
  return (
    <div className="overview">
      <header className="overview__header">
        <div>
          <h1 className="overview__title">Mission Control</h1>
          <p className="overview__subtle">Coordination services at a glance.</p>
        </div>
        <Badge label="Axon live" tone="ok" />
      </header>
      <section className="overview__grid" aria-label="Service status">
        {SERVICES.map((service) => (
          <ServiceOverviewCard key={service.id} service={service} />
        ))}
      </section>
    </div>
  );
}

// Render one live service summary card.
function ServiceOverviewCard({ service }: { service: ServiceDef }) {
  const queryKey = useMemo(() => ['stats', service.id] as const, [service.id]);
  const query = useLive(queryKey, STAT_FETCHERS[service.id], service.channel);
  const metric = summarizeServiceStats(service.id, query.data);
  const badgeTone = query.isError ? 'err' : query.isLoading ? 'warn' : 'ok';
  const badgeLabel = query.isError ? 'error' : query.isLoading ? 'loading' : 'live';

  return (
    <Card accent={service.id}>
      <div className="overview-card">
        <div className="overview-card__top">
          <span className="overview-card__name">{service.label}</span>
          <Badge label={badgeLabel} tone={badgeTone} />
        </div>
        <StatPill label={metric.label} value={metric.value} />
        <p className="overview-card__detail">{metric.detail}</p>
      </div>
    </Card>
  );
}

// Convert typed service stats into a consistent overview metric.
function summarizeServiceStats(serviceId: ServiceId, data: unknown): ServiceMetric {
  switch (serviceId) {
    case 'chiasm': {
      const stats = data as ChiasmStats | undefined;
      return {
        detail: `${formatCount(stats?.by_status.active)} active`,
        label: 'tasks',
        value: formatCount(stats?.total)
      };
    }
    case 'broca': {
      const stats = data as BrocaStats | undefined;
      return {
        detail: `${formatCount(stats?.agents)} agents across ${formatCount(stats?.services)} services`,
        label: 'actions',
        value: formatCount(stats?.total_actions)
      };
    }
    case 'soma': {
      const stats = data as SomaStats | undefined;
      return {
        detail: `${formatCount(stats?.online_agents)} online`,
        label: 'agents',
        value: formatCount(stats?.total_agents)
      };
    }
    case 'loom': {
      const stats = data as LoomStats | undefined;
      return {
        detail: `${formatCount(stats?.active_runs)} active runs`,
        label: 'runs',
        value: formatCount(stats?.runs)
      };
    }
    case 'axon': {
      const stats = data as AxonStats | undefined;
      return {
        detail: `${formatCount(stats?.channels)} channels`,
        label: 'events',
        value: formatCount(stats?.total_events)
      };
    }
    case 'thymus': {
      const stats = data as ThymusStats | undefined;
      return {
        detail: `${formatCount(stats?.evaluations)} evaluations`,
        label: 'rubrics',
        value: formatCount(stats?.rubrics)
      };
    }
  }
}

// Format numeric counts for compact panels while data is loading.
function formatCount(value: number | undefined) {
  return typeof value === 'number' ? value.toLocaleString() : '...';
}
