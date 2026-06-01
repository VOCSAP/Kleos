// Render a small animated loading spinner.
export function Spinner() {
  return (
    <div
      aria-label="Loading"
      role="status"
      style={{
        animation: 'spin .7s linear infinite',
        border: '2px solid var(--border)',
        borderRadius: '50%',
        borderTopColor: 'var(--accent)',
        height: 14,
        width: 14
      }}
    />
  );
}
