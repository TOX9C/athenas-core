/// Minimal OSC 133 byte scanner.
///
/// Recognizes sequences of the form:
///   ESC ] 133 ; <type> [ ; <args> ] BEL
///   ESC ] 133 ; <type> [ ; <args> ] ST   (ST = ESC \
///
/// Types:
///   A → PromptStart { cwd: Option<String> }
///   C → CommandStart
///   D → CommandFinished { status: i32 }

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const ST_SECOND: u8 = 0x5c; // '\'

#[derive(Debug, Clone, PartialEq)]
pub enum Osc133Event {
    PromptStart { cwd: Option<String> },
    CommandStart,
    CommandFinished { status: i32 },
}

/// Scan `input` for OSC 133 sequences, strip them out, and return the cleaned
/// bytes together with any parsed events.
///
/// Performs a single pass over the input with no per-byte allocations.
pub fn strip_osc133(input: &[u8]) -> (Vec<u8>, Vec<Osc133Event>) {
    let mut output = Vec::with_capacity(input.len());
    let mut events = Vec::new();
    let mut i = 0;

    while i < input.len() {
        // Fast path: look for OSC 133 prefix: ESC ] 133 ;
        if i + 6 < input.len()
            && input[i] == ESC
            && input[i + 1] == b']'
            && input[i + 2] == b'1'
            && input[i + 3] == b'3'
            && input[i + 4] == b'3'
            && input[i + 5] == b';'
        {
            let type_byte = input[i + 6];
            let mut scan = i + 7;
            let mut body_end = scan;
            let mut terminator_found = false;

            while scan < input.len() {
                if input[scan] == BEL {
                    body_end = scan;
                    terminator_found = true;
                    scan += 1; // consume BEL
                    break;
                }
                if input[scan] == ESC
                    && scan + 1 < input.len()
                    && input[scan + 1] == ST_SECOND
                {
                    body_end = scan;
                    terminator_found = true;
                    scan += 2; // consume ST
                    break;
                }
                scan += 1;
            }

            if terminator_found {
                let body = &input[i + 7..body_end];
                if let Some(ev) = parse_osc133_body(type_byte, body) {
                    events.push(ev);
                }
                i = scan;
                continue;
            }

            // No terminator found — copy prefix byte as literal and continue
            output.push(input[i]);
            i += 1;
        } else {
            output.push(input[i]);
            i += 1;
        }
    }

    (output, events)
}

fn parse_osc133_body(type_byte: u8, body: &[u8]) -> Option<Osc133Event> {
    match type_byte {
        b'A' => {
            let cwd = if body.starts_with(b";;") {
                Some(String::from_utf8_lossy(&body[2..]).to_string())
            } else {
                None
            };
            Some(Osc133Event::PromptStart { cwd })
        }
        b'C' => Some(Osc133Event::CommandStart),
        b'D' => {
            let status = if body.starts_with(b";") {
                parse_i32_bytes(&body[1..])
            } else {
                0
            };
            Some(Osc133Event::CommandFinished { status })
        }
        _ => None,
    }
}

fn parse_i32_bytes(data: &[u8]) -> i32 {
    if data.is_empty() {
        return 0;
    }
    let negative = data[0] == b'-';
    let start = if negative { 1 } else { 0 };
    let mut val: i32 = 0;
    for &b in &data[start..] {
        if b >= b'0' && b <= b'9' {
            val = val * 10 + (b - b'0') as i32;
        } else {
            break;
        }
    }
    if negative {
        -val
    } else {
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_start_bel() {
        let input = vec![ESC, b']', b'1', b'3', b'3', b';', b'A', BEL];
        let (cleaned, events) = strip_osc133(&input);
        assert!(cleaned.is_empty());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], Osc133Event::PromptStart { cwd: None });
    }

    #[test]
    fn test_command_start_bel() {
        let input = vec![ESC, b']', b'1', b'3', b'3', b';', b'C', BEL];
        let (cleaned, events) = strip_osc133(&input);
        assert!(cleaned.is_empty());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], Osc133Event::CommandStart);
    }

    #[test]
    fn test_command_finished_bel() {
        let input = vec![
            ESC, b']', b'1', b'3', b'3', b';', b'D', b';', b'0', BEL,
        ];
        let (cleaned, events) = strip_osc133(&input);
        assert!(cleaned.is_empty());
        assert_eq!(events.len(), 1);
        assert_eq!(events, vec![Osc133Event::CommandFinished { status: 0 }]);
    }

    #[test]
    fn test_prompt_with_cwd() {
        let mut input = vec![ESC, b']', b'1', b'3', b'3', b';', b'A', b';', b';'];
        input.extend_from_slice(b"/home/user");
        input.push(BEL);
        let (cleaned, events) = strip_osc133(&input);
        assert!(cleaned.is_empty());
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            Osc133Event::PromptStart {
                cwd: Some("/home/user".to_string()),
            }
        );
    }

    #[test]
    fn test_st_terminator() {
        let input = vec![
            ESC, b']', b'1', b'3', b'3', b';', b'D', b';', b'1', ESC, ST_SECOND,
        ];
        let (cleaned, events) = strip_osc133(&input);
        assert!(cleaned.is_empty());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], Osc133Event::CommandFinished { status: 1 });
    }

    #[test]
    fn test_mixed_content() {
        let mut input = b"Hello ".to_vec();
        input.extend_from_slice(&[ESC, b']', b'1', b'3', b'3', b';', b'A', BEL]);
        input.extend_from_slice(b" World");

        let (cleaned, events) = strip_osc133(&input);
        assert_eq!(cleaned, b"Hello  World".to_vec());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0], Osc133Event::PromptStart { cwd: None });
    }

    #[test]
    fn test_no_sequences() {
        let input = b"just some normal text\nwith newlines".to_vec();
        let (cleaned, events) = strip_osc133(&input);
        assert_eq!(cleaned, input);
        assert!(events.is_empty());
    }

    #[test]
    fn test_negative_exit_status() {
        let mut input = vec![
            ESC, b']', b'1', b'3', b'3', b';', b'D', b';',
        ];
        input.extend_from_slice(b"-1");
        input.push(BEL);
        let (_, events) = strip_osc133(&input);
        assert_eq!(events[0], Osc133Event::CommandFinished { status: -1 });
    }
}
