# Athena Shell Integration for Fish
# Source this file from your ~/.config/fish/config.fish:
#   source /path/to/athenas-core/shell/athena-fish.fish
#
# This emits VS Code-style OSC 633 sequences that Athena's Core
# terminal parses to track commands, CWD, and exit codes.
#
# Athena also injects this exact file into its own PTY sessions on spawn.
#
# NOTE: a top-level `exit` here would kill the user's shell (this file is
# sourced, not executed), so the body is wrapped in a guard block instead.

if not set -q __ATHENA_SOURCED
  set -g __ATHENA_SOURCED 1

  # OSC payloads are delimited by ESC ] ... BEL; $PWD and command lines can
  # contain BEL/ESC/other C0 controls, which would break the framing (or
  # inject hostile sequences). Strip them before emission.
  function __athena_osc633 -d "Emit OSC 633 sequence"
    set -l payload (printf %s "$argv[1]" | LC_ALL=C tr -d '\000-\037\177')
    printf "\e]633;%s\a" "$payload"
  end

  function __athena_prompt_start --on-event fish_prompt
    __athena_osc633 A
    __athena_osc633 "P;$PWD"
  end

  function __athena_preexec --on-event fish_preexec
    __athena_osc633 "B;$argv[1]"
    __athena_osc633 C
    __athena_osc633 E
  end

  # fish_postexec's argv[1] is the command line, NOT the exit status;
  # read $status (captured first, before anything can clobber it).
  function __athena_postexec --on-event fish_postexec
    set -l exit_code $status
    __athena_osc633 "D;$exit_code"
  end

  __athena_osc633 "Set=shellIntegration=fish"

  # Emit initial CWD and prompt marker
  __athena_osc633 "P;$PWD"
  __athena_osc633 A
end
