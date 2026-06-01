import { useState, type ReactNode } from 'react';

// Render a small tab switcher for dense dashboard panels.
export function Tabs({ tabs }: { tabs: Array<{ id: string; label: string; content: ReactNode }> }) {
  const [active, setActive] = useState(tabs[0]?.id);
  return (
    <div>
      <div style={{ borderBottom: '1px solid var(--border)', display: 'flex', gap: 4, marginBottom: 'var(--sp-4)' }}>
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActive(tab.id)}
            style={{
              background: 'transparent',
              border: 'none',
              borderBottom: active === tab.id ? '2px solid var(--accent)' : '2px solid transparent',
              color: active === tab.id ? 'var(--accent)' : 'var(--text-dim)',
              cursor: 'pointer',
              minHeight: 44,
              padding: '8px 14px'
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>
      {tabs.find((tab) => tab.id === active)?.content}
    </div>
  );
}
