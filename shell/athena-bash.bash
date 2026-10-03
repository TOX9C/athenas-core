# Athena Shell Integration for Bash
# Source this file from your ~/.bashrc:
#   source /path/to/athenas-core/shell/athena-bash.bash
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
