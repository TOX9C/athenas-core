# Athena Shell Integration for Fish
# Source this file from your ~/.config/fish/config.fish:
#   source /path/to/athenas-core/shell/athena-fish.fish
#
# This emits VS Code-style OSC 633 sequences that Athena's Core
# terminal parses to track commands, CWD, and exit codes.

if set -q __ATHENA_SOURCED
  exit 0
end
set -g __ATHENA_SOURCED 1

if test "$ATHENA_SHELL_INTEGRATION" = "1"
  # Already in an Athena PTY — hooks were injected on spawn
  exit 0
end

function __athena_osc633 -d "Emit OSC 633 sequence"
  printf "\e]633;%s\a" $argv
end

function __athena_prompt_start --on-event fish_prompt
  __athena_osc633 A
  __athena_osc633 "P;(pwd)"
end

function __athena_preexec --on-event fish_preexec
  __athena_osc633 "B;$argv"
  __athena_osc633 C
  __athena_osc633 E
end

function __athena_postexec --on-event fish_postexec -a __athena_exit
  __athena_osc633 "D;$__athena_exit"
end

__athena_osc633 "Set=shellIntegration=fish"

# Emit initial CWD and prompt marker
__athena_osc633 "P;(pwd)"
__athena_osc633 A
