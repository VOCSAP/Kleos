import { useQuery } from '@tanstack/react-query';
import { getInbox } from '$lib/api/memory';
import { EmptyState } from '../../ui/EmptyState';
import { Spinner } from '../../ui/Spinner';

// Render pending memory inbox items.
export function Inbox() {
  const inbox = useQuery({ queryFn: () => getInbox(30), queryKey: ['mem', 'inbox'] });
  if (inbox.isLoading) {
    return <Spinner />;
  }
  if (!inbox.data?.length) {
    return <EmptyState message="Inbox empty." />;
  }

  return <pre className="memory-json">{JSON.stringify(inbox.data, null, 2)}</pre>;
}
