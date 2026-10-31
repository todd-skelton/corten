//! A script's comments, removed from what the browser receives (the macro still reads the JSDoc
//! from the file). Each line keeps its number, so DevTools and a stack trace point at the line
//! the author wrote (a column after a comment inside a line moves: the comment is not padded, to
//! keep the bytes). A licence (`/*! .. */`, `//! ..`, or a comment with
//! `@license` or `@preserve`) stays, as minifiers keep it, and so does a comment the browser
//! reads (`//# sourceMappingURL=..`, `//# sourceURL=..`).

/// Whether the token before a `/` ends an expression (so the `/` divides) or not (so it starts a
/// regular expression).
#[derive(Clone, Copy, PartialEq)]
enum Before {
    Value,
    Operator,
}

/// What an open bracket was, to know what its closing one ends.
#[derive(Clone, Copy, PartialEq)]
enum Open {
    /// `{` of a block or an object.
    Brace,
    /// `${` of a template literal: its `}` goes back into the template.
    Substitution,
    /// `(` of `if`, `while`, `for` or `with`: what follows its `)` is a statement.
    Condition,
    /// Any other `(`, or a `[`.
    Group,
}

/// Words after which a `/` starts a regular expression. Each is reserved, so none can be a
/// variable that is divided (`of`, `let` and `async` can, and are left out).
const BEFORE_REGEX: &[&str] = &[
    "return",
    "typeof",
    "instanceof",
    "in",
    "new",
    "delete",
    "void",
    "throw",
    "case",
    "default",
    "do",
    "else",
    "yield",
    "await",
];

