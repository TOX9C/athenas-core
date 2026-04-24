/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        bg: 'var(--bg)',
        'bg-secondary': 'var(--bgSecondary)',
        'bg-tertiary': 'var(--bgTertiary)',
        border: 'var(--border)',
        text: 'var(--text)',
        'text-muted': 'var(--textMuted)',
        'text-dim': 'var(--textDim)',
        accent: 'var(--accent)',
        'accent-hover': 'var(--accentHover)',
        success: 'var(--success)',
        error: 'var(--error)',
        warning: 'var(--warning)',
      },
      fontFamily: {
        mono: ["'JetBrains Mono'", "'Fira Code'", 'monospace'],
      },
    },
  },
  plugins: [],
}
