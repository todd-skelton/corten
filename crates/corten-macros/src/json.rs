//! JSON for message catalogs and design-token files (RFC 8259): objects keep their keys in
//! order, numbers keep the text they were written with, and errors give a line and column.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),
    String(String),
    /// A number as written, `0.5` or `1e3`, so a value copied into CSS keeps its digits.
    Number(String),
    Bool(bool),
    Null,
}

impl Value {
    /// What the value is, for an error: `a number`.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Object(_) => "an object",
            Value::Array(_) => "an array",
            Value::String(_) => "a string",
            Value::Number(_) => "a number",
            Value::Bool(_) => "a boolean",
            Value::Null => "null",
        }
    }
}

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { bytes: text.as_bytes(), text, i: 0, depth: 0 };
    let value = p.value()?;
    p.space();
    if p.i < p.bytes.len() {
        return Err(p.error("expected the end of the file"));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    i: usize,
    depth: usize,
}

/// Objects and arrays inside one another deeper than this are an error, so a file cannot overflow
/// the compiler's stack here or in the recursive code that reads the value.
pub const MAX_DEPTH: usize = 128;

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        let before = &self.text[..self.i.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.len() - before.rfind('\n').map_or(0, |n| n + 1) + 1;
        format!("{message} at line {line}, column {column}")
    }

    fn space(&mut self) {
        // RFC 8259's four: space, tab, line feed, carriage return (not form feed).
        while self.bytes.get(self.i).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.space();
        if self.bytes.get(self.i) == Some(&byte) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.space();
        match self.bytes.get(self.i) {
            Some(b'{' | b'[') if self.depth == MAX_DEPTH => {
                Err(self.error(&format!("objects and arrays nest more than {MAX_DEPTH} deep")))
            }
            Some(b'{') => {
                self.depth += 1;
                let value = self.object();
                self.depth -= 1;
                value
            }
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'[') => {
                self.depth += 1;
                let value = self.array();
                self.depth -= 1;
                value
            }
            Some(_) => {
                let start = self.i;
                while self.bytes.get(self.i).is_some_and(|b| b.is_ascii_alphanumeric() || b"+-.".contains(b)) {
                    self.i += 1;
                }
                match &self.text[start..self.i] {
                    "true" => Ok(Value::Bool(true)),
                    "false" => Ok(Value::Bool(false)),
                    "null" => Ok(Value::Null),
                    word if is_number(word) => Ok(Value::Number(word.to_string())),
                    _ => {
                        self.i = start;
                        Err(self.error("expected a value"))
                    }
                }
            }
            None => Err(self.error("expected a value")),
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.i += 1;
        let mut items = Vec::new();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            return Err(self.error("expected , or ]"));
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.i += 1;
        let mut entries = Vec::new();
        if self.eat(b'}') {
            return Ok(Value::Object(entries));
        }
        loop {
            self.space();
            if self.bytes.get(self.i) != Some(&b'"') {
                return Err(self.error("expected a key in quotes"));
            }
            let key = self.string()?;
            if !self.eat(b':') {
                return Err(self.error("expected :"));
            }
            let value = self.value()?;
            entries.push((key, value));
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Value::Object(entries));
            }
            return Err(self.error("expected , or }"));
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            while self.bytes.get(self.i).is_some_and(|b| *b != b'"' && *b != b'\\' && *b >= 0x20) {
                self.i += 1;
            }
            out.push_str(&self.text[start..self.i]);
            match self.bytes.get(self.i) {
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let escape = self.bytes.get(self.i).copied();
                    self.i += 1;
                    match escape {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'b') => out.push('\u{8}'),
                        Some(b'f') => out.push('\u{c}'),
                        Some(b'n') => out.push('\n'),
                        Some(b'r') => out.push('\r'),
                        Some(b't') => out.push('\t'),
                        Some(b'u') => {
                            let high = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&high) {
                                if self.bytes.get(self.i..self.i + 2) != Some(b"\\u") {
                                    return Err(self.error("expected the second half of a surrogate pair"));
                                }
                                self.i += 2;
                                let low = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(self.error("invalid surrogate pair"));
                                }
                                0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
                            } else {
                                high
                            };
                            out.push(char::from_u32(code).ok_or_else(|| self.error("invalid \\u escape"))?);
                        }
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                _ => return Err(self.error("unterminated string")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self.text.get(self.i..self.i + 4).ok_or_else(|| self.error("expected 4 hex digits"))?;
        let value = u32::from_str_radix(digits, 16).map_err(|_| self.error("expected 4 hex digits"))?;
        self.i += 4;
        Ok(value)
    }
}

/// JSON's number grammar: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`.
fn is_number(word: &str) -> bool {
    let digits = |s: &str| s.len() - s.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let rest = word.strip_prefix('-').unwrap_or(word);
    let int = digits(rest);
    if int == 0 || (int > 1 && rest.starts_with('0')) {
        return false;
    }
    let mut rest = &rest[int..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let n = digits(fraction);
        if n == 0 {
            return false;
        }
        rest = &fraction[n..];
    }
    if let Some(exponent) = rest.strip_prefix(['e', 'E']) {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        let n = digits(exponent);
        return n > 0 && n == exponent.len();
    }
    rest.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_objects_and_strings_in_order() {
        let value = parse(r#" {"b": "x\"y\u00e9\ud83d\ude00", "a": {"c": "\n"}, "n": 1, "l": [1, "]"]} "#).unwrap();
        let Value::Object(entries) = value else { panic!() };
        assert_eq!(entries[0], ("b".into(), Value::String("x\"y\u{e9}\u{1F600}".into())));
        assert_eq!(entries[1].1, Value::Object(vec![("c".into(), Value::String("\n".into()))]));
        assert_eq!(entries[2].1, Value::Number("1".into()));
        assert_eq!(entries[3].1, Value::Array(vec![Value::Number("1".into()), Value::String("]".into())]));
        assert!(parse("{\"a\": \"b\",\n }").unwrap_err().contains("line 2"));
        assert!(parse("{\"a\" \"b\"}").is_err());
        assert!(parse("{\"a\": \"b\"} x").is_err());
    }

    #[test]
    fn reads_numbers_as_written_and_the_other_values() {
        let value = parse(r#"[0, -0.5, 1e3, 2.50E-2, true, false, null, [], {}]"#).unwrap();
        let number = |s: &str| Value::Number(s.into());
        assert_eq!(
            value,
            Value::Array(vec![
                number("0"),
                number("-0.5"),
                number("1e3"),
                number("2.50E-2"),
                Value::Bool(true),
                Value::Bool(false),
                Value::Null,
                Value::Array(vec![]),
                Value::Object(vec![]),
            ])
        );
        assert!(parse(" \t\r\n[1]\n").is_ok());
        for not_json in ["01", "1.", ".5", "+1", "1e", "inf", "NaN", "0x10", "[1,]", "[1 2]", "\u{c}[1]", "[1]\u{b}"] {
            assert!(parse(not_json).is_err(), "{not_json}");
        }
        let deep = |n: usize| format!("{}1{}", "[".repeat(n), "]".repeat(n));
        assert!(parse(&deep(MAX_DEPTH)).is_ok());
        assert!(parse(&deep(100_000)).unwrap_err().contains("nest more than 128 deep"), "an error, not a stack overflow");
    }
}