/// `source` without its comments: a line comment is removed to its line's end, a block comment
/// becomes the line breaks it held, or a space when it held none and code follows it. Strings,
/// template literals and regular expressions are copied as they are. A script with a `/` only a
/// parser could read is returned as it is.
pub fn strip(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut at = 0;
    let mut copied = 0;
    let mut before = Before::Operator;
    let mut opens: Vec<Open> = Vec::new();
    // Set by `if`, `while`, `for` or `with`, for the `(` that follows.
    let mut condition = false;
    // Whether the last token was `.`.
    let mut dot = false;
    // Whether the last token was a block's `}` or a `++`/`--`, after which a `/` may divide or
    // start a regular expression and only a parser could say which: the script is then served as
    // it is.
    let mut unsure = false;
    // Whether a line ended since the last token: a `/` starting a line after a value may begin a
    // regular expression (`continue\n/re/`, `let t\n/re/`, where the line's end ends the
    // statement) or divide (`a\n/ 2`), and only a parser could say which.
    let mut new_line = false;
    // Set when a template literal's text is being copied (at its start or after a `}`).
    let mut in_template = false;
    while at < bytes.len() {
        if in_template {
            let closed;
            (at, closed) = template(bytes, at, &mut opens);
            in_template = false;
            // After the closing backtick the template is a value; after `${` an expression starts.
            before = if closed { Before::Value } else { Before::Operator };
            continue;
        }
        let b = bytes[at];
        if b >= 0x80 && source[at..].chars().next().is_some_and(space) {
            let c = source[at..].chars().next().unwrap_or(' ');
            new_line |= matches!(c, '\u{2028}' | '\u{2029}');
            at += c.len_utf8();
            continue;
        }
        // What this token leaves for the next `(`, word and `/`; most tokens clear them.
        let (mut next_condition, mut next_dot, mut next_unsure) = (false, false, false);
        match b {
            // JavaScript's whitespace includes the vertical tab, which Rust's ASCII whitespace does not.
            _ if b.is_ascii_whitespace() || b == 0x0b => {
                new_line |= matches!(b, b'\n' | b'\r');
                at += 1;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                let end = source[at..].find(['\n', '\r', '\u{2028}', '\u{2029}']).map_or(bytes.len(), |n| at + n);
                if !kept(&source[at..end]) {
                    out.push_str(&source[copied..at]);
                    trim_end_of_line(&mut out);
                    keep_apart(&mut out, bytes.get(end));
                    copied = end;
                }
                at = end;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = source[at + 2..].find("*/").map_or(bytes.len(), |n| at + 2 + n + 2);
                let comment = &source[at..end];
                new_line |= comment.contains(['\n', '\r', '\u{2028}', '\u{2029}']);
                if !kept(comment) {
                    out.push_str(&source[copied..at]);
                    let breaks = line_breaks(comment);
                    // Only the spaces after it are looked at, so a long line of comments stays
                    // linear.
                    let next = source[end..].bytes().find(|c| !matches!(c, b' ' | b'\t'));
                    let ends_line = matches!(next, None | Some(b'\n' | b'\r'));
                    if !breaks.is_empty() || ends_line {
                        trim_end_of_line(&mut out);
                    }
                    if !breaks.is_empty() {
                        // The comment's own line breaks: one is a statement's end for ASI, and each
                        // keeps a line number.
                        keep_apart(&mut out, breaks.as_bytes().first());
                        out.push_str(&breaks);
                    } else if !ends_line && !out.is_empty() && !out.ends_with([' ', '\t', '\n']) {
                        // `return/**/x` stays two words.
                        out.push(' ');
                    }
                    keep_apart(&mut out, bytes.get(end));
                    copied = end;
                }
                at = end;
                continue;
            }
            b'/' if unsure || (new_line && before == Before::Value) => return source.to_string(),
            // Only where a value is expected is a `/` looked at as a regular expression's start, so a
            // line of divisions stays linear.
            b'/' => match (before == Before::Operator).then(|| regex(source, at)).flatten() {
                Some(end) => {
                    at = end;
                    before = Before::Value;
                }
                None => {
                    at += 1;
                    before = Before::Operator;
                }
            },
            b'"' | b'\'' => {
                at = string(bytes, at);
                before = Before::Value;
            }
            b'`' => {
                in_template = true;
                at += 1;
            }
            b'{' => {
                opens.push(Open::Brace);
                before = Before::Operator;
                at += 1;
            }
            b'}' => {
                if opens.pop() == Some(Open::Substitution) {
                    in_template = true;
                } else if matches!(opens.last(), Some(Open::Group | Open::Condition)) {
                    // Inside parentheses or brackets a `}` ends an object or a pattern, a value
                    // (`({}) / 2`, `for (const {a} of /re/)`).
                    before = Before::Value;
                } else {
                    // A block's end, or an object, function or class at a statement's level: a `/`
                    // after it is not read either way (`unsure`).
                    before = Before::Operator;
                    next_unsure = true;
                }
                at += 1;
            }
            b'(' => {
                opens.push(if condition { Open::Condition } else { Open::Group });
                before = Before::Operator;
                at += 1;
            }
            b'[' => {
                opens.push(Open::Group);
                before = Before::Operator;
                at += 1;
            }
            b')' => {
                // After `if (..)` comes a statement; after any other `)`, a value.
                before = if opens.pop() == Some(Open::Condition) { Before::Operator } else { Before::Value };
                at += 1;
            }
            b']' => {
                opens.pop();
                before = Before::Value;
                at += 1;
            }
            // `a++ / 2` or `++/re/.lastIndex`: a `/` after `++` or `--` is not read either way.
            b'+' | b'-' if bytes.get(at + 1) == Some(&b) => {
                next_unsure = true;
                at += 2;
            }
            _ if b.is_ascii_digit() || (b == b'.' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)) => {
                at = word_end(source, at + 1, true);
                before = Before::Value;
            }
            _ if b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b == b'#' || b >= 0x80 => {
                let end = word_end(source, at + b.is_ascii() as usize, false);
                let word = &source[at..end];
                // After a `.`, a word is a property, a value even when it is `return`.
                let keyword = !dot;
                // `for await (` is a loop as `for (` is.
                next_condition = keyword && (matches!(word, "if" | "while" | "for" | "with") || (word == "await" && condition));
                // In `for (x of /re/)`, `of` after a binding is the operator; anywhere else it is
                // a name, which may be divided.
                let of = word == "of" && before == Before::Value && opens.last() == Some(&Open::Condition);
                before = if keyword && (of || BEFORE_REGEX.contains(&word)) { Before::Operator } else { Before::Value };
                at = end;
            }
            _ => {
                // `?.` is a property access as well as `.`; `...` is a spread, after which a
                // word is not a property.
                next_dot = b == b'.' && !(at > 0 && bytes[at - 1] == b'.') && bytes.get(at + 1) != Some(&b'.');
                before = Before::Operator;
                at += 1;
            }
        }
        condition = next_condition;
        dot = next_dot;
        unsure = next_unsure;
        new_line = false;
    }
    out.push_str(&source[copied..]);
    out
}

