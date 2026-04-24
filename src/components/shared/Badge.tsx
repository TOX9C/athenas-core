interface BadgeProps {
  children: React.ReactNode
  color?: string
}

export function Badge({ children, color = 'var(--accent)' }: BadgeProps) {
  return (
    <span
      className="inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium"
      style={{ background: `${color}22`, color }}
    >
      {children}
    </span>
  )
}
