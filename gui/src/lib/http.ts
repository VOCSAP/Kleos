// Represents an unsuccessful HTTP response from the Kleos API.
export class HttpError extends Error {
  // Create an error that preserves the response status and body.
  constructor(
    public status: number,
    public body: string
  ) {
    super(`${status}: ${body}`);
    this.name = 'HttpError';
  }
}

let unauthorized: (() => void) | null = null;

// Register the callback fired when an API request receives 401.
export function onUnauthorized(cb: () => void) {
  unauthorized = cb;
  return () => {
    if (unauthorized === cb) {
      unauthorized = null;
    }
  };
}

// Build the API URL for production same-origin or local Vite proxy mode.
export function buildUrl(path: string, port?: string): string {
  const resolvedPort = port ?? (typeof window !== 'undefined' ? window.location.port : '');
  return `${resolvedPort === '4200' ? '' : '/api'}${path}`;
}

// Describes options accepted by the shared request helper.
export interface RequestOpts {
  method?: string;
  body?: unknown;
  token?: string;
  port?: string;
  signal?: AbortSignal;
}

// Execute a same-origin Kleos API request and parse its JSON response.
export async function request<T>(path: string, opts: RequestOpts = {}): Promise<T> {
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  const token = opts.token ?? currentToken();
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  const res = await fetch(buildUrl(path, opts.port), {
    body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    credentials: 'same-origin',
    headers,
    method: opts.method ?? 'GET',
    signal: opts.signal
  });

  if (res.status === 401) {
    unauthorized?.();
  }
  if (!res.ok) {
    throw new HttpError(res.status, await res.text());
  }

  return res.json() as Promise<T>;
}

// Return the saved bearer token used by development API calls.
export function currentToken(): string {
  if (typeof localStorage === 'undefined') {
    return '';
  }
  return localStorage.getItem('kleos_api_key') ?? '';
}
