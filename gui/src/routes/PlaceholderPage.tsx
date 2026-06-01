// Render a named route surface until its full planned view lands.
export function PlaceholderPage({ phase = 'Phase 2', title }: { phase?: string; title: string }) {
  return (
    <div className="placeholder">
      <h1>{title}</h1>
      <p className="overview__subtle">Service view pending {phase}.</p>
    </div>
  );
}
