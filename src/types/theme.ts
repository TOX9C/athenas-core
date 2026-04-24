export type ThemeName =
  | 'void' | 'ghost' | 'plasma' | 'carbon' | 'hex'
  | 'neon-tokyo' | 'obsidian' | 'nebula' | 'storm' | 'infrared'
  | 'nova' | 'stealth' | 'hologram' | 'dracula' | 'athena'
  | 'synthwave' | 'cybernetics' | 'quantum' | 'mecha' | 'abyss'
  | 'paper' | 'chalk' | 'solar' | 'arctic' | 'ivory'

export interface ThemeDefinition {
  name: ThemeName
  label: string
  type: 'dark' | 'light'
  colors: {
    bg: string
    bgSecondary: string
    bgTertiary: string
    border: string
    text: string
    textMuted: string
    textDim: string
    accent: string
    accentHover: string
    success: string
    error: string
    warning: string
    terminalBg: string
    terminalFg: string
    terminalCursor: string
    terminalSelection: string
  }
}
