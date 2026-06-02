// Render skeleton placeholder lines for async content. Preferred over a bare
// spinner on first load: it previews the shape of the data that is arriving.
export function Skeleton({ lines = 3, width = '100%' }: { lines?: number; width?: string }) {
  return (
    <div aria-busy="true" aria-live="polite" role="status" style={{ display: 'flex', flexDirection: 'column', gap: 'var(--sp-2)' }}>
      {Array.from({ length: lines }, (_, index) => (
        <div
          className="skeleton"
          key={index}
          style={{
            // Taper the last line so the block reads as text, not a box.
            height: 14,
            width: index === lines - 1 ? '60%' : width
          }}
        />
      ))}
    </div>
  );
}