/// A lone `\r` left before a `\n` would become one `\r\n` and a line would be lost: a space keeps
/// them two line ends.
fn keep_apart(out: &mut String, next: Option<&u8>) {
    if out.ends_with('\r') && next == Some(&b'\n') {
        out.push(' ');
    }
}

/// The line terminators in `comment`, in order: `\n`, `\r\n`, a lone `\r`, U+2028 and U+2029.
fn line_breaks(comment: &str) -> String {
    let mut out = String::new();
    let mut chars = comment.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                out.push_str("\r\n");
            }
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => {
                // A lone `\r` before a later `\n` stays two line ends.
                keep_apart(&mut out, (c == '\n').then_some(&b'\n'));
                out.push(c);
            }
            _ => {}
        }
    }
    out
}

/// A comment that stays: a licence, which minifiers keep, or one the browser reads.
fn kept(comment: &str) -> bool {
    ["/*!", "//!", "//#", "/*#"].iter().any(|start| comment.starts_with(start))
        || comment.contains("@license")
        || comment.contains("@preserve")
}

/// Whitespace in JavaScript outside ASCII: a no-break space, a byte order mark, any other Unicode
/// space, or a line or paragraph separator.
fn space(c: char) -> bool {
    c.is_whitespace() || c == '\u{FEFF}'
}

/// Removes the spaces and tabs at the end of `out`, before a comment that ran to its line's end.
fn trim_end_of_line(out: &mut String) {
    let kept = out.trim_end_matches([' ', '\t']).len();
    out.truncate(kept);
}

/// The end of a word, or of a number (`number`, which may hold a `.`), that starts before `at` (or
/// at it, when it starts with a character outside ASCII). A word stops at a `.`, which is then
/// read as a property access.
fn word_end(source: &str, at: usize, number: bool) -> usize {
    source[at..]
        .char_indices()
        .find(|&(_, c)| {
            !(c.is_ascii_alphanumeric() || matches!(c, '_' | '$') || (number && c == '.') || (!c.is_ascii() && !space(c)))
        })
        .map_or(source.len(), |(n, _)| at + n)
}

/// The end of the string that starts at `at` (its closing quote included), or of its line when
/// it is not closed there.
fn string(bytes: &[u8], at: usize) -> usize {
    let quote = bytes[at];
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            // A backslash before a line break continues the string, also when it is `\r\n`.
            b'\\' if bytes[i + 1..].starts_with(b"\r\n") => i += 3,
            b'\\' => i += 2,
            b'\n' => return i,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Skips a template literal's text from `at` to its closing backtick (past it, and `true`) or to a
/// `${` (past it, and `false`), which opens a substitution whose `}` comes back here.
fn template(bytes: &[u8], at: usize, opens: &mut Vec<Open>) -> (usize, bool) {
    let mut i = at;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'`' => return (i + 1, true),
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                opens.push(Open::Substitution);
                return (i + 2, false);
            }
            _ => i += 1,
        }
    }
    (bytes.len(), true)
}

