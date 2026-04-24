interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost'
  size?: 'sm' | 'md'
}

export function Button({ variant = 'primary', size = 'md', children, style, ...props }: ButtonProps) {
  const base = 'rounded-md font-medium transition-colors flex items-center gap-1.5'
  const sizeClass = size === 'sm' ? 'px-2 py-1 text-[11px]' : 'px-3 py-1.5 text-xs'

  const variantStyles: Record<string, React.CSSProperties> = {
    primary: { background: 'var(--accent)', color: '#fff' },
    secondary: { background: 'var(--bgTertiary)', color: 'var(--text)', border: '1px solid var(--border)' },
    ghost: { background: 'transparent', color: 'var(--textMuted)' },
  }

  return (
    <button className={`${base} ${sizeClass}`} style={{ ...variantStyles[variant], ...style }} {...props}>
      {children}
    </button>
  )
}
