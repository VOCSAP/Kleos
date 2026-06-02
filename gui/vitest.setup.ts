import '@testing-library/jest-dom/vitest';

// Provide a controllable default EventSource shape for tests that replace it.
if (!('EventSource' in globalThis)) {
  // @ts-expect-error jsdom does not ship EventSource.
  globalThis.EventSource = class {
    // Close is intentionally empty because tests install their own behavior.
    close() {}

    // Listener registration is intentionally empty for the default stub.
    addEventListener() {}
  } as unknown as typeof EventSource;
}
