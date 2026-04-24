import type { Space, GridTemplate } from '../../types/workspace'
import { TerminalPane } from './TerminalPane'

const GRID_LAYOUT: Record<GridTemplate, { cols: number; rows: number }> = {
  '1x1': { cols: 1, rows: 1 },
  '1x2': { cols: 2, rows: 1 },
  '2x2': { cols: 2, rows: 2 },
  '2x3': { cols: 3, rows: 2 },
  '3x3': { cols: 3, rows: 3 },
  '3x4': { cols: 4, rows: 3 },
  '4x4': { cols: 4, rows: 4 },
}

interface TerminalGridProps {
  space: Space
}

export function TerminalGrid({ space }: TerminalGridProps) {
  const layout = GRID_LAYOUT[space.grid]

  return (
    <div
      className="flex-1 grid gap-1 p-1 min-h-0"
      style={{
        gridTemplateColumns: `repeat(${layout.cols}, 1fr)`,
        gridTemplateRows: `repeat(${layout.rows}, 1fr)`,
      }}
    >
      {space.panes.map((pane) => (
        <TerminalPane key={pane.id} pane={pane} cwd={space.dir} />
      ))}
    </div>
  )
}
