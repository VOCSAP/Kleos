import { useQuery } from '@tanstack/react-query';
import { listMemories } from '$lib/api/memory';
import { Spinner } from '../../ui/Spinner';

// Render the latest memories in chronological list form.
export function Timeline() {
  const memories = useQuery({ queryFn: () => listMemories(60), queryKey: ['mem', 'list'] });
  if (memories.isLoading) {
    return <Spinner />;
  }

  return (
    <div className="memory-list">
      {(memories.data ?? []).map((memory) => (
        <article className="memory-row" key={memory.id}>
          <time>{memory.created_at.slice(0, 16)}</time>
          <p>{memory.content}</p>
        </article>
      ))}
    </div>
  );
}
