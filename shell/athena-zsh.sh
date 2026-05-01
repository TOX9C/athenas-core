# Athena Shell Integration for Zsh
# Source this file from your ~/.zshrc:
#   source /path/to/athenas-core/shell/athena-zsh.sh
#
# This emits VS Code-style OSC 633 sequences that Athena's Core
# terminal parses to track commands, CWD, and exit codes.
#
# Alternatively, if ATHENA_SHELL_INTEGRATION=1 is set in your environment
# (Athena sets this automatically for its own PTY sessions), the hooks
# are installed automatically by the terminal itself.

if [[ "$ATHENA_SHELL_INTEGRATION" == "1" ]]; then
  # Already in an Athena PTY — hooks were injected on spawn
  return 0
fi

__athena_osc633() { printf "\e]633;%s\a" "$1"; }

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
