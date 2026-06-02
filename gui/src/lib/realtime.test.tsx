import { act, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RealtimeProvider, useStreamStatus } from './realtime';

// Provides a controllable EventSource for provider status tests.
class FakeEventSource {
  static last: FakeEventSource;
  onopen: ((event: Event) => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  // Record the most recent EventSource instance.
  constructor() {
    FakeEventSource.last = this;
  }

  // Listener registration is not needed for status-only tests.
  addEventListener() {}

  // Close is a no-op for status-only tests.
  close() {}
}

// Render the current stream status for assertions.
function StatusProbe() {
  return <span>{useStreamStatus()}</span>;
}

describe('RealtimeProvider', () => {
  beforeEach(() => {
    vi.stubGlobal('EventSource', FakeEventSource as unknown as typeof EventSource);
  });

  it('provides stream status updates', () => {
    render(
      <RealtimeProvider>
        <StatusProbe />
      </RealtimeProvider>
    );

    expect(screen.getByText('connecting')).toBeInTheDocument();
    act(() => FakeEventSource.last.onopen?.(new Event('open')));
    expect(screen.getByText('live')).toBeInTheDocument();
  });
});
