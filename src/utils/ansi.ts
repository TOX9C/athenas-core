// Comprehensive ANSI escape code stripper for terminal output
// Handles: CSI (SGR, cursor, erase, scroll), OSC (title, hyperlink),
// control chars (BEL, BS, CR, ESC), and other terminal sequences

const ANSI_PATTERN = [
  // OSC sequences: \x1b]...<BEL or ST(\x1b\\)>
  '(?:\x1b\\]|x1b\\])[^\x07\x1b]*(?:\x07|\x1b\\\\)',
  // CSI sequences: \x1b[<params><final-byte>
  '\x1b\\[[\x30-\x3f]*[\x20-\x2f]*[\x40-\x7e]',
  // Simple ESC sequences: \x1b<single-byte>
  '\x1b[\x40-\x5a\x5c-\x7e]',
  // SS2/SS3: \x1bN or \x1bO
  '\x1b[N-O]',
  // Backspace and overstrike patterns (spinner artifacts)
  '[\x08]\x1b\\[K',
  // Stray control characters
  '[\x00-\x08\x0b\x0c\x0e-\x1a]',
].join('|')

const ANSI_REGEX = new RegExp(ANSI_PATTERN, 'g')

// Clean up common terminal output artifacts after ANSI removal
const EXTRA_CLEANUP = [
  [/\r\n/g, '\n'], // normalize line endings
  [/\r/g, ''], // strip lone CR
  [/\x08+/g, ''], // strip leftover backspaces
  [/\n{3,}/g, '\n\n'], // collapse excessive blank lines
]

export function stripAnsi(str: string): string {
  // Convert Cursor Forward (\x1b[<n>C) to literal spaces to prevent smushing text
  let clean = str.replace(/\x1b\[(\d*)C/g, (_, n) => ' '.repeat(parseInt(n || '1', 10)))

  clean = clean.replace(ANSI_REGEX, '')

  for (const [pattern, replacement] of EXTRA_CLEANUP) {
    clean = clean.replace(pattern as RegExp, replacement as string)
  }
  return clean
}
