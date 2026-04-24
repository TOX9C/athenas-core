const extensionMap: Record<string, string> = {
  ts: '🔷', tsx: '⚛️', js: '🟡', jsx: '⚛️',
  json: '📋', md: '📝', css: '🎨', scss: '🎨',
  html: '🌐', svg: '🖼️', png: '🖼️', jpg: '🖼️',
  yml: '⚙️', yaml: '⚙️', toml: '⚙️',
  sh: '🐚', bash: '🐚', zsh: '🐚',
  py: '🐍', go: '🔵', rs: '🦀', rb: '💎',
  lock: '🔒', gitignore: '👁️',
}

export function getFileIcon(filename: string): string {
  const ext = filename.split('.').pop()?.toLowerCase() ?? ''
  return extensionMap[ext] ?? '📄'
}
