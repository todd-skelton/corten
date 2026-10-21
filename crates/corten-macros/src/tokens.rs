//! Small helpers for building output tokens with `proc_macro` alone.

use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree};

/// A compile error at a span.
pub type Error = (Span, String);

pub fn err<T>(span: Span, message: impl Into<String>) -> Result<T, Error> {
    Err((span, message.into()))
}

/// The output buffer's name. Mixed-site hygiene keeps it out of reach of the user's expressions.
pub fn out_ident() -> Ident {
    Ident::new("__corten_out", Span::mixed_site())
}

/// Parses `template` and replaces the identifier `__OUT` with [`out_ident`] and `__A0`, `__A1`, ...
/// with `args`. The arguments keep their own spans, so errors in user code point at user code.
pub fn q(template: &str, args: &[TokenStream]) -> TokenStream {
    subst(template.parse().expect("generated code parses"), args)
}

fn subst(tokens: TokenStream, args: &[TokenStream]) -> TokenStream {
    let mut out = TokenStream::new();
    for token in tokens {
        match token {
            TokenTree::Ident(ident) => {
                let name = ident.to_string();
                if name == "__OUT" {
                    out.extend([TokenTree::Ident(out_ident())]);
                } else if let Some(n) = name.strip_prefix("__A").and_then(|n| n.parse::<usize>().ok()) {
                    out.extend(args[n].clone());
                } else {
                    out.extend([TokenTree::Ident(ident)]);
                }
            }
            TokenTree::Group(group) => {
                let mut copy = Group::new(group.delimiter(), subst(group.stream(), args));
                copy.set_span(group.span());
                out.extend([TokenTree::Group(copy)]);
            }
            other => out.extend([other]),
        }
    }
    out
}

/// `::core::compile_error! { "message" }` at `span`.
pub fn compile_error(span: Span, message: &str) -> TokenStream {
    let mut tokens: Vec<TokenTree> = vec![
        Punct::new(':', Spacing::Joint).into(),
        Punct::new(':', Spacing::Alone).into(),
        Ident::new("core", span).into(),
        Punct::new(':', Spacing::Joint).into(),
        Punct::new(':', Spacing::Alone).into(),
        Ident::new("compile_error", span).into(),
        Punct::new('!', Spacing::Alone).into(),
        Group::new(Delimiter::Brace, TokenTree::from(Literal::string(message)).into()).into(),
    ];
    for token in &mut tokens {
        token.set_span(span);
    }
    tokens.into_iter().collect()
}

pub fn literal(text: &str) -> TokenStream {
    TokenTree::from(Literal::string(text)).into()
}

pub fn stream(tokens: impl IntoIterator<Item = TokenTree>) -> TokenStream {
    tokens.into_iter().collect()
}

/// Wraps tokens in parentheses, so an expression spliced into generated code stays one expression.
pub fn parens(tokens: TokenStream) -> TokenStream {
    TokenTree::Group(Group::new(Delimiter::Parenthesis, tokens)).into()
}

pub fn is_punct(token: Option<&TokenTree>, ch: char) -> bool {
    matches!(token, Some(TokenTree::Punct(p)) if p.as_char() == ch)
}

pub fn is_ident(token: Option<&TokenTree>, name: &str) -> bool {
    matches!(token, Some(TokenTree::Ident(i)) if i.to_string() == name)
}

/// The value of a string literal (plain or raw), or `None` for any other literal.
pub fn string_value(literal: &Literal) -> Option<String> {
    let text = literal.to_string();
    if let Some(rest) = text.strip_prefix('r') {
        let hashes = rest.len() - rest.trim_start_matches('#').len();
        let body = rest[hashes..].strip_prefix('"')?;
        return Some(body[..body.len() - 1 - hashes].to_string());
    }
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next()? {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            'x' => {
                let hex: String = chars.by_ref().take(2).collect();
                out.push(u8::from_str_radix(&hex, 16).ok()? as char);
            }
            'u' => {
                chars.next()?;
                let hex: String = chars.by_ref().take_while(|c| *c != '}').filter(|c| *c != '_').collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
            }
            '\n' | '\r' => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

/// The text a non-string literal prints as in HTML: a number as written (without `_`), a char
/// as itself. `None` for byte strings and numbers with a type suffix.
pub fn literal_text(literal: &Literal) -> Option<String> {
    let text = literal.to_string();
    if let Some(c) = text.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
        return format!("\"{c}\"").parse::<Literal>().ok().and_then(|l| string_value(&l));
    }
    let number = text.starts_with(|c: char| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | '_' | 'e' | 'E' | '-' | '+'));
    number.then(|| text.replace('_', ""))
}

/// Escapes text for HTML text and double-quoted attributes, the same way the runtime does.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}
