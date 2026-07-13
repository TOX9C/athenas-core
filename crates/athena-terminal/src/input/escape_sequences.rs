// Encode keyboard input into ANSI escape sequences for PTY.
// Returns the raw bytes to send to the shell.

/// Encode a regular character with optional modifier keys.
/// For simple ASCII characters with no modifiers, returns the raw byte.
pub fn encode_char(c: char, ctrl: bool, alt: bool, _shift: bool) -> Vec<u8> {
    if ctrl && !alt {
        // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
        let byte = c as u8;
        if byte.is_ascii_lowercase() {
            return vec![byte - b'a' + 1];
        }
        if byte.is_ascii_uppercase() {
            return vec![byte - b'A' + 1];
        }
        if c == ' ' {
            return vec![0x00]; // Ctrl+Space = NUL
        }
        if c == '[' {
            return vec![0x1B]; // Ctrl+[ = ESC
        }
        if c == '\\' {
            return vec![0x1C]; // Ctrl+\ = GS
        }
        if c == ']' {
            return vec![0x1D]; // Ctrl+] = GS
        }
        if c == '^' {
            return vec![0x1E]; // Ctrl+^ = RS
        }
        if c == '_' {
            return vec![0x1F]; // Ctrl+_ = US
        }
    }

    if alt {
        // Alt+key sends ESC prefix
        let mut result = vec![0x1B];
        result.extend(c.encode_utf8(&mut [0; 4]).bytes());
        return result;
    }

    // Regular character
    c.to_string().into_bytes()
}

/// Encode special keys (arrows, function keys, etc.)
pub fn encode_special_key(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<Vec<u8>> {
    // Build modifier suffix for sequences with modifiers
    // Modifier encoding: 1=none, 2=Shift, 3=Alt, 4=Alt+Shift, 5=Ctrl, 6=Ctrl+Shift, 7=Ctrl+Alt, 8=Ctrl+Alt+Shift
    let _mod_suffix = |base: u8| -> String {
        let modifier = if ctrl && shift && alt {
            8
        } else if ctrl && alt {
            7
        } else if ctrl && shift {
            6
        } else if ctrl {
            5
        } else if alt && shift {
            4
        } else if alt {
            3
        } else if shift {
            2
        } else {
            1
        };
        if modifier == 1 {
            base.to_string()
        } else {
            format!(";{}", modifier)
        }
    };

    match key {
        "Enter" => Some(vec![0x0D]),
        "Return" => Some(vec![0x0D]),
        "Tab" => {
            if shift {
                // Shift+Tab = CSI Z
                Some(vec![0x1B, b'[', b'Z'])
            } else {
                Some(vec![0x09])
            }
        }
        "Backspace" => Some(vec![0x7F]),
        "Delete" => Some(vec![0x1B, b'[', b'3', b'~']),
        "Escape" => Some(vec![0x1B]),
        "Insert" => Some(vec![0x1B, b'[', b'2', b'~']),
        "Home" => {
            if ctrl {
                Some(vec![0x1B, b'[', b'1', b';', b'5', b'H'])
            } else {
                Some(vec![0x1B, b'[', b'H'])
            }
        }
        "End" => {
            if ctrl {
                Some(vec![0x1B, b'[', b'1', b';', b'5', b'F'])
            } else {
                Some(vec![0x1B, b'[', b'F'])
            }
        }
        "PageUp" => Some(vec![0x1B, b'[', b'5', b'~']),
        "PageDown" => Some(vec![0x1B, b'[', b'6', b'~']),

        // Arrow keys
        "ArrowUp" => Some(vec![0x1B, b'[', b'A']),
        "ArrowDown" => Some(vec![0x1B, b'[', b'B']),
        "ArrowRight" => Some(vec![0x1B, b'[', b'C']),
        "ArrowLeft" => Some(vec![0x1B, b'[', b'D']),

        // Function keys
        "F1" => Some(vec![0x1B, b'O', b'P']),
        "F2" => Some(vec![0x1B, b'O', b'Q']),
        "F3" => Some(vec![0x1B, b'O', b'R']),
        "F4" => Some(vec![0x1B, b'O', b'S']),
        "F5" => Some(vec![0x1B, b'[', b'1', b'5', b'~']),
        "F6" => Some(vec![0x1B, b'[', b'1', b'7', b'~']),
        "F7" => Some(vec![0x1B, b'[', b'1', b'8', b'~']),
        "F8" => Some(vec![0x1B, b'[', b'1', b'9', b'~']),
        "F9" => Some(vec![0x1B, b'[', b'2', b'0', b'~']),
        "F10" => Some(vec![0x1B, b'[', b'2', b'1', b'~']),
        "F11" => Some(vec![0x1B, b'[', b'2', b'3', b'~']),
        "F12" => Some(vec![0x1B, b'[', b'2', b'4', b'~']),

        _ => None,
    }
}

/// Encode a paste bracket sequence if bracketed paste is enabled.
pub fn bracketed_paste(data: &str, enabled: bool) -> Vec<u8> {
    if !enabled {
        return data.as_bytes().to_vec();
    }
    let mut result = vec![0x1B, b'[', b'2', b'0', b'0', b'~']; // Start bracketed paste
    result.extend(data.as_bytes());
    result.extend_from_slice(&[0x1B, b'[', b'2', b'0', b'1', b'~']); // End bracketed paste
    result
}

/// Parse a string of text input, breaking into individual keystrokes.
/// This is a simple parser for text paste operations.
pub fn parse_text_input(text: &str) -> Vec<Vec<u8>> {
    text.chars().map(|c| c.to_string().into_bytes()).collect()
}
