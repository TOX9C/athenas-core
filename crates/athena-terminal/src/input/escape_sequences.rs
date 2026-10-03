// Encode keyboard input into ANSI escape sequences for PTY.
// Returns the raw bytes to send to the shell.

/// Encode a regular character with optional modifier keys.
/// For simple ASCII characters with no modifiers, returns the raw byte.
pub fn encode_char(c: char, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    // Shift uppercases ASCII letters (idempotent if caller already supplied
    // the shifted char). Non-letters need layout maps — caller supplies those.
    let c = if shift && !ctrl {
        c.to_ascii_uppercase()
    } else {
        c
    };
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
    // xterm modifier encoding: 1 + shift + 2*alt + 4*ctrl
    // (2=Shift, 3=Alt, 4=Alt+Shift, 5=Ctrl, 6=Ctrl+Shift, 7=Ctrl+Alt, 8=Ctrl+Alt+Shift)
    let modifier = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;
    // Build a modified sequence: ESC [ <num> ; <mod> <final> (or bare when no modifier).
    let mod_seq = |num: &str, final_byte: u8| -> Vec<u8> {
        if modifier == 1 {
            if num == "1" {
                // Arrows/Home/End use the compact form when unmodified.
                return vec![0x1B, b'[', final_byte];
            }
            format!("\x1b[{}{}", num, final_byte as char).into_bytes()
        } else {
            format!("\x1b[{};{}{}", num, modifier, final_byte as char).into_bytes()
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
        "Delete" => Some(mod_seq("3", b'~')),
        "Escape" => Some(vec![0x1B]),
        "Insert" => Some(mod_seq("2", b'~')),
        "Home" => Some(mod_seq("1", b'H')),
        "End" => Some(mod_seq("1", b'F')),
        "PageUp" => Some(mod_seq("5", b'~')),
        "PageDown" => Some(mod_seq("6", b'~')),

        // Arrow keys
        "ArrowUp" => Some(mod_seq("1", b'A')),
        "ArrowDown" => Some(mod_seq("1", b'B')),
        "ArrowRight" => Some(mod_seq("1", b'C')),
        "ArrowLeft" => Some(mod_seq("1", b'D')),

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
///
/// ESC bytes are stripped from the payload: an embedded `\x1b[201~` would
/// otherwise terminate the paste early and execute the remainder as typed
/// input.
pub fn bracketed_paste(data: &str, enabled: bool) -> Vec<u8> {
    if !enabled {
        return data.as_bytes().to_vec();
    }
    let mut result = vec![0x1B, b'[', b'2', b'0', b'0', b'~']; // Start bracketed paste
    result.extend(data.bytes().filter(|&b| b != 0x1B));
    result.extend_from_slice(&[0x1B, b'[', b'2', b'0', b'1', b'~']); // End bracketed paste
    result
}

/// Parse a string of text input, breaking into individual keystrokes.
/// This is a simple parser for text paste operations.
///
/// `\n` maps to CR (0x0D): in a PTY, the "Enter" a shell expects from typed or
/// pasted lines is carriage return, not a bare LF byte.
pub fn parse_text_input(text: &str) -> Vec<Vec<u8>> {
    text.chars()
        .map(|c| {
            if c == '\n' {
                vec![0x0D]
            } else {
                c.to_string().into_bytes()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::encode_char;

    #[test]
    fn shift_uppercases_ascii_letters() {
        assert_eq!(encode_char('a', false, false, true), b"A");
        // Idempotent when caller already supplied the shifted char.
        assert_eq!(encode_char('A', false, false, true), b"A");
        // Shift does not affect ctrl chords.
        assert_eq!(encode_char('a', true, false, true), vec![0x01]);
    }
}
