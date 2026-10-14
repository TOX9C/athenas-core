#![no_main]

use athena_core::shell_integration::Osc633Parser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The parser consumes raw terminal text byte-by-byte across reads; ensure
    // no panics, infinite loops, or pathological buffer growth in arbitrary
    // byte prefixes/splits.
    let mut parser = Osc633Parser::new();
    if let Ok(s) = std::str::from_utf8(data) {
        for chunk in s.as_bytes().chunks(23).filter_map(|c| std::str::from_utf8(c).ok()) {
            let _ = parser.feed(chunk);
        }
    }
});
