import type { GridTemplate } from '../../types/workspace'

const GRIDS: { template: GridTemplate; label: string; cols: number; rows: number }[] = [
  { template: '1x1', label: 'Solo', cols: 1, rows: 1 },
  { template: '1x2', label: 'Split', cols: 2, rows: 1 },
  { template: '2x2', label: 'Quad', cols: 2, rows: 2 },
  { template: '2x3', label: 'Six', cols: 3, rows: 2 },
  { template: '3x3', label: 'Nine', cols: 3, rows: 3 },
  { template: '3x4', label: 'Twelve', cols: 4, rows: 3 },
  { template: '4x4', label: 'Sixteen', cols: 4, rows: 4 },
]

interface GridTemplateSelectorProps {
  selected: GridTemplate
  onSelect: (g: GridTemplate) => void
}

export function GridTemplateSelector({ selected, onSelect }: GridTemplateSelectorProps) {
  return (
    <div className="flex flex-col gap-3">
      <label className="text-xs font-medium" style={{ color: 'var(--textMuted)' }}>
        Choose layout
      </label>
      <div className="grid grid-cols-4 gap-3">
        {GRIDS.map(({ template, label, cols, rows }) => (
          <button
            key={template}
            onClick={() => onSelect(template)}
            className="flex flex-col items-center gap-2 p-3 rounded-lg transition-all"
            style={{
              background: selected === template ? 'var(--accent)' : 'var(--bg)',
              border: `1px solid ${selected === template ? 'var(--accent)' : 'var(--border)'}`,
            }}
            onMouseEnter={(e) => {
              if (selected !== template) e.currentTarget.style.borderColor = 'var(--textDim)'
            }}
            onMouseLeave={(e) => {
              if (selected !== template) e.currentTarget.style.borderColor = 'var(--border)'
            }}
          >
            <svg width={48} height={36} viewBox="0 0 48 36">
              {Array.from({ length: rows }).map((_, r) =>
                Array.from({ length: cols }).map((_, c) => {
                  const gap = 2
                  const w = (48 - gap * (cols - 1)) / cols
                  const h = (36 - gap * (rows - 1)) / rows
                  return (
                    <rect
                      key={`${r}-${c}`}
                      x={c * (w + gap)}
                      y={r * (h + gap)}
                      width={w}
                      height={h}
                      rx={2}
                      fill={selected === template ? 'rgba(255,255,255,0.3)' : 'var(--bgTertiary)'}
                    />
                  )
                })
              )}
            </svg>
            <div className="flex flex-col items-center">
              <span
                className="text-[11px] font-medium"
                style={{ color: selected === template ? '#fff' : 'var(--text)' }}
              >
                {label}
              </span>
              <span
                className="text-[10px]"
                style={{ color: selected === template ? 'rgba(255,255,255,0.7)' : 'var(--textDim)' }}
              >
                {template}
              </span>
            </div>
          </button>
        ))}
      </div>
    </div>
  )
}
