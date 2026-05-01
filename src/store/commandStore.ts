import { create } from 'zustand'
import type { Command, CommandCategory } from '../types/command'

interface CommandPaletteState {
  isOpen: boolean
  query: string
  commands: Command[]
  recentIds: string[]
  open: () => void
  close: () => void
  toggle: () => void
  setQuery: (q: string) => void
  registerCommand: (cmd: Command) => void
  unregisterCommand: (id: string) => void
  registerCommands: (cmds: Command[]) => void
  executeCommand: (id: string) => void
}

const MAX_RECENT = 8

export const useCommandStore = create<CommandPaletteState>((set, get) => ({
  isOpen: false,
  query: '',
  commands: [],
  recentIds: [],

  open: () => set({ isOpen: true, query: '' }),
  close: () => set({ isOpen: false, query: '' }),
  toggle: () => set((s) => ({ isOpen: !s.isOpen, query: s.isOpen ? s.query : '' })),

  setQuery: (q) => set({ query: q }),

  registerCommand: (cmd) =>
    set((s) => {
      if (s.commands.some((c) => c.id === cmd.id)) return s
      return { commands: [...s.commands, cmd] }
    }),

  unregisterCommand: (id) =>
    set((s) => ({
      commands: s.commands.filter((c) => c.id !== id),
      recentIds: s.recentIds.filter((rid) => rid !== id),
    })),

  registerCommands: (cmds) =>
    set((s) => {
      const existing = new Set(s.commands.map((c) => c.id))
      const newCmds = cmds.filter((c) => !existing.has(c.id))
      return { commands: [...s.commands, ...newCmds] }
    }),

  executeCommand: (id) => {
    const cmd = get().commands.find((c) => c.id === id)
    if (!cmd) return
    set((s) => {
      const filtered = s.recentIds.filter((rid) => rid !== id)
      return { recentIds: [id, ...filtered].slice(0, MAX_RECENT) }
    })
    cmd.handler()
    get().close()
  },
}))

export function selectFilteredCommands(
  commands: Command[],
  recentIds: string[],
  query: string,
): { label: string; commands: Command[] }[] {
  const available = commands.filter((c) => (c.when ? c.when() : true))

  if (!query.trim()) {
    const recent = recentIds
      .map((id) => available.find((c) => c.id === id))
      .filter((c): c is Command => !!c)

    const byCategory = groupByCategory(available.filter((c) => !recentIds.includes(c.id)))
    const groups: { label: string; commands: Command[] }[] = []
    if (recent.length > 0) groups.push({ label: 'Recent', commands: recent })
    for (const [cat, cmds] of byCategory) {
      if (cmds.length > 0) groups.push({ label: cat, commands: cmds })
    }
    return groups
  }

  const lower = query.toLowerCase()
  const terms = lower.split(/\s+/).filter(Boolean)

  const scored = available
    .map((cmd) => {
      const labelLower = cmd.label.toLowerCase()
      const descLower = (cmd.description || '').toLowerCase()
      const kwString = (cmd.keywords || []).join(' ').toLowerCase()
      const searchable = `${labelLower} ${descLower} ${kwString}`

      let score = 0

      if (labelLower.startsWith(lower)) {
        score = 10
      } else if (labelLower.includes(lower)) {
        score = 7
      }

      if (score === 0 && terms.length > 1) {
        const allMatch = terms.every(
          (t) => labelLower.includes(t) || descLower.includes(t) || kwString.includes(t),
        )
        if (allMatch) score = 5
      }

      if (score === 0) {
        let qi = 0
        for (let i = 0; i < labelLower.length && qi < lower.length; i++) {
          if (labelLower[i] === lower[qi]) qi++
        }
        if (qi === lower.length) score = 3
      }

      if (score === 0 && (descLower.includes(lower) || kwString.includes(lower))) {
        score = 2
      }

      const recentBoost = recentIds.includes(cmd.id) ? 1 : 0

      return { cmd, score: score + recentBoost }
    })
    .filter(({ score }) => score > 0)
    .sort((a, b) => b.score - a.score || a.cmd.label.localeCompare(b.cmd.label))
    .map(({ cmd }) => cmd)

  return scored.length > 0 ? [{ label: 'Results', commands: scored }] : []
}

const CATEGORY_ORDER: CommandCategory[] = [
  'workspace',
  'panel',
  'athena',
  'terminal',
  'file',
  'navigation',
  'settings',
]

const CATEGORY_LABELS: Record<CommandCategory, string> = {
  workspace: 'Workspace',
  panel: 'Panels',
  athena: 'Athena',
  terminal: 'Terminal',
  file: 'File',
  navigation: 'Navigation',
  settings: 'Settings',
}

function groupByCategory(commands: Command[]): Map<string, Command[]> {
  const map = new Map<string, Command[]>()
  for (const cat of CATEGORY_ORDER) {
    const cmds = commands.filter((c) => c.category === cat)
    if (cmds.length > 0) map.set(CATEGORY_LABELS[cat], cmds)
  }
  for (const cmd of commands) {
    if (!CATEGORY_ORDER.includes(cmd.category)) {
      const label = CATEGORY_LABELS[cmd.category] ?? cmd.category
      if (!map.has(label)) map.set(label, [])
      map.get(label)!.push(cmd)
    }
  }
  return map
}
