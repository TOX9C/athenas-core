import { create } from 'zustand'
import type { KanbanTask, KanbanStatus } from '../types/task'

const MAX_TASKS = 200

interface TaskState {
  tasks: KanbanTask[]
  addTask: (task: KanbanTask) => void
  removeTask: (id: string) => void
  updateTask: (id: string, updates: Partial<KanbanTask>) => void
  moveTask: (id: string, status: KanbanStatus) => void
  setTasks: (tasks: KanbanTask[]) => void
}

function persistTasks(tasks: KanbanTask[]) {
  window.athena.store.set('tasks', tasks)
}

export const useTaskStore = create<TaskState>((set, get) => ({
  tasks: [],
  addTask: (task) => {
    set((s) => ({ tasks: [...s.tasks, task].slice(-MAX_TASKS) }))
    persistTasks(get().tasks)
  },
  removeTask: (id) => {
    set((s) => ({ tasks: s.tasks.filter((t) => t.id !== id) }))
    persistTasks(get().tasks)
  },
  updateTask: (id, updates) => {
    set((s) => ({
      tasks: s.tasks.map((t) => (t.id === id ? { ...t, ...updates } : t)),
    }))
    persistTasks(get().tasks)
  },
  moveTask: (id, status) => {
    set((s) => ({
      tasks: s.tasks.map((t) => (t.id === id ? { ...t, status } : t)),
    }))
    persistTasks(get().tasks)
  },
  setTasks: (tasks) => set({ tasks: tasks.slice(-MAX_TASKS) }),
}))
