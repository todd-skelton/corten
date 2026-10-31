//! A script's comments, removed from what the browser receives (the macro still reads the JSDoc
//! from the file). Each line keeps its number, so DevTools and a stack trace point at the line
//! the author wrote. A legal comment (`/*! .. */`, or one with `@license` or `@preserve`) stays,
//! as minifiers keep it.

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

/// Words after which a `/` starts a regular expression.
const BEFORE_REGEX: &[&str] =
    &["return", "typeof", "instanceof", "in", "of", "new", "delete", "void", "throw", "case", "do", "else", "yield", "await"];

/// `source` without its comments: a line comment is removed to its line's end, a block comment
/// becomes the newlines it held, or a space when it held none. Strings, template literals and
/// regular expressions are copied as they are.
pub fn without_comments(source: &str) -> String {
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
    // Set when a template literal's text is being copied (at its start or after a `}`).
    let mut in_template = false;
    while at < bytes.len() {
        if in_template {
            at = template(bytes, at, &mut opens, &mut in_template);
            before = Before::Value;
            continue;
        }
        let b = bytes[at];
        // What this token leaves for the next `(` and word; most tokens clear both.
        let (mut next_condition, mut next_dot) = (false, false);
        match b {
            _ if b.is_ascii_whitespace() => {
                at += 1;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                let end = bytes[at..].iter().position(|&c| c == b'\n').map_or(bytes.len(), |n| at + n);
                if !legal(&source[at..end]) {
                    out.push_str(&source[copied..at]);
                    trim_end_of_line(&mut out);
                    copied = end;
                }
                at = end;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let end = source[at + 2..].find("*/").map_or(bytes.len(), |n| at + 2 + n + 2);
                let comment = &source[at..end];
                if !legal(comment) {
                    out.push_str(&source[copied..at]);
                    let lines = comment.matches('\n').count();
                    let rest_of_line = source[end..].split('\n').next().unwrap_or("");
                    let ends_line = rest_of_line.trim().is_empty();
                    if lines > 0 || ends_line {
                        trim_end_of_line(&mut out);
                    }
                    if lines > 0 {
                        out.extend(std::iter::repeat_n('\n', lines));
                    } else if !ends_line && !out.is_empty() && !out.ends_with([' ', '\t', '\n']) {
                        // `return/**/x` stays two words.
                        out.push(' ');
                    }
                    copied = end;
                }
                at = end;
                continue;
            }
            b'/' => match (before, regex(bytes, at)) {
                (Before::Operator, Some(end)) => {
                    at = end;
                    before = Before::Value;
                }
                _ => {
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
                } else {
                    // A block's end starts a statement, which may be a regular expression; an
                    // object literal divided by something is far rarer.
                    before = Before::Operator;
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
            // `a++ / 2`: `++` and `--` leave what came before them.
            b'+' | b'-' if bytes.get(at + 1) == Some(&b) => at += 2,
            _ if b.is_ascii_digit() || (b == b'.' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit)) => {
                at = word_end(bytes, at + 1);
                before = Before::Value;
            }
            _ if b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b == b'#' || b >= 0x80 => {
                let end = word_end(bytes, at + 1);
                let word = &source[at..end];
                // After a `.`, a word is a property, a value even when it is `return`.
                let keyword = !dot;
                next_condition = keyword && matches!(word, "if" | "while" | "for" | "with");
                before = if keyword && BEFORE_REGEX.contains(&word) { Before::Operator } else { Before::Value };
                at = end;
            }
            _ => {
                // `?.` is a property access as well as `.`.
                next_dot = b == b'.';
                before = Before::Operator;
                at += 1;
            }
        }
        condition = next_condition;
        dot = next_dot;
    }
    out.push_str(&source[copied..]);
    out
}

/// A comment a minifier keeps: a licence.
fn legal(comment: &str) -> bool {
    comment.starts_with("/*!") || comment.contains("@license") || comment.contains("@preserve")
}

/// Removes the spaces and tabs at the end of `out`, before a comment that ran to its line's end.
fn trim_end_of_line(out: &mut String) {
    let kept = out.trim_end_matches([' ', '\t']).len();
    out.truncate(kept);
}

/// The end of a word or number that starts before `at`.
fn word_end(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'_' | b'$' | b'.') || bytes[at] >= 0x80)
    {
        at += 1;
    }
    at
}

/// The end of the string that starts at `at` (its closing quote included), or of its line when
/// it is not closed there.
fn string(bytes: &[u8], at: usize) -> usize {
    let quote = bytes[at];
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\n' => return i,
            c if c == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Copies a template literal's text from `at` to its closing backtick (past it) or to a `${`,
/// which opens a substitution its `}` returns from.
fn template(bytes: &[u8], at: usize, opens: &mut Vec<Open>, in_template: &mut bool) -> usize {
    let mut i = at;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'`' => {
                *in_template = false;
                return i + 1;
            }
            b'$' if bytes.get(i + 1) == Some(&b'{') => {
                opens.push(Open::Substitution);
                *in_template = false;
                return i + 2;
            }
            _ => i += 1,
        }
    }
    *in_template = false;
    bytes.len()
}

/// The end of the regular expression that starts at `at` (its flags included), or `None` when
/// none closes on its line: then the `/` divides.
fn regex(bytes: &[u8], at: usize) -> Option<usize> {
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
            b'/' if !class => return Some(word_end(bytes, i + 1)),
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::without_comments as strip;

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
        assert_eq!(strip("/*! (c) Ann */\n// @license MIT\nx"), "/*! (c) Ann */\n// @license MIT\nx", "a licence stays");
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
        assert_eq!(strip("x = a / b; // c / d"), "x = a / b;");
        assert_eq!(strip("x = a / b /* c */ / d;"), "x = a / b  / d;");
        assert_eq!(strip(r#"s.replace(/\\/g, "") // backslashes"#), r#"s.replace(/\\/g, "")"#);
    }
}
