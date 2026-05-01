# Athena Shell Integration for Bash
# Source this file from your ~/.bashrc:
#   source /path/to/athenas-core/shell/athena-bash.bash
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

__athena_prompt_command() {
  local __athena_exit="$?"
  if [[ -n $__athena_si_last_cmd ]]; then
    __athena_osc633 "D;$__athena_exit"
    __athena_si_last_cmd=""
  fi
  __athena_osc633 A
  __athena_osc633 "P;$PWD"
}

__athena_debug_trap() {
  if [[ -n $__athena_si_last_cmd ]]; then
    return
  fi
  local __athena_cmd="$BASH_COMMAND"
  if [[ "$__athena_cmd" != "__athena_prompt_command" && "$__athena_cmd" != *"__athena_osc633"* ]]; then
    __athena_si_last_cmd="$__athena_cmd"
    __athena_osc633 "B;$__athena_cmd"
    __athena_osc633 C
    __athena_osc633 E
  fi
}

trap "__athena_debug_trap" DEBUG
PROMPT_COMMAND="__athena_prompt_command; $PROMPT_COMMAND"

__athena_osc633 "Set=shellIntegration=bash"

# Emit initial CWD and prompt marker
__athena_osc633 "P;$PWD"
__athena_osc633 A
