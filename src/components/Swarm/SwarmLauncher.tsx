import { Users } from 'lucide-react'

interface SwarmLauncherProps {
  onLaunch: () => void
}

export function SwarmLauncher({ onLaunch }: SwarmLauncherProps) {
  return (
    <button
      onClick={onLaunch}
      className="flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[11px] font-medium transition-colors"
      style={{
        background: 'var(--bgTertiary)',
        color: 'var(--textMuted)',
        border: '1px solid var(--border)',
      }}
      onMouseEnter={(e) => (e.currentTarget.style.borderColor = 'var(--accent)')}
      onMouseLeave={(e) => (e.currentTarget.style.borderColor = 'var(--border)')}
    >
      <Users size={12} />
      Swarm
    </button>
  )
}
