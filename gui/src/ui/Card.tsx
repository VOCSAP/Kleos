import type { ReactNode } from 'react';

// Render a compact framed surface for repeated dashboard items.
export function Card({ accent, children }: { accent?: string; children: ReactNode }) {
  return (
    <div className="glass" data-accent={accent} style={{ padding: 'var(--sp-5)' }}>
      {children}
    </div>
  );
}
