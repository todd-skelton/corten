//! Just enough JSON for message catalogs: objects and strings, in order, with positions in errors.

#[derive(Debug, PartialEq)]
pub enum Value {
    Object(Vec<(String, Value)>),
    String(String),
    /// A number, `true`, `false`, `null` or an array: what it was, for the error.
    Other(&'static str),
}

pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { bytes: text.as_bytes(), text, i: 0 };
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
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        let before = &self.text[..self.i.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.len() - before.rfind('\n').map_or(0, |n| n + 1) + 1;
        format!("{message} at line {line}, column {column}")
    }

    fn space(&mut self) {
        while self.bytes.get(self.i).is_some_and(|b| b.is_ascii_whitespace()) {
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
            Some(b'{') => self.object(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b'[') => {
                self.skip_nested()?;
                Ok(Value::Other("an array"))
            }
            Some(_) => {
                let start = self.i;
                while self.bytes.get(self.i).is_some_and(|b| b.is_ascii_alphanumeric() || b"+-.".contains(b)) {
                    self.i += 1;
                }
                match &self.text[start..self.i] {
                    "true" | "false" => Ok(Value::Other("a boolean")),
                    "null" => Ok(Value::Other("null")),
                    word if word.parse::<f64>().is_ok() => Ok(Value::Other("a number")),
                    _ => {
                        self.i = start;
                        Err(self.error("expected a value"))
                    }
                }
            }
            None => Err(self.error("expected a value")),
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

    fn skip_nested(&mut self) -> Result<(), String> {
        let mut depth = 0;
        while let Some(&b) = self.bytes.get(self.i) {
            match b {
                b'"' => {
                    self.string()?;
                    continue;
                }
                b'[' | b'{' => depth += 1,
                b']' | b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.i += 1;
                        return Ok(());
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
        Err(self.error("unclosed array"))
    }
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
        assert_eq!(entries[2].1, Value::Other("a number"));
        assert_eq!(entries[3].1, Value::Other("an array"));
        assert!(parse("{\"a\": \"b\",\n }").unwrap_err().contains("line 2"));
        assert!(parse("{\"a\" \"b\"}").is_err());
        assert!(parse("{\"a\": \"b\"} x").is_err());
    }
}
