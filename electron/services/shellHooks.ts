import { platform } from 'os'

export function getShellIntegrationScript(shell: string): string {
  const base = shell.split('/').pop() || 'zsh'

  switch (base) {
    case 'zsh':
      return getZshIntegration()
    case 'bash':
      return getBashIntegration()
    case 'fish':
      return getFishIntegration()
    default:
      return getZshIntegration()
  }
}

function getZshIntegration(): string {
  return [
    '__athena_si_enabled=1',
    '',
    '__athena_osc633() { printf "\\e]633;%s\\a" "$1"; }',
    '',
    '__athena_precmd() {',
    '  local __athena_exit=$?',
    '  if [[ -n $__athena_si_last_cmd ]]; then',
    '    __athena_osc633 "D;$__athena_exit"',
    '    __athena_si_last_cmd=""',
    '  fi',
    '  __athena_osc633 A',
    '  __athena_osc633 "P;$PWD"',
    '}',
    '',
    '__athena_preexec() {',
    '  __athena_si_last_cmd="$3"',
    '  __athena_osc633 "B;$3"',
    '  __athena_osc633 C',
    '  __athena_osc633 E',
    '}',
    '',
    'autoload -Uz add-zsh-hook 2>/dev/null',
    'add-zsh-hook precmd __athena_precmd 2>/dev/null',
    'add-zsh-hook preexec __athena_preexec 2>/dev/null',
    '',
    '__athena_osc633 "Set=shellIntegration=zsh"',
  ].join('\n')
}

function getBashIntegration(): string {
  return [
    '__athena_si_enabled=1',
    '',
    '__athena_osc633() { printf "\\e]633;%s\\a" "$1"; }',
    '',
    '__athena_prompt_command() {',
    '  local __athena_exit="$?"',
    '  if [[ -n $__athena_si_last_cmd ]]; then',
    '    __athena_osc633 "D;$__athena_exit"',
    '    __athena_si_last_cmd=""',
    '  fi',
    '  __athena_osc633 A',
    '  __athena_osc633 "P;$PWD"',
    '}',
    '',
    '__athena_debug_trap() {',
    '  if [[ -n $__athena_si_last_cmd ]]; then',
    '    return',
    '  fi',
    '  local __athena_cmd="$BASH_COMMAND"',
    '  if [[ "$__athena_cmd" != "__athena_prompt_command" && "$__athena_cmd" != *"__athena_osc633"* ]]; then',
    '    __athena_si_last_cmd="$__athena_cmd"',
    '    __athena_osc633 "B;$__athena_cmd"',
    '    __athena_osc633 C',
    '    __athena_osc633 E',
    '  fi',
    '}',
    '',
    'trap "__athena_debug_trap" DEBUG',
    'PROMPT_COMMAND="__athena_prompt_command; $PROMPT_COMMAND"',
    '',
    '__athena_osc633 "Set=shellIntegration=bash"',
  ].join('\n')
}

function getFishIntegration(): string {
  return [
    'set -g __athena_si_enabled 1',
    '',
    'function __athena_osc633 -d "Emit OSC 633 sequence"',
    '  printf "\\e]633;%s\\a" $argv',
    'end',
    '',
    'function __athena_prompt_start --on-event fish_prompt',
    '  __athena_osc633 A',
    '  __athena_osc633 "P;(pwd)"',
    'end',
    '',
    'function __athena_preexec --on-event fish_preexec',
    '  __athena_osc633 "B;$argv"',
    '  __athena_osc633 C',
    '  __athena_osc633 E',
    'end',
    '',
    'function __athena_postexec --on-event fish_postexec -a __athena_exit',
    '  __athena_osc633 "D;$__athena_exit"',
    'end',
    '',
    '__athena_osc633 "Set=shellIntegration=fish"',
  ].join('\n')
}

export function isShellIntegrationCompatible(shell: string): boolean {
  if (platform() === 'win32') return false
  const base = shell.split('/').pop() || ''
  return ['zsh', 'bash', 'fish', 'sh'].includes(base)
}

export function buildShellIntegrationEnv(shell: string): Record<string, string> {
  return {
    ATHENA_SHELL_INTEGRATION: '1',
    ATHENA_TERM: 'athena-core',
  }
}
