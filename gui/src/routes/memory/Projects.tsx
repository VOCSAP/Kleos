import { useQuery } from '@tanstack/react-query';
import { listProjects } from '$lib/api/memory';
import { Badge } from '../../ui/Badge';
import { Spinner } from '../../ui/Spinner';
import { Table } from '../../ui/Table';

// Render memory projects with their status and memory counts.
export function Projects() {
  const projects = useQuery({ queryFn: () => listProjects(), queryKey: ['mem', 'projects'] });
  if (projects.isLoading) {
    return <Spinner />;
  }

  return (
    <Table
      headers={['Name', 'Status', 'Memories']}
      rows={(projects.data ?? []).map((project) => [
        project.name,
        <Badge label={project.status} />,
        project.memory_count ?? 0
      ])}
    />
  );
}
