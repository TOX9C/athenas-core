import { useState } from 'react'
import { GripVertical, Play, Trash2, Bot } from 'lucide-react'
import type { KanbanTask } from '../../types/task'
import { getAgentLabel, getAgentColor } from '../../utils/agentCommands'
import { useSortable } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'

interface KanbanCardProps {
  task: KanbanTask
  onUpdate: (updates: Partial<KanbanTask>) => void
  onDelete: () => void
  onRunTask?: () => void
}

export function KanbanCard({ task, onUpdate, onDelete, onRunTask }: KanbanCardProps) {
  const [editing, setEditing] = useState(false)
  const [editTitle, setEditTitle] = useState(task.title)

  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: task.id,
    data: { task },
  })

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.5 : 1,
  }

  const handleTitleSubmit = () => {
    if (editTitle.trim()) {
      onUpdate({ title: editTitle.trim() })
    }
    setEditing(false)
  }

  return (
    <div ref={setNodeRef} style={style} {...attributes} {...listeners}>
      <div
        className="group rounded-md p-2.5 transition-colors cursor-grab active:cursor-grabbing"
        style={{
          background: 'var(--bgSecondary)',
          border: '1px solid var(--border)',
        }}
      >
        <div className="flex items-start gap-1.5">
          <GripVertical
            size={12}
            className="mt-0.5 shrink-0 opacity-30 group-hover:opacity-60"
            style={{ color: 'var(--textDim)' }}
          />

          <div className="flex-1 min-w-0">
            {editing ? (
              <input
                autoFocus
                value={editTitle}
                onChange={(e) => setEditTitle(e.target.value)}
                onBlur={handleTitleSubmit}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') handleTitleSubmit()
                  if (e.key === 'Escape') {
                    setEditTitle(task.title)
                    setEditing(false)
                  }
                }}
                className="w-full bg-transparent text-xs outline-none"
                style={{ color: 'var(--text)' }}
              />
            ) : (
              <span
                className="text-xs leading-snug cursor-text"
                style={{ color: 'var(--text)' }}
                onDoubleClick={() => setEditing(true)}
              >
                {task.title}
              </span>
            )}

            {task.description && (
              <p
                className="text-[10px] mt-1 leading-relaxed line-clamp-2"
                style={{ color: 'var(--textDim)' }}
              >
                {task.description}
              </p>
            )}
          </div>
        </div>

        <div className="flex items-center justify-between mt-2">
          <div className="flex items-center gap-1">
            {task.assignedAgent && (
              <span
                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[9px] font-medium"
                style={{
                  background: `${getAgentColor(task.assignedAgent)}20`,
                  color: getAgentColor(task.assignedAgent),
                }}
              >
                <Bot size={8} />
                {getAgentLabel(task.assignedAgent)}
              </span>
            )}
          </div>

          <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
            {onRunTask && task.status === 'todo' && (
              <button
                onClick={(e) => {
                  e.stopPropagation()
                  onRunTask()
                }}
                className="p-1 rounded hover:bg-white/10 transition-colors"
                title="Run task"
              >
                <Play size={10} style={{ color: 'var(--success)' }} />
              </button>
            )}
            <button
              onClick={(e) => {
                e.stopPropagation()
                onDelete()
              }}
              className="p-1 rounded hover:bg-white/10 transition-colors"
              title="Delete"
            >
              <Trash2 size={10} style={{ color: 'var(--error)' }} />
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
