import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { search } from '$lib/api/memory';
import { EmptyState } from '../../ui/EmptyState';
import { Spinner } from '../../ui/Spinner';

// Render memory search controls and search results.
export function Search() {
  const [query, setQuery] = useState('');
  const mutation = useMutation({ mutationFn: () => search(query, 25) });

  return (
    <div className="memory-view">
      <form
        className="memory-search"
        onSubmit={(event) => {
          event.preventDefault();
          mutation.mutate();
        }}
      >
        <input aria-label="Search memories" onChange={(event) => setQuery(event.target.value)} placeholder="search memories..." value={query} />
        <button>Search</button>
      </form>
      {mutation.isPending ? (
        <Spinner />
      ) : mutation.data?.length === 0 ? (
        <EmptyState message="No results." />
      ) : (
        <div className="memory-list">
          {(mutation.data ?? []).map((result) => (
            <article className="glass memory-card" key={result.id}>
              <div className="memory-card__meta">
                <span>{result.category}</span>
                <span>score {result.score.toFixed(3)}</span>
              </div>
              <p>{result.content}</p>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
