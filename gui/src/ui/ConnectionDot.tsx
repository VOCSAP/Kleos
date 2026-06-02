import { useStreamStatus } from '$lib/realtime';

const STATUS_COLOR: Record<string, string> = {
  connecting: 'var(--warn)',
  down: 'var(--err)',
  live: 'var(--ok)'
};

// Render the current realtime connection status.
export function ConnectionDot() {
  const status = useStreamStatus();
  return (
    <span style={{ alignItems: 'center', color: 'var(--text-dim)', display: 'inline-flex', fontSize: 11, gap: 6 }}>
      <span aria-hidden="true" style={{ background: STATUS_COLOR[status], borderRadius: '50%', height: 8, width: 8 }} />
      {status === 'down' ? 'polling' : status}
    </span>
  );
}
