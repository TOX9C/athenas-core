interface WorkspaceTabProps {
  name: string
  color: string
  active: boolean
  onClick: () => void
  onClose: () => void
}

export function WorkspaceTab({ name, color, active, onClick, onClose }: WorkspaceTabProps) {
  return (
    <div
      onClick={onClick}
      className="group flex items-center gap-1.5 px-3 py-1 rounded-md cursor-pointer transition-all shrink-0"
      style={{
        background: active ? 'var(--bgTertiary)' : 'transparent',
      }}
    >
      <div className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: color }} />
      <span
        className="text-[11px] truncate max-w-[100px]"
        style={{ color: active ? 'var(--text)' : 'var(--textMuted)' }}
      >
        {name}
      </span>
    </div>
  )
}
