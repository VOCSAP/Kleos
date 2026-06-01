import { listAgents } from '$lib/api/soma';
import { useLive } from '$lib/realtime';
import { Badge } from '../ui/Badge';
import { Panel } from '../ui/Panel';
import { Spinner } from '../ui/Spinner';
import { Table } from '../ui/Table';

// Render the Soma live-agent table.
export function Soma() {
  const agents = useLive(['soma', 'agents'], () => listAgents(), 'soma');

  return (
    <div data-accent="soma">
      <header className="route-header">
        <div>
          <h1>Soma</h1>
          <p>Registered agents</p>
        </div>
      </header>
      <Panel title="Live agents">
        {agents.isLoading ? (
          <Spinner />
        ) : (
          <Table
            headers={['Name', 'Type', 'Status', 'Quality', 'Heartbeat']}
            rows={(agents.data ?? []).map((agent) => [
              agent.name,
              agent.type,
              <Badge label={agent.status} tone={agent.status === 'online' ? 'ok' : 'default'} />,
              agent.quality_score != null ? agent.quality_score.toFixed(2) : '--',
              agent.heartbeat_at ? agent.heartbeat_at.slice(11, 19) : '--'
            ])}
          />
        )}
      </Panel>
    </div>
  );
}
