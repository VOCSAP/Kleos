import type { ReactNode } from 'react';

// Render a helpful placeholder when a surface has no records. A bare `message`
// keeps the original call sites working; `title`/`hint`/`action` upgrade it
// into a guiding empty state.
export function EmptyState({
  message = 'Nothing here yet.',
  title,
  hint,
  action
}: {
  message?: string;
  title?: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div
      style={{
        alignItems: 'center',
        color: 'var(--text-faint)',
        display: 'flex',
        flexDirection: 'column',
        fontSize: 12,
        gap: 'var(--sp-2)',
        padding: 'var(--sp-6)',
        textAlign: 'center'
      }}
    >
      {title ? (
        <span style={{ color: 'var(--text-dim)', fontFamily: 'var(--font-display)', fontSize: 'var(--step-1)' }}>
          {title}
        </span>
      ) : null}
      <span>{message}</span>
      {hint ? <span style={{ color: 'var(--text-faint)', fontSize: 11 }}>{hint}</span> : null}
      {action ? <div style={{ marginTop: 'var(--sp-2)' }}>{action}</div> : null}
    </div>
  );
}
