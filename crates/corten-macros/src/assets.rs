//! `embed!`: a static directory compiled into the program, with a constant per file.
//!
//! Each file is an `include_bytes!`, so editing one rebuilds the crate. A file added to the
//! directory is seen when the crate next compiles (any change to its code), since a macro cannot
//! ask cargo to watch a directory; `corten dev` reads the directory itself and restarts the app
//! without a build when only its files changed.
//!
//! A debug build keeps the directory's full path (`Embed::DIR`) for `corten dev`; a release
//! build keeps none. A linked directory is refused rather than followed.
//!
//! A file's name is its URL, so the names are checked here: letters, digits and `- _ . ~ @ + , = !`
//! only, which a URL carries as they are (`#`, `?`, `%` or a space would break it).

use std::path::{Path, PathBuf};

use proc_macro::{Delimiter, Span, TokenStream, TokenTree};

use crate::tokens::{err, is_ident, is_punct, literal, q, string_value, Error};

pub fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let usage = "expected: embed!(pub struct Static = \"static\"); with the directory's path from the crate's root";
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut i = 0;
    let mut attrs = TokenStream::new();
    while is_punct(tokens.get(i), '#') {
        attrs.extend(tokens[i..(i + 2).min(tokens.len())].iter().cloned());
        i += 2;
    }
    let mut vis = TokenStream::new();
    if is_ident(tokens.get(i), "pub") {
        vis.extend([tokens[i].clone()]);
        i += 1;
        if let Some(TokenTree::Group(g)) = tokens.get(i) {
            if g.delimiter() == Delimiter::Parenthesis {
                vis.extend([tokens[i].clone()]);
                i += 1;
            }
        }
    }
    if !is_ident(tokens.get(i), "struct") {
        return err(tokens.get(i).map_or(Span::call_site(), TokenTree::span), usage);
    }
    let (Some(TokenTree::Ident(name)), true, Some(TokenTree::Literal(path))) =
        (tokens.get(i + 1), is_punct(tokens.get(i + 2), '='), tokens.get(i + 3))
    else {
        return err(Span::call_site(), usage);
    };
    let rest = &tokens[i + 4..];
    if !(rest.is_empty() || (rest.len() == 1 && is_punct(rest.first(), ';'))) {
        return err(rest[0].span(), usage);
    }
    let relative = string_value(path).ok_or_else(|| (path.span(), usage.to_string()))?;
    {
        use std::path::Component;
        let parts: Vec<Component> = Path::new(&relative).components().collect();
        if !parts.iter().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
            || !parts.iter().any(|c| matches!(c, Component::Normal(_)))
        {
            return err(
                path.span(),
                "name a directory inside the crate (\"static\"): not the crate's root, a path with .., or an absolute path",
            );
        }
    }
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let dir = root.join(&relative);
    if !dir.is_dir() {
        return err(
            path.span(),
            format!("{} is not a directory (the path is from the crate's root, {})", dir.display(), root.display()),
        );
    }
    let mut files = Vec::new();
    collect(&dir, &dir, &mut files).map_err(|e| (path.span(), format!("cannot read {}: {e}", dir.display())))?;
    files.sort();
    let mut consts = TokenStream::new();
    let mut list = TokenStream::new();
    let mut names: Vec<(String, String)> = Vec::new();
    for (url, file) in &files {
        if let Some(c) = unsafe_in_url(url) {
            return err(
                path.span(),
                format!("{relative}/{url}: {c:?} does not belong in a URL; use letters, digits and - _ . ~ @ + , = !"),
            );
        }
        let constant = constant_name(url);
        if constant.trim_start_matches('_').is_empty() {
            return err(path.span(), format!("{relative}/{url}: the name needs a letter or digit, for its constant"));
        }
        if let Some((_, other)) = names.iter().find(|(c, _)| *c == constant) {
            return err(path.span(), format!("{other} and {url} are both {name}::{constant}: rename one of them"));
        }
        names.push((constant.clone(), url.clone()));
        let file = file.to_str().ok_or_else(|| (path.span(), format!("{}: the path is not UTF-8", file.display())))?;
        consts.extend(q(
            "#[doc = __A0] pub const __A1: ::corten::assets::Asset = ::corten::assets::Asset::__embedded(<Self as ::corten::assets::Embed>::OWNER, __A2);",
            &[
                literal(&format!(" `{relative}/{url}`")),
                TokenTree::Ident(proc_macro::Ident::new(&constant, path.span())).into(),
                literal(url),
            ],
        ));
        list.extend(q("(__A0, ::core::include_bytes!(__A1)),", &[literal(url), literal(file)]));
    }
    let dir_text = dir.to_str().ok_or_else(|| (path.span(), format!("{}: the path is not UTF-8", dir.display())))?;
    let ident: TokenStream = TokenTree::Ident(name.clone()).into();
    let mut out = attrs;
    out.extend(q("#[derive(Clone, Copy, Debug)]", &[]));
    out.extend(vis);
    out.extend(q(
        "struct __A0;
        #[allow(dead_code)]
        impl __A0 { __A1 }
        impl ::corten::assets::Embed for __A0 {
            const DIR: &'static str = if ::core::cfg!(debug_assertions) { __A2 } else { \"\" };
            const OWNER: &'static str = ::core::concat!(::core::module_path!(), \"::\", __A4);
            const FILES: &'static [(&'static str, &'static [u8])] = &[__A3];
        }",
        &[ident, consts, literal(dir_text), list, literal(&name.to_string())],
    ));
    Ok(out)
}

/// Each file under `dir`: its path from `root` with `/` (its URL), and its path on disk.
/// Dotfiles (`.env`, `.DS_Store`, editor swap files) are never public.
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() && path.is_dir() {
            return Err(std::io::Error::other(format!(
                "{} is a linked directory, which is not followed; move its files in",
                path.display()
            )));
        }
        if kind.is_dir() {
            collect(root, &path, out)?;
        } else {
            let relative = path.strip_prefix(root).expect("walked from root");
            let url = relative.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            out.push((url, path));
        }
    }
    Ok(())
}

/// The first character of `url` a URL path cannot carry as it is, if any.
fn unsafe_in_url(url: &str) -> Option<char> {
    url.chars().find(|&c| !(c.is_ascii_alphanumeric() || "-_.~@+,=!/".contains(c)))
}

/// `icons/192.png` is `ICONS_192_PNG`; a name that starts with a digit gets a `_`.
pub fn constant_name(url: &str) -> String {
    let mut out = String::with_capacity(url.len() + 1);
    for c in url.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_uppercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{out}")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{constant_name, unsafe_in_url};

    #[test]
    fn a_file_name_is_a_url_as_it_is() {
        assert_eq!(unsafe_in_url("icons/192.png"), None);
        assert_eq!(unsafe_in_url("cards/item-1_v2.svg"), None);
        assert_eq!(unsafe_in_url("a b.css"), Some(' '));
        assert_eq!(unsafe_in_url("a#b.css"), Some('#'));
        assert_eq!(unsafe_in_url("50%.png"), Some('%'));
        assert_eq!(unsafe_in_url("caf\u{e9}.png"), Some('\u{e9}'));
    }

    #[test]
    fn a_file_s_constant_is_its_path_in_capitals() {
        assert_eq!(constant_name("style.css"), "STYLE_CSS");
        assert_eq!(constant_name("icons/192.png"), "ICONS_192_PNG");
        assert_eq!(constant_name("cards/item-1.svg"), "CARDS_ITEM_1_SVG");
        assert_eq!(constant_name("404.svg"), "_404_SVG");
        assert_eq!(constant_name("a--b.js"), "A_B_JS");
    }
}
