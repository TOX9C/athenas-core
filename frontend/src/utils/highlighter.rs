/// Escape HTML entities in a string.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Highlight source code based on language.
pub fn highlight_code(code: &str, language: &str) -> String {
    match language.to_lowercase().as_str() {
        "rust" => highlight_rust(code),
        "typescript" | "ts" => highlight_typescript(code),
        "javascript" | "js" | "jsx" | "tsx" => highlight_javascript(code),
        "python" | "py" => highlight_python(code),
        "toml" => highlight_toml(code),
        "json" => highlight_json(code),
        "css" | "scss" | "less" => highlight_css(code),
        "html" | "xml" | "svg" => highlight_html(code),
        "bash" | "sh" | "shell" | "zsh" => highlight_bash(code),
        _ => {
            // Fallback: just escape HTML
            escape_html(code)
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    format!(
                        "<div class=\"code-line\"><span class=\"line-number\">{:>4}</span> {}</div>",
                        i + 1,
                        line
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

// ---------------------------------------------------------------------------
// Rust highlighter
// ---------------------------------------------------------------------------

fn highlight_rust(code: &str) -> String {
    let escaped = escape_html(code);

    // Process line by line to handle multi-line comments
    let mut result = String::new();
    let mut in_block_comment = false;

    for line in escaped.lines() {
        let highlighted = highlight_rust_line(line, &mut in_block_comment);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_rust_line(line: &str, in_block_comment: &mut bool) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Check for line number prefix (format: "   N ")
    let mut line_num_end = 0;
    if len >= 6 {
        let mut all_space_or_digit = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                all_space_or_digit = false;
                break;
            }
        }
        if all_space_or_digit && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    while j < rest_len {
        // Block comment end
        if *in_block_comment {
            if j + 1 < rest_len && rest_chars[j] == '*' && rest_chars[j + 1] == '/' {
                output.push_str("<span class=\"token-comment\">*/</span>");
                j += 2;
                *in_block_comment = false;
            } else {
                output.push(rest_chars[j]);
                j += 1;
            }
            continue;
        }

        // Block comment start
        if j + 1 < rest_len && rest_chars[j] == '/' && rest_chars[j + 1] == '*' {
            output.push_str("<span class=\"token-comment\">/*");
            j += 2;
            *in_block_comment = true;
            // Continue to capture rest of line in comment
            while j < rest_len {
                if j + 1 < rest_len && rest_chars[j] == '*' && rest_chars[j + 1] == '/' {
                    output.push_str("*/</span>");
                    j += 2;
                    *in_block_comment = false;
                    break;
                }
                output.push(rest_chars[j]);
                j += 1;
            }
            continue;
        }

        // Line comment
        if j + 1 < rest_len && rest_chars[j] == '/' && rest_chars[j + 1] == '/' {
            output.push_str("<span class=\"token-comment\">");
            while j < rest_len {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            break;
        }

        // String literal
        if rest_chars[j] == '"' {
            output.push_str("<span class=\"token-string\">\"");
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == '"' {
                    output.push('"');
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Char literal
        if rest_chars[j] == '\'' {
            output.push_str("<span class=\"token-string\">'");
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == '\'' {
                    output.push('\'');
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Raw string r#"..."#
        if rest_chars[j] == 'r' && j + 1 < rest_len && rest_chars[j + 1] == '#' {
            let mut k = j + 1;
            let mut hashes = 0;
            while k < rest_len && rest_chars[k] == '#' {
                hashes += 1;
                k += 1;
            }
            if k < rest_len && rest_chars[k] == '"' {
                output.push_str("<span class=\"token-string\">");
                for _ in 0..=hashes {
                    output.push(rest_chars[j]);
                    j += 1;
                }
                output.push('"');
                j += 1;
                let mut found = false;
                while j < rest_len {
                    if rest_chars[j] == '"' {
                        let mut h = 0;
                        let mut m = j + 1;
                        while m < rest_len && rest_chars[m] == '#' && h < hashes {
                            h += 1;
                            m += 1;
                        }
                        if h == hashes {
                            for _ in 0..=hashes {
                                output.push('"');
                                j += 1;
                            }
                            for _ in 0..hashes {
                                output.push('#');
                                j += 1;
                            }
                            found = true;
                            break;
                        }
                    }
                    output.push(rest_chars[j]);
                    j += 1;
                }
                if !found {
                    while j < rest_len {
                        output.push(rest_chars[j]);
                        j += 1;
                    }
                }
                output.push_str("</span>");
                continue;
            }
        }

        // Lifetime
        if rest_chars[j] == '\'' && j + 1 < rest_len && rest_chars[j + 1].is_alphabetic() {
            output.push_str("<span class=\"token-lifetime\">'");
            j += 1;
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Number
        if rest_chars[j].is_ascii_digit()
            || (rest_chars[j] == '-' && j + 1 < rest_len && rest_chars[j + 1].is_ascii_digit())
        {
            if rest_chars[j] == '-' {
                output.push('-');
                j += 1;
            }
            output.push_str("<span class=\"token-number\">");
            while j < rest_len && (rest_chars[j].is_ascii_digit() || rest_chars[j] == '_' || rest_chars[j] == '.') {
                output.push(rest_chars[j]);
                j += 1;
            }
            // Handle type suffix
            if j < rest_len && rest_chars[j].is_alphabetic() {
                while j < rest_len && rest_chars[j].is_alphabetic() {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Identifier or keyword
        if rest_chars[j].is_alphabetic() || rest_chars[j] == '_' {
            let mut word = String::new();
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_') {
                word.push(rest_chars[j]);
                j += 1;
            }

            if is_rust_keyword(&word) {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else if is_rust_type(&word) {
                output.push_str(&format!("<span class=\"token-type\">{}</span>", word));
            } else if is_rust_bool(&word) {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else if is_rust_macro(&word) {
                output.push_str(&format!("<span class=\"token-function\">{}</span>", word));
            } else if j < rest_len && rest_chars[j] == '!' {
                output.push_str(&format!("<span class=\"token-function\">{}!</span>", word));
                j += 1;
            } else {
                output.push_str(&word);
            }
            continue;
        }

        // Operators
        if "=&lt;&gt;+-*/%&amp;|^!~?:".contains(rest_chars[j])
            || (rest_chars[j] == '-' && j + 1 < rest_len && rest_chars[j + 1] == '>')
        {
            output.push_str("<span class=\"token-operator\">");
            output.push(rest_chars[j]);
            j += 1;
            if j < rest_len && "=&lt;&gt;".contains(rest_chars[j]) {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Default
        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

fn is_rust_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "let" | "mut" | "pub" | "struct" | "impl" | "enum" | "match"
            | "if" | "else" | "loop" | "for" | "while" | "return" | "use"
            | "mod" | "crate" | "where" | "type" | "const" | "static" | "async"
            | "await" | "move" | "ref" | "dyn" | "trait" | "unsafe" | "extern"
            | "in" | "as" | "break" | "continue" | "try" | "self" | "super"
    )
}

fn is_rust_type(word: &str) -> bool {
    matches!(
        word,
        "String" | "Vec" | "Option" | "Result" | "HashMap" | "HashSet"
            | "Box" | "Rc" | "Arc" | "Cell" | "RefCell" | "Cow"
            | "Some" | "None" | "Ok" | "Err" | "True" | "False"
            | "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
            | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
            | "f32" | "f64" | "bool" | "char" | "str"
    )
}

fn is_rust_bool(word: &str) -> bool {
    matches!(word, "true" | "false")
}

fn is_rust_macro(word: &str) -> bool {
    word.ends_with('!')
        && matches!(
            word.trim_end_matches('!'),
            "println" | "eprintln" | "print" | "eprint" | "format"
                | "vec" | "dbg" | "todo" | "unimplemented" | "panic"
                | "assert" | "assert_eq" | "assert_ne" | "debug_assert"
                | "write" | "writeln" | "include" | "include_str"
                | "include_bytes" | "env" | "option_env" | "concat"
                | "stringify" | "file" | "line" | "column" | "cfg"
                | "derive" | "rsx"
        )
}

// ---------------------------------------------------------------------------
// TypeScript / JavaScript highlighter
// ---------------------------------------------------------------------------

fn highlight_typescript(code: &str) -> String {
    highlight_javascript_like(code, true)
}

fn highlight_javascript(code: &str) -> String {
    highlight_javascript_like(code, false)
}

fn highlight_javascript_like(code: &str, is_ts: bool) -> String {
    let escaped = escape_html(code);
    let mut result = String::new();
    let mut in_block_comment = false;
    let mut in_template = false;

    for line in escaped.lines() {
        let highlighted = highlight_js_line(line, &mut in_block_comment, &mut in_template, is_ts);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_js_line(line: &str, in_block_comment: &mut bool, in_template: &mut bool, is_ts: bool) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Line number prefix
    let mut line_num_end = 0;
    if len >= 6 {
        let mut ok = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                ok = false;
                break;
            }
        }
        if ok && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    while j < rest_len {
        if *in_block_comment {
            if j + 1 < rest_len && rest_chars[j] == '*' && rest_chars[j + 1] == '/' {
                output.push_str("<span class=\"token-comment\">*/</span>");
                j += 2;
                *in_block_comment = false;
            } else {
                output.push(rest_chars[j]);
                j += 1;
            }
            continue;
        }

        if j + 1 < rest_len && rest_chars[j] == '/' && rest_chars[j + 1] == '*' {
            output.push_str("<span class=\"token-comment\">/*");
            j += 2;
            *in_block_comment = true;
            while j < rest_len {
                if j + 1 < rest_len && rest_chars[j] == '*' && rest_chars[j + 1] == '/' {
                    output.push_str("*/</span>");
                    j += 2;
                    *in_block_comment = false;
                    break;
                }
                output.push(rest_chars[j]);
                j += 1;
            }
            continue;
        }

        if j + 1 < rest_len && rest_chars[j] == '/' && rest_chars[j + 1] == '/' {
            output.push_str("<span class=\"token-comment\">");
            while j < rest_len {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            break;
        }

        if rest_chars[j] == '"' || rest_chars[j] == '\'' {
            let quote = rest_chars[j];
            output.push_str("<span class=\"token-string\">");
            output.push(quote);
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == quote {
                    output.push(quote);
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Template literal
        if rest_chars[j] == '`' {
            *in_template = !*in_template;
            output.push_str("<span class=\"token-string\">`</span>");
            j += 1;
            continue;
        }

        if *in_template {
            output.push(rest_chars[j]);
            j += 1;
            continue;
        }

        // Number
        if rest_chars[j].is_ascii_digit()
            || (rest_chars[j] == '.' && j + 1 < rest_len && rest_chars[j + 1].is_ascii_digit())
        {
            output.push_str("<span class=\"token-number\">");
            while j < rest_len && (rest_chars[j].is_ascii_digit() || rest_chars[j] == '.' || rest_chars[j] == '_') {
                output.push(rest_chars[j]);
                j += 1;
            }
            if j < rest_len && (rest_chars[j] == 'e' || rest_chars[j] == 'E') {
                output.push(rest_chars[j]);
                j += 1;
                if j < rest_len && (rest_chars[j] == '+' || rest_chars[j] == '-') {
                    output.push(rest_chars[j]);
                    j += 1;
                }
                while j < rest_len && rest_chars[j].is_ascii_digit() {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Identifier/keyword
        if rest_chars[j].is_alphabetic() || rest_chars[j] == '_' || rest_chars[j] == '$' {
            let mut word = String::new();
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_' || rest_chars[j] == '$') {
                word.push(rest_chars[j]);
                j += 1;
            }

            if is_js_keyword(&word, is_ts) {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else if is_js_builtin(&word) {
                output.push_str(&format!("<span class=\"token-type\">{}</span>", word));
            } else if is_js_bool(&word) {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else {
                output.push_str(&word);
            }
            continue;
        }

        // Operators
        if "=<>+-*/%&|^!~?:".contains(rest_chars[j]) {
            output.push_str("<span class=\"token-operator\">");
            output.push(rest_chars[j]);
            j += 1;
            if j < rest_len && "=<>".contains(rest_chars[j]) {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

fn is_js_keyword(word: &str, is_ts: bool) -> bool {
    let common = matches!(
        word,
        "const" | "let" | "var" | "function" | "return" | "if" | "else"
            | "for" | "while" | "do" | "switch" | "case" | "break"
            | "continue" | "new" | "this" | "class" | "extends"
            | "super" | "import" | "export" | "from" | "default"
            | "try" | "catch" | "finally" | "throw" | "typeof"
            | "instanceof" | "in" | "of" | "yield" | "async"
            | "await" | "delete" | "void" | "with" | "debugger"
    );
    if !is_ts {
        return common;
    }
    common
        || matches!(
            word,
            "type" | "interface" | "enum" | "implements" | "namespace"
                | "declare" | "abstract" | "readonly" | "keyof"
                | "satisfies" | "as" | "is" | "override"
        )
}

fn is_js_builtin(word: &str) -> bool {
    matches!(
        word,
        "console" | "document" | "window" | "Math" | "JSON" | "Array"
            | "Object" | "String" | "Number" | "Boolean" | "Date"
            | "RegExp" | "Error" | "Map" | "Set" | "WeakMap"
            | "WeakSet" | "Promise" | "Symbol" | "Proxy" | "Reflect"
            | "Iterator" | "Generator" | "Int8Array" | "Uint8Array"
            | "Float32Array" | "Float64Array" | "undefined" | "null"
            | "NaN" | "Infinity"
    )
}

fn is_js_bool(word: &str) -> bool {
    matches!(word, "true" | "false")
}

// ---------------------------------------------------------------------------
// Python highlighter
// ---------------------------------------------------------------------------

fn highlight_python(code: &str) -> String {
    let escaped = escape_html(code);
    let mut result = String::new();

    for line in escaped.lines() {
        let highlighted = highlight_python_line(line);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_python_line(line: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Line number prefix
    let mut line_num_end = 0;
    if len >= 6 {
        let mut ok = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                ok = false;
                break;
            }
        }
        if ok && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    while j < rest_len {
        // Comment
        if rest_chars[j] == '#' {
            output.push_str("<span class=\"token-comment\">");
            while j < rest_len {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            break;
        }

        // Triple-quoted string
        if j + 2 < rest_len
            && (rest_chars[j] == rest_chars[j + 1] && rest_chars[j + 1] == rest_chars[j + 2])
            && (rest_chars[j] == '"' || rest_chars[j] == '\'')
        {
            let quote = rest_chars[j];
            output.push_str("<span class=\"token-string\">");
            output.push(quote);
            output.push(quote);
            output.push(quote);
            j += 3;
            while j + 2 < rest_len {
                if rest_chars[j] == quote && rest_chars[j + 1] == quote && rest_chars[j + 2] == quote {
                    output.push(quote);
                    output.push(quote);
                    output.push(quote);
                    j += 3;
                    break;
                }
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // String
        if rest_chars[j] == '"' || rest_chars[j] == '\'' {
            let quote = rest_chars[j];
            output.push_str("<span class=\"token-string\">");
            output.push(quote);
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == quote {
                    output.push(quote);
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Decorator
        if rest_chars[j] == '@' && j == 0 {
            output.push_str("<span class=\"token-function\">@");
            j += 1;
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_' || rest_chars[j] == '.') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Number
        if rest_chars[j].is_ascii_digit() {
            output.push_str("<span class=\"token-number\">");
            while j < rest_len && (rest_chars[j].is_ascii_digit() || rest_chars[j] == '.' || rest_chars[j] == '_') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Identifier/keyword
        if rest_chars[j].is_alphabetic() || rest_chars[j] == '_' {
            let mut word = String::new();
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_') {
                word.push(rest_chars[j]);
                j += 1;
            }

            if is_python_keyword(&word) {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else if is_python_builtin(&word) {
                output.push_str(&format!("<span class=\"token-type\">{}</span>", word));
            } else if is_python_bool_none(&word) {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else {
                output.push_str(&word);
            }
            continue;
        }

        // Operators
        if "=<>+-*/%&|^!~:@".contains(rest_chars[j]) {
            output.push_str("<span class=\"token-operator\">");
            output.push(rest_chars[j]);
            j += 1;
            if j < rest_len && "=<>".contains(rest_chars[j]) {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

fn is_python_keyword(word: &str) -> bool {
    matches!(
        word,
        "def" | "class" | "return" | "if" | "elif" | "else" | "for"
            | "while" | "break" | "continue" | "pass" | "import"
            | "from" | "as" | "with" | "try" | "except" | "finally"
            | "raise" | "yield" | "lambda" | "global" | "nonlocal"
            | "assert" | "del" | "in" | "not" | "and" | "or"
            | "is" | "async" | "await"
    )
}

fn is_python_builtin(word: &str) -> bool {
    matches!(
        word,
        "print" | "len" | "range" | "str" | "int" | "float" | "bool"
            | "list" | "dict" | "set" | "tuple" | "type" | "isinstance"
            | "issubclass" | "hasattr" | "getattr" | "setattr" | "super"
            | "property" | "staticmethod" | "classmethod" | "open"
            | "enumerate" | "zip" | "map" | "filter" | "sorted"
            | "reversed" | "any" | "all" | "sum" | "min" | "max"
            | "abs" | "round" | "format" | "input" | "self"
    )
}

fn is_python_bool_none(word: &str) -> bool {
    matches!(word, "True" | "False" | "None")
}

// ---------------------------------------------------------------------------
// TOML highlighter
// ---------------------------------------------------------------------------

fn highlight_toml(code: &str) -> String {
    let escaped = escape_html(code);
    let mut result = String::new();

    for line in escaped.lines() {
        let highlighted = highlight_toml_line(line);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_toml_line(line: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Line number prefix
    let mut line_num_end = 0;
    if len >= 6 {
        let mut ok = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                ok = false;
                break;
            }
        }
        if ok && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    // Table header [section] or [[array]]
    if rest_len > 0 && rest_chars[0] == '[' {
        output.push_str("<span class=\"token-type\">");
        while j < rest_len {
            output.push(rest_chars[j]);
            j += 1;
        }
        output.push_str("</span>");
        return format!("<div class=\"code-line\">{}</div>", output);
    }

    while j < rest_len {
        // Comment
        if rest_chars[j] == '#' {
            output.push_str("<span class=\"token-comment\">");
            while j < rest_len {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            break;
        }

        // String
        if rest_chars[j] == '"' || rest_chars[j] == '\'' {
            let quote = rest_chars[j];
            output.push_str("<span class=\"token-string\">");
            output.push(quote);
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == quote {
                    output.push(quote);
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Key (before =)
        if rest_chars[j].is_alphabetic() || rest_chars[j] == '_' || rest_chars[j] == '-' {
            let mut word = String::new();
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_' || rest_chars[j] == '-' || rest_chars[j] == '.') {
                word.push(rest_chars[j]);
                j += 1;
            }
            // Check if followed by = (it's a key)
            let trimmed = &rest[j..].trim_start();
            if trimmed.starts_with('=') {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else {
                output.push_str(&word);
            }
            continue;
        }

        // Number or boolean
        if rest_chars[j].is_ascii_digit() || rest_chars[j] == '-' {
            output.push_str("<span class=\"token-number\">");
            while j < rest_len && (rest_chars[j].is_ascii_digit() || rest_chars[j] == '.' || rest_chars[j] == '_' || rest_chars[j] == '-' || rest_chars[j] == '+' || rest_chars[j] == 'e' || rest_chars[j] == 'E') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Boolean
        if j + 3 <= rest_len {
            let substr: String = rest_chars[j..].iter().take(4).collect();
            if substr == "true" || substr == "True" {
                output.push_str("<span class=\"token-keyword\">true</span>");
                j += 4;
                continue;
            }
        }
        if j + 4 <= rest_len {
            let substr: String = rest_chars[j..].iter().take(5).collect();
            if substr == "false" || substr == "False" {
                output.push_str("<span class=\"token-keyword\">false</span>");
                j += 5;
                continue;
            }
        }

        // Equals
        if rest_chars[j] == '=' {
            output.push_str("<span class=\"token-operator\">=</span>");
            j += 1;
            continue;
        }

        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

// ---------------------------------------------------------------------------
// JSON highlighter
// ---------------------------------------------------------------------------

fn highlight_json(code: &str) -> String {
    let escaped = escape_html(code);
    let mut result = String::new();

    for line in escaped.lines() {
        let highlighted = highlight_json_line(line);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_json_line(line: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Line number prefix
    let mut line_num_end = 0;
    if len >= 6 {
        let mut ok = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                ok = false;
                break;
            }
        }
        if ok && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    while j < rest_len {
        // String
        if rest_chars[j] == '"' {
            output.push_str("<span class=\"token-string\">\"");
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == '"' {
                    output.push('"');
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            // Check if this key (followed by :)
            let remaining: String = rest_chars[j..].iter().collect();
            if remaining.trim_start().starts_with(':') {
                // It was a key, re-wrap as keyword
                // Actually let's just leave it - strings in JSON are all the same color
            }
            continue;
        }

        // Number
        if rest_chars[j].is_ascii_digit() || rest_chars[j] == '-' {
            output.push_str("<span class=\"token-number\">");
            while j < rest_len && (rest_chars[j].is_ascii_digit() || rest_chars[j] == '.' || rest_chars[j] == 'e' || rest_chars[j] == 'E' || rest_chars[j] == '+' || rest_chars[j] == '-') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Boolean/null
        if j + 3 <= rest_len {
            let substr: String = rest_chars[j..].iter().take(4).collect();
            if substr == "true" {
                output.push_str("<span class=\"token-keyword\">true</span>");
                j += 4;
                continue;
            }
        }
        if j + 4 <= rest_len {
            let substr: String = rest_chars[j..].iter().take(5).collect();
            if substr == "false" {
                output.push_str("<span class=\"token-keyword\">false</span>");
                j += 5;
                continue;
            }
        }
        if j + 3 <= rest_len {
            let substr: String = rest_chars[j..].iter().take(4).collect();
            if substr == "null" {
                output.push_str("<span class=\"token-keyword\">null</span>");
                j += 4;
                continue;
            }
        }

        // Braces/brackets
        if "{}[],:".contains(rest_chars[j]) {
            output.push_str("<span class=\"token-operator\">");
            output.push(rest_chars[j]);
            j += 1;
            output.push_str("</span>");
            continue;
        }

        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

// ---------------------------------------------------------------------------
// CSS highlighter
// ---------------------------------------------------------------------------

fn highlight_css(code: &str) -> String {
    let escaped = escape_html(code);
    let mut result = String::new();

    for line in escaped.lines() {
        let highlighted = highlight_css_line(line);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_css_line(line: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Line number prefix
    let mut line_num_end = 0;
    if len >= 6 {
        let mut ok = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                ok = false;
                break;
            }
        }
        if ok && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    // Comment
    if j + 1 < rest_len && rest_chars[j] == '/' && rest_chars[j + 1] == '*' {
        output.push_str("<span class=\"token-comment\">");
        while j < rest_len {
            output.push(rest_chars[j]);
            j += 1;
        }
        output.push_str("</span>");
        return format!("<div class=\"code-line\">{}</div>", output);
    }

    while j < rest_len {
        // Comment
        if j + 1 < rest_len && rest_chars[j] == '/' && rest_chars[j + 1] == '*' {
            output.push_str("<span class=\"token-comment\">");
            while j < rest_len {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // String
        if rest_chars[j] == '"' || rest_chars[j] == '\'' {
            let quote = rest_chars[j];
            output.push_str("<span class=\"token-string\">");
            output.push(quote);
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == quote {
                    output.push(quote);
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Property name (word followed by :)
        if rest_chars[j].is_alphabetic() || rest_chars[j] == '-' {
            let mut word = String::new();
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '-' || rest_chars[j] == '_') {
                word.push(rest_chars[j]);
                j += 1;
            }
            let remaining: String = rest_chars[j..].iter().collect();
            if remaining.trim_start().starts_with(':') {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else {
                output.push_str(&word);
            }
            continue;
        }

        // Number with unit
        if rest_chars[j].is_ascii_digit() || (rest_chars[j] == '-' && j + 1 < rest_len && rest_chars[j + 1].is_ascii_digit()) {
            if rest_chars[j] == '-' {
                output.push('-');
                j += 1;
            }
            output.push_str("<span class=\"token-number\">");
            while j < rest_len && (rest_chars[j].is_ascii_digit() || rest_chars[j] == '.') {
                output.push(rest_chars[j]);
                j += 1;
            }
            // Unit
            if j < rest_len && rest_chars[j].is_alphabetic() {
                while j < rest_len && rest_chars[j].is_alphabetic() {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            } else if j < rest_len && rest_chars[j] == '%' {
                output.push('%');
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // @-rule
        if rest_chars[j] == '@' {
            output.push_str("<span class=\"token-keyword\">@");
            j += 1;
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '-' || rest_chars[j] == '_') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Selector pseudo-class/element
        if rest_chars[j] == ':' {
            output.push_str("<span class=\"token-function\">:");
            j += 1;
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '-' || rest_chars[j] == '_') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // # color / id
        if rest_chars[j] == '#' {
            output.push_str("<span class=\"token-number\">#");
            j += 1;
            while j < rest_len && rest_chars[j].is_alphanumeric() {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // !important
        if rest_chars[j] == '!' {
            output.push_str("<span class=\"token-keyword\">");
            while j < rest_len && rest_chars[j].is_alphabetic() {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Braces
        if "{}();,".contains(rest_chars[j]) {
            output.push_str("<span class=\"token-operator\">");
            output.push(rest_chars[j]);
            j += 1;
            output.push_str("</span>");
            continue;
        }

        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

// ---------------------------------------------------------------------------
// HTML highlighter
// ---------------------------------------------------------------------------

fn highlight_html(code: &str) -> String {
    let escaped = escape_html(code);
    let mut result = String::new();

    for line in escaped.lines() {
        let highlighted = highlight_html_line(line);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_html_line(line: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Line number prefix
    let mut line_num_end = 0;
    if len >= 6 {
        let mut ok = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                ok = false;
                break;
            }
        }
        if ok && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    // Comment <!-- -->
    if j + 3 < rest_len
        && rest_chars[j] == '<'
        && rest_chars[j + 1] == '!'
        && rest_chars[j + 2] == '-'
        && rest_chars[j + 3] == '-'
    {
        output.push_str("<span class=\"token-comment\">&lt;!--");
        j += 4;
        while j + 2 < rest_len {
            if rest_chars[j] == '-' && rest_chars[j + 1] == '-' && rest_chars[j + 2] == '>' {
                output.push_str("--&gt;</span>");
                break;
            }
            output.push(rest_chars[j]);
            j += 1;
        }
        return format!("<div class=\"code-line\">{}</div>", output);
    }

    while j < rest_len {
        // Tag opening
        if rest_chars[j] == '<' && j + 1 < rest_len && (rest_chars[j + 1].is_alphabetic() || rest_chars[j + 1] == '/' || rest_chars[j + 1] == '!') {
            output.push_str("<span class=\"token-function\">&lt;");
            j += 1;
            if rest_chars[j] == '/' {
                output.push_str("/");
                j += 1;
            }
            // Tag name
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '-') {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            // Attributes
            while j < rest_len && rest_chars[j] != '>' {
                if rest_chars[j] == '"' || rest_chars[j] == '\'' {
                    let quote = rest_chars[j];
                    output.push_str("<span class=\"token-string\">");
                    output.push(quote);
                    j += 1;
                    while j < rest_len && rest_chars[j] != quote {
                        output.push(rest_chars[j]);
                        j += 1;
                    }
                    if j < rest_len {
                        output.push(quote);
                        j += 1;
                    }
                    output.push_str("</span>");
                } else if rest_chars[j].is_alphabetic() {
                    output.push_str("<span class=\"token-keyword\">");
                    while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '-' || rest_chars[j] == '_' || rest_chars[j] == ':') {
                        output.push(rest_chars[j]);
                        j += 1;
                    }
                    output.push_str("</span>");
                } else if rest_chars[j] == '=' {
                    output.push_str("<span class=\"token-operator\">=</span>");
                    j += 1;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            if j < rest_len {
                output.push_str("<span class=\"token-function\">");
                if j + 1 < rest_len && rest_chars[j + 1] == '/' {
                    output.push_str("&gt;");
                    j += 2;
                } else {
                    output.push_str("&gt;");
                    j += 1;
                }
                output.push_str("</span>");
            }
            continue;
        }

        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

// ---------------------------------------------------------------------------
// Bash highlighter
// ---------------------------------------------------------------------------

fn highlight_bash(code: &str) -> String {
    let escaped = escape_html(code);
    let mut result = String::new();

    for line in escaped.lines() {
        let highlighted = highlight_bash_line(line);
        result.push_str(&highlighted);
        result.push('\n');
    }

    result
}

fn highlight_bash_line(line: &str) -> String {
    let mut output = String::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();

    // Line number prefix
    let mut line_num_end = 0;
    if len >= 6 {
        let mut ok = true;
        for j in 0..5 {
            if !chars[j].is_ascii_digit() && chars[j] != ' ' {
                ok = false;
                break;
            }
        }
        if ok && chars[5] == ' ' {
            line_num_end = 6;
            output.push_str(&line[..line_num_end]);
        }
    }

    let rest = &line[line_num_end..];
    let rest_chars: Vec<char> = rest.chars().collect();
    let rest_len = rest_chars.len();
    let mut j = 0;

    while j < rest_len {
        // Comment
        if rest_chars[j] == '#' {
            output.push_str("<span class=\"token-comment\">");
            while j < rest_len {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            break;
        }

        // String
        if rest_chars[j] == '"' || rest_chars[j] == '\'' {
            let quote = rest_chars[j];
            output.push_str("<span class=\"token-string\">");
            output.push(quote);
            j += 1;
            while j < rest_len {
                if rest_chars[j] == '\\' && j + 1 < rest_len {
                    output.push(rest_chars[j]);
                    j += 1;
                    output.push(rest_chars[j]);
                    j += 1;
                } else if rest_chars[j] == quote {
                    output.push(quote);
                    j += 1;
                    break;
                } else {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Variable
        if rest_chars[j] == '$' {
            output.push_str("<span class=\"token-keyword\">$");
            j += 1;
            if j < rest_len && rest_chars[j] == '{' {
                output.push('{');
                j += 1;
                while j < rest_len && rest_chars[j] != '}' {
                    output.push(rest_chars[j]);
                    j += 1;
                }
                if j < rest_len {
                    output.push('}');
                    j += 1;
                }
            } else {
                while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_') {
                    output.push(rest_chars[j]);
                    j += 1;
                }
            }
            output.push_str("</span>");
            continue;
        }

        // Number
        if rest_chars[j].is_ascii_digit() {
            output.push_str("<span class=\"token-number\">");
            while j < rest_len && rest_chars[j].is_ascii_digit() {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        // Keyword
        if rest_chars[j].is_alphabetic() || rest_chars[j] == '_' {
            let mut word = String::new();
            while j < rest_len && (rest_chars[j].is_alphanumeric() || rest_chars[j] == '_' || rest_chars[j] == '-') {
                word.push(rest_chars[j]);
                j += 1;
            }

            if is_bash_keyword(&word) {
                output.push_str(&format!("<span class=\"token-keyword\">{}</span>", word));
            } else {
                output.push_str(&word);
            }
            continue;
        }

        // Operators
        if "=<>+-*/%&|^!~;".contains(rest_chars[j]) {
            output.push_str("<span class=\"token-operator\">");
            output.push(rest_chars[j]);
            j += 1;
            if j < rest_len && "=<>|&".contains(rest_chars[j]) {
                output.push(rest_chars[j]);
                j += 1;
            }
            output.push_str("</span>");
            continue;
        }

        output.push(rest_chars[j]);
        j += 1;
    }

    format!("<div class=\"code-line\">{}</div>", output)
}

fn is_bash_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "then" | "else" | "elif" | "fi" | "for" | "while" | "do"
            | "done" | "case" | "esac" | "function" | "return" | "exit"
            | "echo" | "export" | "source" | "alias" | "unset"
            | "local" | "readonly" | "shift" | "set" | "eval"
            | "exec" | "trap" | "wait" | "test" | "true" | "false"
            | "in" | "select" | "until"
    )
}
