// Defines supported toast color treatments.
type ToastTone = 'default' | 'ok' | 'err';

const TOAST_COLOR: Record<ToastTone, string> = {
  default: 'var(--text)',
  err: 'var(--err)',
  ok: 'var(--ok)'
};

// Render a compact transient message surface.
export function Toast({ message, tone = 'default' }: { message: string; tone?: ToastTone }) {
  return (
    <div className="glass" style={{ color: TOAST_COLOR[tone], fontSize: 12, padding: 'var(--sp-3) var(--sp-4)' }}>
      {message}
    </div>
  );
}
