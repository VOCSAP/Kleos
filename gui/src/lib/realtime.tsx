import { QueryClient, QueryClientProvider, useQuery, useQueryClient } from '@tanstack/react-query';
import { createContext, useContext, useEffect, useMemo, useRef, useState } from 'react';
import type { QueryKey } from '@tanstack/react-query';
import type { ReactNode } from 'react';
import { AxonStream, type StreamStatus } from './sse';

const StreamCtx = createContext<AxonStream | null>(null);
const StatusCtx = createContext<StreamStatus>('connecting');

// Provide TanStack Query and a shared Axon stream to the dashboard.
export function RealtimeProvider({ children }: { children: ReactNode }) {
  const queryClient = useMemo(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry: 1,
            staleTime: 5000
          }
        }
      }),
    []
  );
  const stream = useRef<AxonStream | null>(null);
  if (!stream.current) {
    stream.current = new AxonStream('kleos-gui');
  }
  const [status, setStatus] = useState<StreamStatus>('connecting');

  useEffect(() => {
    const activeStream = stream.current!;
    const off = activeStream.onStatus(setStatus);
    activeStream.connect();
    return () => {
      off();
      activeStream.close();
    };
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <StreamCtx.Provider value={stream.current}>
        <StatusCtx.Provider value={status}>{children}</StatusCtx.Provider>
      </StreamCtx.Provider>
    </QueryClientProvider>
  );
}

// Return the current Axon stream status.
export function useStreamStatus() {
  return useContext(StatusCtx);
}

// Bind one query to an Axon channel with polling as a fallback.
export function useLive<T>(key: QueryKey, fetcher: () => Promise<T>, channel: string, pollMs = 12000) {
  const stream = useContext(StreamCtx);
  const queryClient = useQueryClient();

  useEffect(() => {
    if (!stream) {
      return undefined;
    }
    return stream.onChannel(channel, () => queryClient.invalidateQueries({ queryKey: key }));
  }, [channel, key, queryClient, stream]);

  return useQuery({ queryFn: fetcher, queryKey: key, refetchInterval: pollMs });
}
