import type { LucideIcon } from 'lucide-react'

export type CommandCategory =
  | 'workspace'
  | 'panel'
  | 'athena'
  | 'terminal'
  | 'file'
  | 'settings'
  | 'navigation'

export interface Command {
  id: string
  label: string
  category: CommandCategory
  description?: string
  keywords?: string[]
  icon?: LucideIcon
  shortcut?: string
  handler: () => void
  when?: () => boolean
}
