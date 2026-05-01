import { useCallback } from 'react'
import {
  DndContext,
  DragEndEvent,
  DragOverEvent,
  closestCorners,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import { nanoid } from 'nanoid'
import { useTaskStore } from '../../store/taskStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { KanbanColumn } from './KanbanColumn'
import type { KanbanStatus, KanbanTask } from '../../types/task'
import { ClipboardList } from 'lucide-react'

const COLUMNS: KanbanStatus[] = ['todo', 'in_progress', 'in_review', 'complete']

export function KanbanBoard() {
  const { tasks, addTask, removeTask, updateTask, moveTask } = useTaskStore()
  const activeSpaceId = useWorkspaceStore((s) => s.activeSpaceId)
  const spaces = useWorkspaceStore((s) => s.spaces)
  const activeSpace = spaces.find((s) => s.id === activeSpaceId)

  const spaceTasks = tasks.filter((t) => t.spaceId === activeSpaceId)

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 5 } }))

  const handleAddTask = useCallback(
    (title: string) => {
      if (!activeSpaceId) return
      const task: KanbanTask = {
        id: nanoid(),
        spaceId: activeSpaceId,
        title,
        status: 'todo',
        order: spaceTasks.filter((t) => t.status === 'todo').length,
        createdAt: Date.now(),
      }
      addTask(task)
    },
    [activeSpaceId, spaceTasks, addTask],
  )

  const handleRunTask = useCallback(
    (task: KanbanTask) => {
      if (!activeSpace) return
      const targetPane = task.assignedAgent
        ? activeSpace.panes.find((p) => p.agentType === task.assignedAgent)
        : activeSpace.panes[0]
      if (!targetPane) return

      const prompt = task.description ? `${task.title}\n\n${task.description}\n` : `${task.title}\n`
      window.athena.pty.write(targetPane.id, prompt)
      moveTask(task.id, 'in_progress')
    },
    [activeSpace, moveTask],
  )

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event
      if (!over) return

      const overId = over.id as string
      const isColumn = COLUMNS.includes(overId as KanbanStatus)

      if (isColumn) {
        moveTask(active.id as string, overId as KanbanStatus)
      } else {
        const overTask = spaceTasks.find((t) => t.id === overId)
        if (overTask) {
          moveTask(active.id as string, overTask.status)
        }
      }
    },
    [moveTask, spaceTasks],
  )

  const handleDragOver = useCallback(
    (event: DragOverEvent) => {
      const { active, over } = event
      if (!over) return

      const overId = over.id as string
      const isColumn = COLUMNS.includes(overId as KanbanStatus)

      if (isColumn) {
        moveTask(active.id as string, overId as KanbanStatus)
      } else {
        const overTask = spaceTasks.find((t) => t.id === overId)
        if (overTask) {
          moveTask(active.id as string, overTask.status)
        }
      }
    },
    [moveTask, spaceTasks],
  )

  if (!activeSpaceId) {
    return (
      <div className="flex-1 h-full w-full flex items-center justify-center">
        <div className="flex flex-col items-center gap-2" style={{ color: 'var(--textDim)' }}>
          <ClipboardList size={32} style={{ opacity: 0.3 }} />
          <span className="text-xs">Select a workspace to use Kanban</span>
        </div>
      </div>
    )
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCorners}
      onDragEnd={handleDragEnd}
      onDragOver={handleDragOver}
    >
      <div className="flex-1 h-full w-full flex gap-3 p-3 overflow-x-auto min-h-0">
        {COLUMNS.map((status) => (
          <KanbanColumn
            key={status}
            status={status}
            tasks={spaceTasks.filter((t) => t.status === status).sort((a, b) => a.order - b.order)}
            onAddTask={handleAddTask}
            onUpdateTask={updateTask}
            onDeleteTask={removeTask}
            onRunTask={handleRunTask}
          />
        ))}
      </div>
    </DndContext>
  )
}
