export function getDefaultShell(): string {
  return '/bin/zsh'
}

export function isMac(): boolean {
  return navigator.platform?.toLowerCase().includes('mac') ?? false
}

export function modKey(): string {
  return isMac() ? '⌘' : 'Ctrl'
}
