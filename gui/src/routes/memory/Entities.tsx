import { useQuery } from '@tanstack/react-query';
import { listEntities } from '$lib/api/memory';
import { Spinner } from '../../ui/Spinner';
import { Table } from '../../ui/Table';

// Render extracted memory entities.
export function Entities() {
  const entities = useQuery({ queryFn: () => listEntities(80), queryKey: ['mem', 'entities'] });
  if (entities.isLoading) {
    return <Spinner />;
  }

  return (
    <Table
      headers={['Name', 'Type', 'Seen', 'Last']}
      rows={(entities.data ?? []).map((entity) => [
        entity.name,
        entity.entity_type,
        entity.occurrence_count,
        entity.last_seen_at.slice(0, 16)
      ])}
    />
  );
}
