# Athena Shell Integration for Zsh
# Source this file from your ~/.zshrc:
#   source /path/to/athenas-core/shell/athena-zsh.sh
#
# This emits VS Code-style OSC 633 sequences that Athena's Core
# terminal parses to track commands, CWD, and exit codes.
#
# Athena also injects this exact file into its own PTY sessions on spawn.
# The guard below makes double-sourcing (injected + manual) a no-op.

if [[ -n "$__ATHENA_SOURCED" ]]; then
  return 0
fi
__ATHENA_SOURCED=1

# OSC payloads are delimited by ESC ] ... BEL; $PWD and command lines can
# contain BEL/ESC/other C0 controls, which would break the framing (or
# inject hostile sequences). Strip them before emission.
__athena_osc633() {
  local __athena_payload
  __athena_payload="$(printf %s "$1" | LC_ALL=C tr -d '\000-\037\177')"
  printf "\e]633;%s\a" "$__athena_payload"
}

__athena_precmd() {
  local __athena_exit=$?
  if [[ -n $__athena_si_last_cmd ]]; then
    __athena_osc633 "D;$__athena_exit"
    __athena_si_last_cmd=""
  fi
  __athena_osc633 A
  __athena_osc633 "P;$PWD"
}

__athena_preexec() {
  __athena_si_last_cmd="$3"
  __athena_osc633 "B;$3"
  __athena_osc633 C
  __athena_osc633 E
}

autoload -Uz add-zsh-hook 2>/dev/null
add-zsh-hook precmd __athena_precmd 2>/dev/null
add-zsh-hook preexec __athena_preexec 2>/dev/null

__athena_osc633 "Set=shellIntegration=zsh"

# Emit initial CWD and prompt marker
__athena_osc633 "P;$PWD"
__athena_osc633 A
