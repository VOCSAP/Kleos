import type { ReactNode } from 'react';

// Render a dashboard panel with an optional accent-colored heading.
export function Panel({ children, title }: { title?: string; children: ReactNode }) {
  return (
    <section className="glass" style={{ padding: 'var(--sp-5)' }}>
      {title ? (
        <h2
          style={{
            color: 'var(--accent)',
            fontFamily: 'var(--font-display)',
            fontSize: 'var(--step-2)',
            marginBottom: 'var(--sp-3)'
          }}
        >
          {title}
        </h2>
      ) : null}
      {children}
    </section>
  );
}
