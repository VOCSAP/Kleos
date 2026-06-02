import { useQueryClient } from '@tanstack/react-query';
import { useMemo, useState } from 'react';
import { listTasks, updateTask } from '$lib/api/chiasm';
import { useLive } from '$lib/realtime';
import type { Task } from '$lib/types';
import { Badge } from '../ui/Badge';
import { EmptyState } from '../ui/EmptyState';
import { Spinner } from '../ui/Spinner';
import { COLUMNS, groupByColumn, type ColumnKey } from './chiasm/board';

// Render the Chiasm task board with filters and drag-to-status updates.
export function Chiasm() {
  const queryClient = useQueryClient();
  const [agent, setAgent] = useState('');
  const [project, setProject] = useState('');
  const queryKey = useMemo(() => ['chiasm', 'tasks', agent, project] as const, [agent, project]);
  const tasks = useLive(queryKey, () => listTasks({ agent: agent || undefined, project: project || undefined }), 'chiasm');
  const groups = groupByColumn(tasks.data ?? []);

  // Move a dragged task into the target board column.
  async function handleDrop(event: React.DragEvent, column: ColumnKey) {
    event.preventDefault();
    const taskId = Number(event.dataTransfer.getData('text/plain'));
    const status = COLUMNS.find((item) => item.key === column)?.setStatus;
    if (!taskId || !status) {
      return;
    }
    await updateTask(taskId, { status });
    queryClient.invalidateQueries({ queryKey });
  }

  return (
    <div data-accent="chiasm">
      <header className="route-header">
        <div>
          <h1>Chiasm</h1>
          <p>Task coordination board</p>
        </div>
        <div className="route-filters">
          <input aria-label="Agent filter" onChange={(event) => setAgent(event.target.value)} placeholder="agent" value={agent} />
          <input
            aria-label="Project filter"
            onChange={(event) => setProject(event.target.value)}
            placeholder="project"
            value={project}
          />
        </div>
      </header>
      {tasks.isLoading ? (
        <Spinner />
      ) : (
        <section className="chiasm-board" aria-label="Task board">
          {COLUMNS.map((column) => (
            <div
              className="chiasm-column"
              key={column.key}
              onDragOver={(event) => event.preventDefault()}
              onDrop={(event) => handleDrop(event, column.key)}
            >
              <div className="chiasm-column__header">
                <span>{column.label}</span>
                <Badge label={String(groups[column.key].length)} />
              </div>
              {groups[column.key].length === 0 ? (
                <EmptyState message="" />
              ) : (
                groups[column.key].map((task) => <TaskCard key={task.id} task={task} />)
              )}
            </div>
          ))}
        </section>
      )}
    </div>
  );
}

// Render a draggable task card for the Chiasm board.
function TaskCard({ task }: { task: Task }) {
  return (
    <article
      className="glass chiasm-card"
      draggable
      onDragStart={(event) => event.dataTransfer.setData('text/plain', String(task.id))}
    >
      <div className="chiasm-card__meta">
        <span>{task.agent}</span>
        <span>{task.project}</span>
      </div>
      <strong>{task.title}</strong>
      {task.summary ? <p>{task.summary}</p> : null}
    </article>
  );
}