/// The end of the regular expression that starts at `at` (its flags included), or `None` when
/// none closes on its line: then the `/` divides.
fn regex(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = at + 1;
    let mut class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if bytes.get(i + 1).is_some_and(|&c| c != b'\n') => i += 2,
            b'\n' | b'\r' => return None,
            b'[' => {
                class = true;
                i += 1;
            }
            b']' => {
                class = false;
                i += 1;
            }
            b'/' if !class => return Some(word_end(source, i + 1, false)),
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::strip;

    #[test]
    fn comments_go_and_every_line_stays_where_it_was() {
        let source =
            "/**\n * Saves.\n * @param {HTMLElement} a\n */\nexport function save(a) { // why\n  return a; /* done */\n}\n";
        let out = strip(source);
        assert_eq!(out, "\n\n\n\nexport function save(a) {\n  return a;\n}\n");
        assert_eq!(out.lines().count(), source.lines().count());
        assert_eq!(strip("a/* x */b"), "a b", "two words stay two");
        assert_eq!(strip("return/**/x"), "return x");
        assert_eq!(strip("a /* x */ b"), "a  b");
        assert_eq!(strip("/* a\n  bc */ x"), "\n x");
        for kept in
            ["/*! (c) Ann */\n// @license MIT\nx", "//! (c) Ann\nx", "x\n//# sourceMappingURL=a.js.map", "x\n//# sourceURL=a.js"]
        {
            assert_eq!(strip(kept), kept, "a licence, or a comment the browser reads, stays");
        }
    }

    #[test]
    fn a_block_comments_line_breaks_stay_whatever_they_are() {
        assert_eq!(strip("return/*\u{2028}*/1;"), "return\u{2028}1;", "still returns undefined");
        assert_eq!(strip("return/*\r*/1;"), "return\r1;");
        assert_eq!(strip("a /* x\r\ny */ b"), "a\r\n b");
        assert_eq!(
            strip("x = a\u{b}/ 2 + \"/\" + \"//kept\";"),
            "x = a\u{b}/ 2 + \"/\" + \"//kept\";",
            "a vertical tab is a space"
        );
        // A long line of comments, or of divisions after a `[`, each once looked to the line's end.
        let long = "a /* x */ ".repeat(200_000);
        assert_eq!(strip(&long).len(), "a  ".len() * 200_000 - 1);
        let divisions = format!("x = {}0;", "a / 2 + \"[\" + ".repeat(50_000));
        assert_eq!(strip(&divisions), divisions);
        // A lone CR, a comment, then an LF: still two line ends.
        for (source, lines) in
            [("a\r// c\nb", 3), ("a\r/* c */\nb", 3), ("a/* x\r*/\nb", 3), ("a\r/*\n*/b", 3), ("x = 1;/* a\rb\nc */\ny", 4)]
        {
            let out = strip(source);
            let ends = |s: &str| s.matches("\r\n").count() + s.matches(['\r', '\n']).count() - 2 * s.matches("\r\n").count();
            assert_eq!(ends(&out) + 1, lines, "{source:?} -> {out:?}");
        }
    }

    #[test]
    fn a_slash_only_a_parser_could_read_leaves_the_script_as_it_is() {
        // After a block's `}` or a `++`, a `/` divides or starts a regular expression and only a
        // parser knows which: the script is served as written.
        for source in [
            "x = class {} / 2 + \"/\" + \"//kept\"; // c",
            "w = {} / 2 + \"/\" + \"//kept\"; // c",
            "x = function () {} / 2 + \"//kept\"; // c",
            "if (a) { b() }\n/\"/.test(s); const u = \"//kept\"; // c",
            "x\n++/[//]/.a; // c",
            "a++ / 2; // c",
            "for (const c of s) {\n  if (c) continue\n  /'/.test(c); const u = '//kept' // c\n}",
            "for (;;) { break\n/'/.test(c); const u = '//kept' } // c",
            "let t\n/`/.test(s); const u = '//kept' // c",
            "import \"./x.mjs\"\n/'/.test(s); const u = '//kept' // c",
            "x = a /* c\n */ / 2 + \"//kept\"; // c",
            "x = a\u{2028}/ 2 + \"//kept\"; // c",
        ] {
            assert_eq!(strip(source), source, "{source:?}");
        }
        // Inside parentheses a `}` ends a value or a pattern, and the rest is stripped.
        for (source, out) in [
            ("y = ({}) / 2 + \"//kept\"; // c", "y = ({}) / 2 + \"//kept\";"),
            (
                "for (const {a} of /\"/.exec(s)) g(a); const u = \"//kept\"; // c",
                "for (const {a} of /\"/.exec(s)) g(a); const u = \"//kept\";",
            ),
            ("for (const { a: { b } } of /\"/.exec(s)) g(b); // c", "for (const { a: { b } } of /\"/.exec(s)) g(b);"),
            ("for await (const {a} of /\"/.exec(s)) g(a); // c", "for await (const {a} of /\"/.exec(s)) g(a);"),
            ("f(() => {}, /\"/); const u = \"//kept\"; // c", "f(() => {}, /\"/); const u = \"//kept\";"),
        ] {
            assert_eq!(strip(source), out, "{source:?}");
        }
    }

    #[test]
    fn a_property_named_as_a_keyword_is_a_property_across_a_line_or_comment() {
        for (source, out) in [
            ("x = o.\n  in / 2 + \"/\" + \"//kept\"; // c", "x = o.\n  in / 2 + \"/\" + \"//kept\";"),
            ("x = o./* c */in / 2 + \"/\" + \"//kept\";", "x = o. in / 2 + \"/\" + \"//kept\";"),
            ("x = o.return / 2 + \"//kept\"; // c", "x = o.return / 2 + \"//kept\";"),
        ] {
            assert_eq!(strip(source), out, "{source:?}");
        }
    }

    #[test]
    fn windows_line_endings_and_spaces_outside_ascii_are_read_as_such() {
        let crlf = "const s = \"a\\\r\nb//c\"; // gone\r\nconst t = 1; // gone\r\n";
        assert_eq!(strip(crlf), "const s = \"a\\\r\nb//c\";\r\nconst t = 1;\r\n");
        for kept in [
            "x =\u{a0}/\"/.test(s); const u = \"//kept\";",
            "\u{feff}/\"/.test(s); const u = \"//kept\";",
            "x =\u{2028}/\"/.test(s); const u = \"//kept\";",
            "const café = 4; x = café / 2 + \"/\" + \"//kept\";",
        ] {
            assert_eq!(strip(kept), kept);
        }
    }

    #[test]
    fn strings_templates_and_regular_expressions_keep_what_looks_like_a_comment() {
        for kept in [
            r#"const url = "https://example.com"; const b = '/* no */';"#,
            "const t = `// ${a} /* ${`//${b}`} */`;",
            "const t = `${ { a: 1 }.a }//`;",
            r"const re = /\/\/ [/*] x/g.test(s);",
            "if (ok) /a\\/b/.test(s);",
            "return /a/.test(s) ? /b/ : /c/;",
            "x = a / b / c;",
            "x = a++ / 2 / b;",
            "x = (a) / 2 / b;",
            "x = a[0] / 2 / b;",
            "x = obj.return / 2 / b;",
            "x = 1.5 / 2 / b;",
        ] {
            assert_eq!(strip(kept), kept);
        }
        for kept in [
            r#"x = `${/"/.test(s)}`; const u = "//kept";"#,
            r#"x = `${/'/.test(s)}`; const u = '//kept';"#,
            r#"x = `a${b}${/"/.source}`; const u = "//kept";"#,
            r#"export default /"/; const s = "a//b";"#,
            r#"let of = 4; x = of / 2 + "/" + "//kept";"#,
            r#"let async = 4; x = async / 2 + "/" + "//kept";"#,
            r#"for await (const x of y) /"/.test(x); const u = "//kept";"#,
            r#"x = await (a) / 2 / b; const u = "//kept";"#,
            r#"async function g(){ f(...await /"/); const u = "//kept"; }"#,
            r#"x = [...typeof /'/]; const u = '//kept';"#,
            r#"for (const m of /"/.exec(s)) f(m); const u = "//kept";"#,
            r#"for (const [a] of /"/.exec(s)) f(a); const u = "//kept";"#,
            r#"if (of / 2) x(); const u = "/" + "//kept";"#,
        ] {
            assert_eq!(strip(kept), kept);
        }
        assert_eq!(strip("x = a / b; // c / d"), "x = a / b;");
        assert_eq!(strip("x = a / b /* c */ / d;"), "x = a / b  / d;");
        assert_eq!(strip(r#"s.replace(/\\/g, "") // backslashes"#), r#"s.replace(/\\/g, "")"#);
    }
}
