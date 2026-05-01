import { useDroppable } from '@dnd-kit/core'
import { SortableContext, verticalListSortingStrategy } from '@dnd-kit/sortable'
import { Plus } from 'lucide-react'
import type { KanbanTask, KanbanStatus } from '../../types/task'
import { KanbanCard } from './KanbanCard'
import { useState } from 'react'

const STATUS_LABELS: Record<KanbanStatus, string> = {
  todo: 'Todo',
  in_progress: 'In Progress',
  in_review: 'In Review',
  complete: 'Complete',
}

const STATUS_COLORS: Record<KanbanStatus, string> = {
  todo: 'var(--textDim)',
  in_progress: 'var(--accent)',
  in_review: 'var(--warning)',
  complete: 'var(--success)',
}

interface KanbanColumnProps {
  status: KanbanStatus
  tasks: KanbanTask[]
  onAddTask: (title: string) => void
  onUpdateTask: (id: string, updates: Partial<KanbanTask>) => void
  onDeleteTask: (id: string) => void
  onRunTask?: (task: KanbanTask) => void
}

export function KanbanColumn({
  status,
  tasks,
  onAddTask,
  onUpdateTask,
  onDeleteTask,
  onRunTask,
}: KanbanColumnProps) {
  const [adding, setAdding] = useState(false)
  const [newTitle, setNewTitle] = useState('')

  const { setNodeRef, isOver } = useDroppable({ id: status })

  const handleAdd = () => {
    if (newTitle.trim()) {
      onAddTask(newTitle.trim())
      setNewTitle('')
      setAdding(false)
    }
  }

  return (
    <div
      ref={setNodeRef}
      className="flex flex-col rounded-lg min-w-[220px] max-w-[280px] flex-1"
      style={{
        background: isOver ? 'var(--bgTertiary)' : 'var(--bg)',
        border: '1px solid var(--border)',
        transition: 'background 150ms',
      }}
    >
      <div
        className="flex items-center justify-between px-3 py-2 border-b"
        style={{ borderColor: 'var(--border)' }}
      >
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full" style={{ background: STATUS_COLORS[status] }} />
          <span className="text-[11px] font-semibold" style={{ color: 'var(--text)' }}>
            {STATUS_LABELS[status]}
          </span>
          <span
            className="text-[10px] px-1.5 py-0.5 rounded-full"
            style={{ background: 'var(--bgTertiary)', color: 'var(--textDim)' }}
          >
            {tasks.length}
          </span>
        </div>
        {status === 'todo' && (
          <button
            onClick={() => setAdding(true)}
            className="p-0.5 rounded hover:bg-white/10 transition-colors"
          >
            <Plus size={13} style={{ color: 'var(--textMuted)' }} />
          </button>
        )}
      </div>

      <div className="flex-1 overflow-y-auto p-1.5 flex flex-col gap-1.5">
        <SortableContext items={tasks.map((t) => t.id)} strategy={verticalListSortingStrategy}>
          {tasks.map((task) => (
            <KanbanCard
              key={task.id}
              task={task}
              onUpdate={(updates) => onUpdateTask(task.id, updates)}
              onDelete={() => onDeleteTask(task.id)}
              onRunTask={onRunTask ? () => onRunTask(task) : undefined}
            />
          ))}
        </SortableContext>

        {adding && (
          <div
            className="p-2 rounded-md"
            style={{ background: 'var(--bgSecondary)', border: '1px solid var(--border)' }}
          >
            <input
              autoFocus
              value={newTitle}
              onChange={(e) => setNewTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleAdd()
                if (e.key === 'Escape') {
                  setAdding(false)
                  setNewTitle('')
                }
              }}
              onBlur={() => {
                if (!newTitle.trim()) setAdding(false)
              }}
              placeholder="Task title..."
              className="w-full bg-transparent text-xs outline-none"
              style={{ color: 'var(--text)' }}
            />
          </div>
        )}
      </div>
    </div>
  )
}
