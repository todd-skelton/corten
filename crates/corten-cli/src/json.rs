//! A JSON reader for `cargo metadata`'s output, so the CLI reads its packages by structure
//! rather than by searching the text. Standard library only.

/// A JSON value. Numbers are kept as their text; the CLI reads none.
#[derive(Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The value of `key` in an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// The items of an array; none for any other value.
    pub fn items(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            _ => &[],
        }
    }
}

pub fn parse(text: &str) -> Result<Value, String> {
    let mut reader = Reader { bytes: text.as_bytes(), at: 0 };
    let value = reader.value()?;
    reader.space();
    if reader.at == reader.bytes.len() { Ok(value) } else { Err(reader.error("trailing text")) }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn error(&self, what: &str) -> String {
        format!("invalid JSON at byte {}: {what}", self.at)
    }

    fn space(&mut self) {
        while self.bytes.get(self.at).is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.space();
        let found = self.bytes.get(self.at) == Some(&byte);
        if found {
            self.at += 1;
        }
        found
    }

    fn word(&mut self, word: &str, value: Value) -> Result<Value, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a value"))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.space();
        match self.bytes.get(self.at) {
            Some(b'{') => {
                self.at += 1;
                let mut fields = Vec::new();
                if self.eat(b'}') {
                    return Ok(Value::Object(fields));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    if !self.eat(b':') {
                        return Err(self.error("expected ':'"));
                    }
                    fields.push((key, self.value()?));
                    if self.eat(b'}') {
                        return Ok(Value::Object(fields));
                    }
                    if !self.eat(b',') {
                        return Err(self.error("expected ',' or '}'"));
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                if self.eat(b']') {
                    return Ok(Value::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    if self.eat(b']') {
                        return Ok(Value::Array(items));
                    }
                    if !self.eat(b',') {
                        return Err(self.error("expected ',' or ']'"));
                    }
                }
            }
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => {
                let start = self.at;
                while self.bytes.get(self.at).is_some_and(|b| matches!(b, b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.at += 1;
                }
                Ok(Value::Number(String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned()))
            }
            _ => Err(self.error("expected a value")),
        }
    }

    fn string(&mut self) -> Result<String, String> {
        if self.bytes.get(self.at) != Some(&b'"') {
            return Err(self.error("expected a string"));
        }
        self.at += 1;
        let mut out = Vec::new();
        loop {
            match *self.bytes.get(self.at).ok_or_else(|| self.error("unterminated string"))? {
                b'"' => {
                    self.at += 1;
                    return String::from_utf8(out).map_err(|_| self.error("a string is not UTF-8"));
                }
                b'\\' => {
                    let escape = *self.bytes.get(self.at + 1).ok_or_else(|| self.error("unterminated string"))?;
                    self.at += 2;
                    let c = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let high = self.hex4()?;
                            let code = if (0xd800..0xdc00).contains(&high) && self.bytes[self.at..].starts_with(b"\\u") {
                                self.at += 2;
                                let low = self.hex4()?;
                                0x10000 + ((high - 0xd800) << 10) + (low.wrapping_sub(0xdc00) & 0x3ff)
                            } else {
                                high
                            };
                            char::from_u32(code).ok_or_else(|| self.error("invalid \\u escape"))?
                        }
                        _ => return Err(self.error("invalid escape")),
                    };
                    out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                }
                byte => {
                    out.push(byte);
                    self.at += 1;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let hex = self.bytes.get(self.at..self.at + 4).and_then(|h| std::str::from_utf8(h).ok());
        let code = hex.and_then(|h| u32::from_str_radix(h, 16).ok()).ok_or_else(|| self.error("invalid \\u escape"))?;
        self.at += 4;
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cargo_metadata_shaped_json() {
        let text = r#" {"packages":[{"name":"a\"b","manifest_path":"C:\\x\\Cargo.toml","n":-1.5e3,"ok":true,"none":null,
            "deps":[], "s":"\u00e9\ud83d\ude00\n"}],"workspace_members":["id"] } "#;
        let value = parse(text).unwrap();
        let package = &value.get("packages").unwrap().items()[0];
        assert_eq!(package.get("name").and_then(Value::as_str), Some("a\"b"));
        assert_eq!(package.get("manifest_path").and_then(Value::as_str), Some(r"C:\x\Cargo.toml"));
        assert_eq!(package.get("n"), Some(&Value::Number("-1.5e3".into())));
        assert_eq!(package.get("ok"), Some(&Value::Bool(true)));
        assert_eq!(package.get("none"), Some(&Value::Null));
        assert!(package.get("deps").unwrap().items().is_empty());
        assert_eq!(package.get("s").and_then(Value::as_str), Some("\u{e9}\u{1f600}\n"));
        assert_eq!(value.get("workspace_members").unwrap().items().len(), 1);
    }

    #[test]
    fn rejects_broken_json() {
        for text in ["", "{", r#"{"a" 1}"#, "[1,]", r#""\x""#, "[1] 2", "tru"] {
            assert!(parse(text).is_err(), "{text:?}");
        }
    }
}
