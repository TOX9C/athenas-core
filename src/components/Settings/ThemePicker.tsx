import { themes } from '../../themes/themes'
import { useUIStore } from '../../store/uiStore'
import { applyTheme } from '../../themes/themes'
import type { ThemeName } from '../../types/theme'

export function ThemePicker() {
  const { theme, setTheme } = useUIStore()

  const handleSelect = (name: ThemeName) => {
    setTheme(name)
    applyTheme(themes[name])
  }

  return (
    <div className="grid grid-cols-5 gap-2">
      {Object.values(themes).map((t) => (
        <button
          key={t.name}
          onClick={() => handleSelect(t.name)}
          className="p-2 rounded-lg flex flex-col items-center gap-1"
          style={{
            background: t.colors.bg,
            border: theme === t.name ? `2px solid ${t.colors.accent}` : '2px solid transparent',
          }}
        >
          <div className="w-full h-6 rounded" style={{ background: t.colors.bgSecondary }} />
          <span className="text-[9px]" style={{ color: t.colors.text }}>
            {t.label}
          </span>
        </button>
      ))}
    </div>
  )
}
