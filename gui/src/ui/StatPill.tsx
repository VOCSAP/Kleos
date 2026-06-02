// Render a large stat value with a compact uppercase label.
export function StatPill({ label, value }: { value: string; label: string }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
      <span style={{ color: 'var(--accent)', fontFamily: 'var(--font-display)', fontSize: 'var(--step-3)' }}>
        {value}
      </span>
      <span style={{ color: 'var(--text-dim)', fontSize: 11, letterSpacing: 0, textTransform: 'uppercase' }}>{label}</span>
    </div>
  );
}
