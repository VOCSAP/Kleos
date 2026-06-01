// Defines supported badge color treatments.
type Tone = 'default' | 'ok' | 'warn' | 'err';

const TONE_COLOR: Record<Tone, string> = {
  default: 'var(--text-dim)',
  err: 'var(--err)',
  ok: 'var(--ok)',
  warn: 'var(--warn)'
};

// Render a compact status label.
export function Badge({ label, tone = 'default' }: { label: string; tone?: Tone }) {
  const color = TONE_COLOR[tone];
  return (
    <span
      style={{
        alignItems: 'center',
        background: `color-mix(in srgb, ${color} 12%, transparent)`,
        border: `1px solid ${color}`,
        borderRadius: 999,
        color,
        display: 'inline-flex',
        fontSize: 11,
        minHeight: 22,
        padding: '2px 8px'
      }}
    >
      {label}
    </span>
  );
}
