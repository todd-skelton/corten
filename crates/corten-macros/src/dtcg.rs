//! The W3C Design Tokens format, 2025.10 (its Format, Color and Resolver modules), read for
//! `tokens!`: a token file, or a resolver document that puts token files together under
//! modifiers (a theme, a density). Tokens become CSS values; a reference stays a reference,
//! `var(--color-accent)`, so a theme that changes the token it names changes it too. A type CSS
//! has no one value for (a gradient has no direction, a typography token is several properties)
//! and anything the format forbids are errors that say so.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

use crate::json::{self, Value};

/// Corten's key under `$extensions`, in the reverse-domain notation DTCG recommends: breakpoints
/// and utilities on a token file's root group, the media condition a context follows on a modifier.
pub const VENDOR: &str = "run.corten";

const TYPES: &[&str] = &[
    "color",
    "dimension",
    "fontFamily",
    "fontWeight",
    "duration",
    "cubicBezier",
    "number",
    "strokeStyle",
    "border",
    "transition",
    "shadow",
    "gradient",
    "typography",
];

/// A token, resolved.
pub struct Token {
    /// `["color", "accent"]`; a group's `$root` token has the group's path.
    pub path: Vec<String>,
    /// The top-level group's name as the file writes it (`Brand Colors` where `path` has
    /// `brand-colors`), which a utility's `group` names.
    pub group: String,
    pub ty: String,
    pub css: String,
    pub description: Option<String>,
}

impl Token {
    /// `--color-accent`.
    pub fn property(&self) -> String {
        format!("--{}", self.path.join("-"))
    }
}

/// A resolver's modifier: each context with the tokens it sets, the default first (the default
/// values of every token another context changes).
pub struct Modifier {
    pub name: String,
    pub default: String,
    /// (context, [(custom property, value)]), in the modifier's order.
    pub contexts: Vec<(String, Vec<(String, String)>)>,
    /// Its `$extensions` under [`VENDOR`].
    pub extension: Option<Value>,
}

pub struct Tokens {
    /// With every modifier at its default.
    pub tokens: Vec<Token>,
    /// The root group's `$extensions` under [`VENDOR`].
    pub extension: Option<Value>,
    pub modifiers: Vec<Modifier>,
    /// Every file read, so the crate rebuilds when one changes.
    pub files: Vec<PathBuf>,
}

/// A resolver document (one with `resolutionOrder`) or a token file.
pub fn read(file: &Path) -> Result<Tokens, String> {
    let mut files = Files::default();
    let root = files.load(file, &file.display().to_string())?;
    // A resolver has a resolutionOrder array, which no token file can (its members are groups and
    // tokens, objects); the `.resolver.json` name is only recommended.
    let resolver = matches!(&*root, Value::Object(entries) if matches!(field(entries, "resolutionOrder"), Some(Value::Array(_))));
    let mut tokens = if resolver { resolver_document(&root, file.parent(), &mut files)? } else { token_file(&root)? };
    tokens.files = files.read.into_iter().map(|(path, _)| path).collect();
    Ok(tokens)
}

pub fn token_file(root: &Value) -> Result<Tokens, String> {
    WORK.with(|work| work.set(0));
    let (tokens, extension) = resolve(root)?;
    Ok(Tokens { tokens, extension, modifiers: Vec::new(), files: Vec::new() })
}

/// A resolver document; `dir` is where the files it names are (`None`: it names none).
pub fn resolver_document(root: &Value, dir: Option<&Path>, files: &mut Files) -> Result<Tokens, String> {
    WORK.with(|work| work.set(0));
    Resolver { doc: root, shared: Rc::new(root.clone()), dir, files }.read()
}

#[derive(Default)]
pub struct Files {
    /// Shared: a reference into a file holds it, not a copy.
    read: Vec<(PathBuf, Rc<Value>)>,
    /// Where each path is in `read`.
    at: HashMap<PathBuf, usize>,
}

impl Files {
    fn load(&mut self, path: &Path, name: &str) -> Result<Rc<Value>, String> {
        if let Some(&i) = self.at.get(path) {
            return Ok(self.read[i].1.clone());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {name}: {e}"))?;
        let value = Rc::new(json::parse(&text).map_err(|e| format!("{name}: {e}"))?);
        self.at.insert(path.to_path_buf(), self.read.len());
        self.read.push((path.to_path_buf(), value.clone()));
        Ok(value)
    }
}

fn object<'a>(value: &'a Value, what: &str) -> Result<&'a [(String, Value)], String> {
    match value {
        Value::Object(entries) => Ok(entries),
        other => Err(format!("{what} is an object, not {}", other.kind())),
    }
}

fn string<'a>(value: &'a Value, what: &str) -> Result<&'a str, String> {
    match value {
        Value::String(s) => Ok(s),
        other => Err(format!("{what} is a string, not {}", other.kind())),
    }
}

fn field<'a>(entries: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// A token (an object with `$value`, or `$ref` to one) rather than a group.
fn is_token(entries: &[(String, Value)]) -> bool {
    entries.iter().any(|(k, _)| k == "$value" || k == "$ref")
}

/// Puts `from` over `into`, as the resolver and `$extends` do: a group merges with a group,
/// `$extensions` merges by vendor, and anything else (a token, `$type`) replaces what was there.
fn merge(into: &mut Vec<(String, Value)>, from: Vec<(String, Value)>) {
    // An index of the keys there, so a group of n members merges in O(n), not O(n²).
    let mut at: HashMap<String, usize> = into.iter().enumerate().map(|(i, (k, _))| (k.clone(), i)).collect();
    for (key, value) in from {
        let Some(&i) = at.get(&key) else {
            at.insert(key.clone(), into.len());
            into.push((key, value));
            continue;
        };
        let old = &mut into[i].1;
        let deep = key == "$extensions" || !key.starts_with('$');
        match (old, value) {
            (Value::Object(old), Value::Object(new)) if deep && (key == "$extensions" || !is_token(old) && !is_token(&new)) => {
                merge(old, new)
            }
            (old, value) => *old = value,
        }
    }
}

// The resolver.

struct Resolver<'a, 'f> {
    doc: &'a Value,
    /// `doc`, shared by each [`Doc`] that is the resolver.
    shared: Rc<Value>,
    dir: Option<&'a Path>,
    files: &'f mut Files,
}

/// A document a reference is in: its file's folder and contents, so a reference in a file the
/// resolver names is relative to that file.
#[derive(Clone)]
struct Doc {
    value: Rc<Value>,
    dir: Option<PathBuf>,
    /// The file, canonical, so `a.json` and `./a.json` are one; `None` for the resolver itself.
    file: Option<PathBuf>,
}

enum Item {
    Set(Vec<Value>),
    Modifier { name: String, contexts: Vec<(String, Vec<Value>)>, default: String, extension: Option<Value> },
}

impl Resolver<'_, '_> {
    fn read(mut self) -> Result<Tokens, String> {
        let root = object(self.doc, "the resolver document")?;
        for (key, value) in root {
            match key.as_str() {
                "$schema" | "name" | "description" => {
                    string(value, key)?;
                }
                "sets" | "modifiers" | "resolutionOrder" | "$defs" => {}
                "version" => {
                    let version = string(value, "version")?;
                    if version != "2025.10" {
                        return Err(format!("version is {version:?}; this resolver reads \"2025.10\""));
                    }
                }
                _ => return Err(format!("unknown property {key:?} of a resolver document")),
            }
        }
        if field(root, "version").is_none() {
            return Err("the resolver document has no version (\"2025.10\")".into());
        }
        let Some(order) = field(root, "resolutionOrder") else {
            return Err("the resolver document has no resolutionOrder".into());
        };
        if matches!(order, Value::Array(items) if items.is_empty()) {
            return Err("resolutionOrder is empty; it names at least one set or modifier".into());
        }
        let Value::Array(order) = order else { return Err(format!("resolutionOrder is an array, not {}", order.kind())) };
        let mut items = Vec::new();
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (n, item) in order.iter().enumerate() {
            let at = format!("resolutionOrder/{n}");
            let fields = object(item, &at)?;
            let (name, kind, body) = match field(fields, "$ref") {
                Some(target) => {
                    let target = string(target, &format!("{at}.$ref"))?;
                    let pointer = fragment(&resolver_fragment(target.split_once('#').map_or("", |(_, p)| p)), &at)?;
                    let kind = if pointer.starts_with("/sets/") {
                        "set"
                    } else if pointer.starts_with("/modifiers/") {
                        "modifier"
                    } else {
                        return Err(format!("{at}: {target:?} is not a set or a modifier"));
                    };
                    let root = self.root();
                    let (body, pointer, doc) = self.reference(&root, target, fields, &at, true)?;
                    (unescape(pointer.rsplit('/').next().unwrap_or_default()), kind.to_string(), (body, doc))
                }
                None => {
                    let what = |key: &str| format!("{at}.{key}");
                    let name = field(fields, "name").ok_or_else(|| format!("{at} has no name"))?;
                    let kind = field(fields, "type").ok_or_else(|| format!("{at} has no type (\"set\" or \"modifier\")"))?;
                    let rest = fields.iter().filter(|(k, _)| k != "name" && k != "type").cloned().collect();
                    let body = (Value::Object(rest), self.root());
                    (string(name, &what("name"))?.to_string(), string(kind, &what("type"))?.to_string(), body)
                }
            };
            if !names.insert(name.clone()) {
                return Err(format!("{at}: two items of resolutionOrder are named {name:?}"));
            }
            let at = format!("{at} ({kind} {name})");
            items.push(match kind.as_str() {
                "set" => Item::Set(self.set(&body.0, &body.1, &at)?),
                "modifier" => self.modifier(name, &body.0, &body.1, &at)?,
                other => return Err(format!("{at}: type is {other:?}; it is \"set\" or \"modifier\"")),
            });
        }
        // Checked before anything is resolved: each context, and each combination of contexts,
        // is resolved once.
        let total = items
            .iter()
            .filter_map(|item| match item {
                Item::Modifier { contexts, .. } => Some(contexts.len() + 1),
                Item::Set(_) => None,
            })
            .try_fold(1usize, |all, n| all.checked_mul(n))
            .unwrap_or(usize::MAX);
        if total > MAX_COMBINATIONS {
            return Err(format!(
                "the modifiers have {total} combinations of contexts (each absent or chosen); tokens! checks each one's CSS, up to {MAX_COMBINATIONS}"
            ));
        }
        let (tokens, extension) = resolve(&layers(&items, &[]))?;
        let mut modifiers = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let Item::Modifier { name, contexts, default, extension: own } = item else { continue };
            let mut changes = Vec::new();
            for (context, _) in contexts.iter().filter(|(c, _)| c != default) {
                let at = format!("modifier {name}, context {context}");
                let (other, other_extension) = resolve(&layers(&items, &[(i, context)])).map_err(|e| format!("{at}: {e}"))?;
                if other_extension != extension {
                    return Err(format!(
                        "{at} changes $extensions.\"{VENDOR}\" on the root group: breakpoints and utilities are the same in every context"
                    ));
                }
                changes.push((context.clone(), diff(&tokens, &other, &at)?));
            }
            // The default context declares, with their default values, the tokens the others
            // change, so `data-theme="light"` inside a dark element, or on a page whose system
            // is dark, is light there.
            let mut back: Vec<(String, String)> = Vec::new();
            for token in &tokens {
                if changes.iter().any(|(_, vars)| vars.iter().any(|(property, _)| *property == token.property())) {
                    back.push((token.property(), token.css.clone()));
                }
            }
            readers(&mut back, &tokens);
            changes.insert(0, (default.clone(), back));
            modifiers.push(Modifier { name: name.clone(), default: default.clone(), contexts: changes, extension: own.clone() });
        }
        combinations(&items, &tokens, &modifiers)?;
        // Each definition is checked whether or not resolutionOrder names it.
        let root = object(self.doc, "the resolver document")?;
        for (key, value) in root {
            for (name, body) in match key.as_str() {
                "sets" | "modifiers" => object(value, key)?,
                _ => continue,
            } {
                let (doc, at) = (self.root(), format!("{key}.{name}"));
                if key == "sets" {
                    self.set(body, &doc, &at)?;
                } else {
                    self.modifier(name.clone(), body, &doc, &at)?;
                }
            }
        }
        Ok(Tokens { tokens, extension, modifiers, files: Vec::new() })
    }

    fn root(&self) -> Doc {
        Doc { value: self.shared.clone(), dir: self.dir.map(Path::to_path_buf), file: None }
    }

    fn set(&mut self, body: &Value, doc: &Doc, at: &str) -> Result<Vec<Value>, String> {
        let fields = object(body, at)?;
        for (key, value) in fields {
            match key.as_str() {
                "description" => {
                    string(value, &format!("{at}.description"))?;
                }
                "sources" => {}
                "$extensions" => no_vendor(value, at, "a set")?,
                _ => return Err(format!("{at}: unknown property {key:?} of a set")),
            }
        }
        let sources = field(fields, "sources").ok_or_else(|| format!("{at} has no sources"))?;
        self.sources(sources, doc, at, &mut Vec::new())
    }

    fn modifier(&mut self, name: String, body: &Value, doc: &Doc, at: &str) -> Result<Item, String> {
        let fields = object(body, at)?;
        let mut extension = None;
        for (key, value) in fields {
            match key.as_str() {
                "description" => {
                    string(value, &format!("{at}.description"))?;
                }
                "contexts" | "default" => {}
                "$extensions" => extension = field(object(value, &format!("{at}.$extensions"))?, VENDOR).cloned(),
                _ => return Err(format!("{at}: unknown property {key:?} of a modifier")),
            }
        }
        let contexts = field(fields, "contexts").ok_or_else(|| format!("{at} has no contexts"))?;
        let mut out = Vec::new();
        for (context, sources) in object(contexts, &format!("{at}.contexts"))? {
            out.push((context.clone(), self.sources(sources, doc, &format!("{at}, context {context}"), &mut Vec::new())?));
        }
        if out.len() < 2 {
            return Err(format!("{at} has {} context; a modifier has two or more (one is a set)", out.len()));
        }
        let default = field(fields, "default")
            .ok_or_else(|| {
                format!(
                    "{at} has no default. A resolver asks for an input to a modifier without one; tokens! takes each default as its input, the values of a page that chooses no context"
                )
            })?;
        let default = string(default, &format!("{at}.default"))?.to_string();
        if !out.iter().any(|(c, _)| *c == default) {
            return Err(format!("{at}: the default {default:?} is not one of its contexts"));
        }
        Ok(Item::Modifier { name, contexts: out, default, extension })
    }

    /// Token sources: inline groups, files, and sets (whose sources are inlined in their place).
    fn sources(&mut self, list: &Value, doc: &Doc, at: &str, sets: &mut Vec<String>) -> Result<Vec<Value>, String> {
        let Value::Array(list) = list else { return Err(format!("{at}: sources are an array, not {}", list.kind())) };
        let mut out = Vec::new();
        for (n, source) in list.iter().enumerate() {
            let at = format!("{at}, source {n}");
            let fields = object(source, &at)?;
            let Some(target) = field(fields, "$ref") else {
                out.push(source.clone());
                continue;
            };
            let target = string(target, &format!("{at}.$ref"))?;
            let (value, pointer, inner_doc) = self.reference(doc, target, fields, &at, false)?;
            // A set is `/sets/{name}` in a resolver, or there with a sources array in another file;
            // a token file may have a group named `sets`, and a group has no array in it.
            let in_resolver = matches!(&*inner_doc.value, Value::Object(root) if field(root, "resolutionOrder").is_some());
            let set = pointer.strip_prefix("/sets/").is_some_and(|name| !name.contains('/'))
                && (in_resolver
                    || matches!(&value, Value::Object(entries) if matches!(field(entries, "sources"), Some(Value::Array(_)))));
            // A pointer in the source is resolved, as an alias is, after the sources are merged
            // (Resolver 6.3; `#/x/$value` is `{x}`), so a later source's value wins.
            if !set {
                object(&value, &format!("{at}: {target:?}"))?;
                // A part of a file is a token tree of its own: a JSON pointer in it names a place
                // in its file, so one into the part is moved to the part's root, and one outside
                // it is what it points at.
                let value = if pointer.is_empty() {
                    value
                } else {
                    rebase(&value, &inner_doc.value, &pointer, &at, &mut Vec::new(), false)?
                };
                out.push(value);
                continue;
            }
            let key = format!("{}#{pointer}", inner_doc.file.as_deref().map(|d| d.display().to_string()).unwrap_or_default());
            if sets.contains(&key) {
                return Err(format!("{at}: {target:?} includes itself"));
            }
            if sets.len() == MAX_CHAIN {
                return Err(format!(
                    "{at}: a chain of more than {MAX_CHAIN} sets including one another; tokens! follows {MAX_CHAIN}"
                ));
            }
            spend(&at)?;
            sets.push(key);
            let inner = field(object(&value, &at)?, "sources").ok_or_else(|| format!("{at}: {target:?} has no sources"))?;
            out.extend(self.sources(inner, &inner_doc, &at, sets)?);
            sets.pop();
        }
        Ok(out)
    }

    /// What a reference object names, with the keys beside `$ref` over it, and its pointer.
    fn reference(
        &mut self,
        from: &Doc,
        target: &str,
        fields: &[(String, Value)],
        at: &str,
        from_order: bool,
    ) -> Result<(Value, String, Doc), String> {
        let (file, pointer) = target.split_once('#').unwrap_or((target, ""));
        let pointer = &resolver_fragment(pointer);
        spend(at)?;
        // A relative path, found from the file that names it: no URL or scheme (`https:`,
        // `file:`), no drive (`C:`) and no absolute path, so the files are the project's own.
        if file.contains(':') || file.starts_with(['/', '\\']) || Path::new(file).is_absolute() {
            return Err(format!(
                "{at}: {target:?} is not a path relative to the file that names it; tokens! reads no URLs or absolute paths"
            ));
        }
        // `#/` is the document root, as the Format module writes it.
        let pointer = if pointer == "/" { "" } else { pointer };
        let pointer = fragment(pointer, at)?;
        let pointer = pointer.as_str();
        if pointer == "/resolutionOrder" || pointer.starts_with("/resolutionOrder/") {
            return Err(format!("{at}: {target:?}: nothing may reference resolutionOrder"));
        }
        if pointer.starts_with("/modifiers/") && !from_order {
            return Err(format!("{at}: {target:?}: only resolutionOrder may reference a modifier"));
        }
        let doc = if file.is_empty() {
            from.clone()
        } else {
            let dir =
                from.dir.as_deref().ok_or_else(|| format!("{at}: {target:?} names a file, and this document has no folder"))?;
            let joined = dir.join(file);
            let path = std::fs::canonicalize(&joined).unwrap_or(joined);
            let value = self.files.load(&path, file)?;
            Doc { value, dir: path.parent().map(Path::to_path_buf), file: Some(path) }
        };
        let mut value: &Value = &doc.value;
        for segment in pointer.split('/').skip(1) {
            let segment = unescape(segment);
            value = match value {
                Value::Object(entries) => field(entries, &segment),
                Value::Array(items) => index(&segment).and_then(|n| items.get(n)),
                _ => None,
            }
            .ok_or_else(|| format!("{at}: {target:?} names nothing"))?;
        }
        let mut value = value.clone();
        let overrides: Vec<(String, Value)> = fields.iter().filter(|(k, _)| k != "$ref").cloned().collect();
        if !overrides.is_empty() {
            let Value::Object(entries) = &mut value else { return Err(format!("{at}: {target:?} is not an object to extend")) };
            for (key, new) in overrides {
                match entries.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, old)) => *old = new,
                    None => entries.push((key, new)),
                }
            }
        }
        Ok((value, pointer.to_string(), doc))
    }
}

/// `value`, taken from `doc` at `part` (`/tokens`), with each `#/...` pointer in it made relative
/// to the part: `#/tokens/a/$value` is `#/a/$value`, and a pointer outside the part is replaced by
/// the value it names.
fn rebase(value: &Value, doc: &Value, part: &str, at: &str, chain: &mut Vec<String>, in_value: bool) -> Result<Value, String> {
    let Value::Object(entries) = value else {
        return match value {
            Value::Array(items) => Ok(Value::Array(
                items.iter().map(|item| rebase(item, doc, part, at, chain, in_value)).collect::<Result<_, String>>()?,
            )),
            other => Ok(other.clone()),
        };
    };
    let mut out = Vec::new();
    for (key, item) in entries {
        let pointer = match (key.as_str(), item) {
            ("$ref", Value::String(text)) => text.strip_prefix('#').map(|p| fragment(p, at)).transpose()?,
            _ => None,
        };
        // `$extensions` holds any JSON a tool likes, a `$ref` included: not a reference here.
        if key == "$extensions" {
            out.push((key.clone(), item.clone()));
            continue;
        }
        let Some(pointer) = pointer.as_deref() else {
            out.push((key.clone(), rebase(item, doc, part, at, chain, in_value || key == "$value")?));
            continue;
        };
        if let Some(rest) = pointer.strip_prefix(part).filter(|rest| rest.is_empty() || rest.starts_with('/')) {
            // Decoded, so a % in a name is written as an escape again.
            out.push((key.clone(), Value::String(format!("#{}", rest.replace('%', "%25")))));
            continue;
        }
        let target = format!("#{pointer}");
        follow(chain, &target, at)?;
        let mut found = doc;
        for segment in pointer.split('/').skip(1) {
            let segment = unescape(segment);
            found = match found {
                Value::Object(inner) => field(inner, &segment),
                Value::Array(items) => index(&segment).and_then(|n| items.get(n)),
                _ => None,
            }
            .ok_or_else(|| format!("{at}: \"#{pointer}\" names nothing in its file"))?;
        }
        // A whole token outside the part is its value, or what its own `$ref` names.
        let found = match found {
            Value::Object(inner) if is_token(inner) => match field(inner, "$value") {
                Some(value) => rebase(value, doc, part, at, chain, true),
                None => {
                    let by_ref = Value::Object(inner.iter().filter(|(k, _)| k == "$ref").cloned().collect());
                    rebase(&by_ref, doc, part, at, chain, true)
                }
            },
            other => rebase(other, doc, part, at, chain, true),
        };
        chain.pop();
        let found = found?;
        if in_value {
            // A reference object, `{"$ref": ...}`, in a value: the value itself.
            return Ok(found);
        }
        // A token's own `$ref`: its value.
        out.push(("$value".into(), found));
    }
    Ok(Value::Object(out))
}

/// The longest chain of references or `$extends` followed: each link is a call, and a longer
/// chain could overflow the compiler's stack. A file needs far fewer.
const MAX_CHAIN: usize = 256;

thread_local! {
    static WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The most references, `$extends`, sets and file references one `tokens!` follows in all: putting a resolver's
/// sources together and resolving the defaults, each context and each combination of contexts. Each copies what it names, so a file
/// naming one thing twice at each of many levels would otherwise take work exponential in its
/// size; the examples follow none (their references are `{...}`, which copy nothing).
const MAX_WORK: usize = 100_000;

/// Counts one reference, `$extends` or set followed against [`MAX_WORK`].
fn spend(at: &str) -> Result<(), String> {
    WORK.with(|work| {
        work.set(work.get() + 1);
        if work.get() > MAX_WORK {
            return Err(format!(
                "{at}: more than {MAX_WORK} references, $extends and sets followed in all, as names of names repeat; tokens! follows {MAX_WORK}"
            ));
        }
        Ok(())
    })
}

/// Adds `target` to the chain of pointers being followed: one already on it is a circle.
fn follow(chain: &mut Vec<String>, target: &str, at: &str) -> Result<(), String> {
    spend(at)?;
    if chain.iter().any(|t| t == target) {
        return Err(format!("{at}: its JSON pointers go round in a circle through {target:?}"));
    }
    if chain.len() == MAX_CHAIN {
        return Err(format!("{at}: a chain of more than {MAX_CHAIN} JSON pointers; tokens! follows {MAX_CHAIN}"));
    }
    chain.push(target.to_string());
    Ok(())
}

/// The JSON pointer a URI fragment holds (RFC 6901, section 6): percent-escapes decoded, and
/// each `~` followed by `0` or `1`, so `#/Brand%20Primary` is `/Brand Primary`.
fn fragment(text: &str, at: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).filter(|h| h.iter().all(u8::is_ascii_hexdigit));
            let hex = hex.ok_or_else(|| format!("{at}: {text:?}: % is followed by two hex digits in a URI"))?;
            out.push(u8::from_str_radix(std::str::from_utf8(hex).expect("ASCII"), 16).expect("hex digits"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let pointer = String::from_utf8(out).map_err(|_| format!("{at}: {text:?} decodes to bytes that are not UTF-8"))?;
    let mut chars = pointer.chars();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return Err(format!("{at}: {text:?}: in a JSON pointer ~ is followed by 0 (~) or 1 (/)"));
        }
    }
    Ok(pointer)
}

/// A resolver reference's fragment: the Resolver module also writes one without its first `/`
/// (`example.json#sets/MySet`, 4.2), which is the same place.
fn resolver_fragment(fragment: &str) -> String {
    if fragment.is_empty() || fragment.starts_with('/') {
        fragment.to_string()
    } else {
        format!("/{fragment}")
    }
}

/// An array index in a JSON pointer (RFC 6901): `0` or digits with no leading zero.
fn index(segment: &str) -> Option<usize> {
    let digits = !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit());
    (digits && (segment == "0" || !segment.starts_with('0'))).then(|| segment.parse().ok()).flatten()
}

/// A JSON pointer segment: `~1` is `/` and `~0` is `~`.
fn unescape(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

/// The token tree with the picked modifiers (by item) at those contexts, the rest at their defaults.
fn layers(items: &[Item], picks: &[(usize, &str)]) -> Value {
    let mut tree = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let sources = match item {
            Item::Set(sources) => sources,
            Item::Modifier { contexts, default, .. } => {
                let context = picks.iter().find(|(at, _)| *at == i).map_or(default.as_str(), |(_, c)| c);
                &contexts.iter().find(|(c, _)| c == context).expect("a context of the modifier").1
            }
        };
        for source in sources {
            if let Value::Object(entries) = source {
                merge(&mut tree, entries.clone());
            }
        }
    }
    Value::Object(tree)
}

/// The most combinations of contexts [`combinations`] resolves.
const MAX_COMBINATIONS: usize = 256;

/// On one element, each modifier's rule for its context applies in resolutionOrder, the later
/// winning. That is the resolver's answer unless a context sets a token by value (or copies, by a
/// pointer into a value, a value) that another modifier changes. So each combination of two or
/// more chosen contexts, a chosen default included, is resolved and compared with what the rules
/// give, and a file whose CSS would be wrong there does not compile.
fn combinations(items: &[Item], tokens: &[Token], modifiers: &[Modifier]) -> Result<(), String> {
    let at_item: Vec<usize> =
        items.iter().enumerate().filter(|(_, item)| matches!(item, Item::Modifier { .. })).map(|(i, _)| i).collect();
    // Each modifier's attribute is absent or names one of its contexts, the default included: an
    // explicit default is its own rule, and can override another modifier's. `read` has checked
    // there are at most MAX_COMBINATIONS.
    let root: HashMap<String, &str> = tokens.iter().map(|t| (t.property(), t.css.as_str())).collect();
    let rules: Vec<Vec<HashMap<&str, &str>>> = modifiers
        .iter()
        .map(|m| m.contexts.iter().map(|(_, vars)| vars.iter().map(|(p, v)| (p.as_str(), v.as_str())).collect()).collect())
        .collect();
    // What the resolver gives for each choice of contexts (an absent attribute is the default).
    let mut resolved: HashMap<Vec<usize>, Vec<Token>> = HashMap::new();
    // 0 is absent; c + 1 is context c, the default being context 0.
    let mut pick = vec![0usize; modifiers.len()];
    loop {
        let Some(m) = pick.iter().zip(modifiers).position(|(c, m)| *c < m.contexts.len()) else { return Ok(()) };
        pick[m] += 1;
        pick[..m].fill(0);
        if pick.iter().filter(|c| **c != 0).count() < 2 {
            continue;
        }
        let contexts: Vec<usize> = pick.iter().map(|c| c.saturating_sub(1)).collect();
        let named = pick
            .iter()
            .zip(modifiers)
            .filter(|(c, _)| **c != 0)
            .map(|(&c, m)| format!("{} {}", m.name, m.contexts[c - 1].0))
            .collect::<Vec<_>>()
            .join(" and ");
        if !resolved.contains_key(&contexts) {
            let picks: Vec<(usize, &str)> =
                contexts.iter().enumerate().map(|(m, &c)| (at_item[m], modifiers[m].contexts[c].0.as_str())).collect();
            let (want, _) = resolve(&layers(items, &picks)).map_err(|e| format!("modifiers at {named}: {e}"))?;
            resolved.insert(contexts.clone(), want);
        }
        for token in &resolved[&contexts] {
            let property = token.property();
            let mut css = root.get(&property).copied().unwrap_or_default();
            for (m, &c) in pick.iter().enumerate() {
                if let Some(value) = c.checked_sub(1).and_then(|c| rules[m][c].get(property.as_str())) {
                    css = value;
                }
            }
            if css != token.css {
                return Err(format!(
                    "with {named}, {property} resolves to {} and the CSS rules give {css}: one modifier's rule sets or declares again a token another modifier changes (a chosen default declares again what its other contexts change); give each modifier tokens of its own, and reference them ({{...}}) rather than copy their values",
                    token.css
                ));
            }
        }
    }
}

/// What a context changes: the same tokens as the defaults, some with other values.
fn diff(base: &[Token], other: &[Token], at: &str) -> Result<Vec<(String, String)>, String> {
    let in_base: std::collections::HashSet<&[String]> = base.iter().map(|b| b.path.as_slice()).collect();
    let in_other: HashMap<&[String], &Token> = other.iter().map(|o| (o.path.as_slice(), o)).collect();
    for token in other {
        if !in_base.contains(token.path.as_slice()) {
            return Err(format!(
                "{at} defines {}, which the defaults do not: a token has a value when no context is chosen",
                token.path.join(".")
            ));
        }
    }
    let mut out = Vec::new();
    for token in base {
        let Some(&changed) = in_other.get(token.path.as_slice()) else {
            return Err(format!("{at} has no {}, which the defaults define", token.path.join(".")));
        };
        if changed.ty != token.ty {
            return Err(format!("{at}: {} is {}, and {} by default", token.path.join("."), changed.ty, token.ty));
        }
        if changed.css != token.css {
            out.push((token.property(), changed.css.clone()));
        }
    }
    // A custom property that reads another is computed where it is declared, so a context set on
    // an element inside the page declares again each token that reads one it changes; otherwise
    // `--color-link: var(--color-ink)` would keep `:root`'s ink there.
    readers(&mut out, other);
    Ok(out)
}

/// Adds each token of `tokens` that reads one in `out`, and each that reads one of those, in the
/// order of `tokens` at each step: a search over who reads whom, so a long chain stays linear.
fn readers(out: &mut Vec<(String, String)>, tokens: &[Token]) {
    let mut read_by: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, token) in tokens.iter().enumerate() {
        for (n, _) in token.css.match_indices("var(--") {
            let name = &token.css[n + 4..];
            let end = name.find([')', ',']).unwrap_or(name.len());
            read_by.entry(&name[..end]).or_default().push(i);
        }
    }
    let mut have: std::collections::HashSet<String> = out.iter().map(|(property, _)| property.clone()).collect();
    let mut next: Vec<String> = out.iter().map(|(property, _)| property.clone()).collect();
    while !next.is_empty() {
        let mut found: Vec<usize> = next.iter().filter_map(|p| read_by.get(p.as_str())).flatten().copied().collect();
        found.sort_unstable();
        found.dedup();
        next.clear();
        for i in found {
            let property = tokens[i].property();
            if have.insert(property.clone()) {
                out.push((property.clone(), tokens[i].css.clone()));
                next.push(property);
            }
        }
    }
}

fn no_vendor(extensions: &Value, at: &str, place: &str) -> Result<(), String> {
    if field(object(extensions, &format!("{at}.$extensions"))?, VENDOR).is_some() {
        return Err(format!(
            "{at}: $extensions.\"{VENDOR}\" on {place} means nothing; it goes on the root group of a token file or on a modifier"
        ));
    }
    Ok(())
}

// A token tree.

/// The tokens of a tree and its root's `$extensions` under [`VENDOR`].
fn resolve(root: &Value) -> Result<(Vec<Token>, Option<Value>), String> {
    let Value::Object(entries) = root else { return Err(format!("a token file is an object, not {}", root.kind())) };
    let tree = extend(entries, entries, &[], &mut Vec::new())?;
    let mut raws = Vec::new();
    walk(&tree, &tree, &[], None, &mut raws)?;
    let extension = match field(&tree, "$extensions") {
        Some(extensions) => field(object(extensions, "$extensions")?, VENDOR).cloned(),
        None => None,
    };
    let mut names = Vec::new();
    for raw in &raws {
        let what = format!("token {}: name", dotted(&raw.at));
        names.push(raw.path.iter().map(|s| slug(s, &what)).collect::<Result<Vec<_>, _>>()?);
    }
    let index = raws.iter().enumerate().map(|(i, raw)| (raw.at.as_slice(), i)).collect();
    Ok((Book { raws: &raws, names, index }.tokens()?, extension))
}

/// A reference object that reads the value of the token `{a.b}` names: its `$value`, or where
/// its `$ref` points. `None` when it names no token (a group's root is `{a.$root}`, not `{a}`),
/// which the reference's own check reports.
fn value_pointer(tree: &[(String, Value)], path: &[String]) -> Option<Value> {
    let mut entries = tree;
    for segment in path {
        let Some(Value::Object(inner)) = field(entries, segment) else { return None };
        entries = inner;
    }
    if !is_token(entries) {
        return None;
    }
    let mut at = path.to_vec();
    if let Some(target) = field(entries, "$ref") {
        return Some(Value::Object(vec![("$ref".into(), target.clone())]));
    }
    at.push("$value".into());
    let escaped: Vec<String> = at.iter().map(|s| s.replace('~', "~0").replace('/', "~1").replace('%', "%25")).collect();
    Some(Value::Object(vec![("$ref".into(), Value::String(format!("#/{}", escaped.join("/"))))]))
}

fn dotted(path: &[String]) -> String {
    path.join(".")
}

/// A group with `$extends` replaced by what it inherits, the group's own tokens over them.
fn extend(
    root: &[(String, Value)],
    entries: &[(String, Value)],
    path: &[String],
    stack: &mut Vec<Vec<String>>,
) -> Result<Vec<(String, Value)>, String> {
    let mut out = Vec::new();
    if let Some(target) = field(entries, "$extends") {
        let at = format!("{}.$extends", dotted(path));
        spend(&at)?;
        let target = string(target, &at)?;
        let to = reference_path(target).ok_or_else(|| format!("{at}: {target:?} is not a reference"))?;
        if stack.len() == MAX_CHAIN {
            return Err(format!(
                "{at}: a chain of more than {MAX_CHAIN} groups extending one another; tokens! follows {MAX_CHAIN}"
            ));
        }
        if to.last().is_some_and(|s| s == "$root") || path.starts_with(&to) || stack.contains(&to) {
            return Err(format!("{at}: {target:?} is the group itself, a group around it or a token, or extends this group"));
        }
        let mut group = root;
        for segment in &to {
            group = match field(group, segment) {
                Some(Value::Object(inner)) if !is_token(inner) => inner,
                _ => return Err(format!("{at}: {target:?} is not a group")),
            };
        }
        stack.push(path.to_vec());
        out = extend(root, group, &to, stack)?;
        stack.pop();
    }
    let mut own = Vec::new();
    for (key, value) in entries {
        match value {
            _ if key == "$extends" => continue,
            Value::Object(inner) if !key.starts_with('$') && !is_token(inner) => {
                let mut inner_path = path.to_vec();
                inner_path.push(key.clone());
                own.push((key.clone(), Value::Object(extend(root, inner, &inner_path, stack)?)));
            }
            _ => own.push((key.clone(), value.clone())),
        }
    }
    merge(&mut out, own);
    Ok(out)
}

/// `{color.accent}` or `#/color/accent`: the path it names (`$root` included).
fn reference_path(text: &str) -> Option<Vec<String>> {
    if let Some(pointer) = text.strip_prefix('#') {
        let pointer = fragment(pointer, "").ok()?;
        return Some(pointer.strip_prefix('/')?.split('/').map(unescape).collect());
    }
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let path: Vec<String> = inner.split('.').map(String::from).collect();
    path.iter().all(|s| !s.is_empty() && !s.contains(['{', '}'])).then_some(path)
}

/// `"{color.accent}"`: the path, if the value is a reference.
fn alias(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::String(text) if text.starts_with('{') => reference_path(text),
        _ => None,
    }
}

struct Raw<'a> {
    /// Where a reference finds it: `["color", "accent", "$root"]` for a group's root token.
    at: Vec<String>,
    path: Vec<String>,
    body: Body,
    declared: Option<&'a str>,
    inherited: Option<&'a str>,
    description: Option<&'a str>,
}

enum Body {
    Value(Value),
    Alias(Vec<String>),
}

fn type_name<'a>(value: &'a Value, at: &str) -> Result<&'a str, String> {
    let name = string(value, at)?;
    if TYPES.contains(&name) {
        Ok(name)
    } else {
        Err(format!("{at}: {name:?} is not a type; the types are {}", TYPES.join(", ")))
    }
}

fn walk<'a>(
    tree: &'a [(String, Value)],
    entries: &'a [(String, Value)],
    path: &[String],
    inherited: Option<&'a str>,
    out: &mut Vec<Raw<'a>>,
) -> Result<(), String> {
    let at = |key: &str| if path.is_empty() { key.to_string() } else { format!("{}.{key}", dotted(path)) };
    let ty = match field(entries, "$type") {
        Some(value) => Some(type_name(value, &at("$type"))?),
        None => inherited,
    };
    for (key, value) in entries {
        match key.as_str() {
            "$type" => {}
            "$description" => {
                string(value, &at(key))?;
            }
            "$deprecated" => deprecated(value, &at(key))?,
            "$schema" if path.is_empty() => {
                string(value, key)?;
            }
            "$extensions" if path.is_empty() => {
                object(value, key)?;
            }
            "$extensions" => no_vendor(value, &dotted(path), "a group")?,
            // The format allows it; CSS has no name for it, as its path is empty.
            "$root" if path.is_empty() => {
                return Err("$root at the top of the file has no name for its custom property; put it in a group".into())
            }
            "$root" => {
                let mut token_at = path.to_vec();
                token_at.push(key.clone());
                token(tree, value, token_at, path.to_vec(), ty, out)?;
            }
            key if key.starts_with('$') => return Err(format!("{}: unknown property {key:?} of a group", at(key))),
            key => {
                if key.contains(['{', '}', '.']) {
                    return Err(format!("{}: a name cannot contain {{, }} or .", at(key)));
                }
                let inner = object(value, &at(key))?;
                let mut inner_path = path.to_vec();
                inner_path.push(key.to_string());
                if is_token(inner) {
                    token(tree, value, inner_path.clone(), inner_path, ty, out)?;
                } else {
                    walk(tree, inner, &inner_path, ty, out)?;
                }
            }
        }
    }
    Ok(())
}

fn deprecated(value: &Value, at: &str) -> Result<(), String> {
    match value {
        Value::Bool(_) | Value::String(_) => Ok(()),
        other => Err(format!("{at} is true, false or a string, not {}", other.kind())),
    }
}

fn token<'a>(
    tree: &[(String, Value)],
    value: &'a Value,
    at: Vec<String>,
    path: Vec<String>,
    inherited: Option<&'a str>,
    out: &mut Vec<Raw<'a>>,
) -> Result<(), String> {
    let name = dotted(&at);
    let entries = object(value, &name)?;
    let mut declared = None;
    let mut description = None;
    for (key, value) in entries {
        match key.as_str() {
            "$value" | "$ref" => {}
            "$type" => declared = Some(type_name(value, &format!("{name}.$type"))?),
            "$description" => description = Some(string(value, &format!("{name}.$description"))?),
            "$deprecated" => deprecated(value, &format!("{name}.$deprecated"))?,
            "$extensions" => no_vendor(value, &name, "a token")?,
            key if key.starts_with('$') => return Err(format!("{name}: unknown property {key:?} of a token")),
            key => return Err(format!("{name} is a token ($value) and a group ({key}) at once")),
        }
    }
    // The schema's font list items are names or reference objects; `{...}` is for a whole value.
    if let (Some("fontFamily"), Some(Value::Array(items))) = (declared.or(inherited), field(entries, "$value")) {
        if let Some(n) = items.iter().position(|item| alias(item).is_some()) {
            return Err(format!(
                "{name}.$value/{n}: a font list names a token with {{\"$ref\": \"#/...\"}}; {{...}} is a whole value"
            ));
        }
    }
    let body = match (field(entries, "$value"), field(entries, "$ref")) {
        (Some(_), Some(_)) => return Err(format!("{name} has both $value and $ref")),
        (Some(value), None) => {
            let value = pointers(tree, value, &format!("{name}.$value"), &mut Vec::new(), true)?;
            alias(&value).map_or(Body::Value(value), Body::Alias)
        }
        (None, Some(target)) => {
            let pointer = Value::Object(vec![("$ref".into(), target.clone())]);
            let value = pointers(tree, &pointer, &format!("{name}.$ref"), &mut Vec::new(), true)?;
            alias(&value).map_or(Body::Value(value), Body::Alias)
        }
        (None, None) => return Err(format!("{name} has no $value: a group's $root is a token")),
    };
    out.push(Raw { at, path, body, declared, inherited, description });
    Ok(())
}

/// A value with each JSON pointer in it, `{"$ref": "#/space/1/$value/value"}`, replaced by what
/// it points at, as the Format module's property-level references are. A pointer to a whole
/// token's value, `#/color/ink/$value`, is `"{color.ink}"`, so it stays `var(--color-ink)` and
/// follows a theme; one into a value is the value itself, written where it is used.
fn pointers(tree: &[(String, Value)], value: &Value, at: &str, chain: &mut Vec<String>, whole: bool) -> Result<Value, String> {
    match value {
        Value::Object(entries) if field(entries, "$ref").is_some() => {
            if entries.len() > 1 {
                return Err(format!("{at}: a reference object has only $ref"));
            }
            let target = string(field(entries, "$ref").expect("checked"), &format!("{at}.$ref"))?;
            follow(chain, target, at)?;
            let path = target
                .strip_prefix('#')
                .map(|p| fragment(p, at))
                .transpose()?
                .filter(|p| p.starts_with('/'))
                .map(|p| p[1..].split('/').map(unescape).collect::<Vec<_>>())
                .ok_or_else(|| format!("{at}: {target:?} is not a JSON pointer into this file (#/...)"))?;
            let mut found: Option<&Value> = None;
            let mut entries = tree;
            for segment in &path {
                let next = match found {
                    None => field(entries, segment),
                    Some(Value::Object(inner)) => field(inner, segment),
                    Some(Value::Array(items)) => index(segment).and_then(|n| items.get(n)),
                    Some(_) => None,
                };
                found = Some(next.ok_or_else(|| format!("{at}: {target:?} names nothing"))?);
                if let Some(Value::Object(inner)) = found {
                    entries = inner;
                }
            }
            let found = found.ok_or_else(|| format!("{at}: {target:?} names nothing"))?;
            // `#/color/ink` or `#/color/ink/$value` names a whole token: `{color.ink}`, so it is
            // `var(--color-ink)` and follows a theme. A number token in a field that takes a
            // number (a color's component) has no var() there, so it is the number.
            let values = path.iter().filter(|s| *s == "$value").count();
            let token = match (values, path.last().map(String::as_str)) {
                (0, _) => matches!(found, Value::Object(inner) if is_token(inner)).then(|| path.clone()),
                (1, Some("$value")) => Some(path[..path.len() - 1].to_vec()),
                _ => None,
            };
            if let Some(token) = token {
                let value = match found {
                    Value::Object(inner) if values == 0 => field(inner, "$value"),
                    other => Some(other),
                };
                let primitive = matches!(value, Some(Value::Number(_) | Value::Bool(_) | Value::Null));
                // In a field, a token that is itself a reference is followed to the token with an
                // explicit value (Format 7.2.2); a primitive there is written where it is used.
                let by_ref = match found {
                    Value::Object(inner) if values == 0 => {
                        field(inner, "$ref").map(|t| Value::Object(vec![("$ref".into(), t.clone())]))
                    }
                    _ => None,
                };
                let next = by_ref
                    .or_else(|| match value? {
                        v @ Value::Object(inner) if field(inner, "$ref").is_some() => Some(v.clone()),
                        v => alias(v).and_then(|to| value_pointer(tree, &to)),
                    })
                    .filter(|_| !whole);
                if let Some(next) = next {
                    let out = pointers(tree, &next, at, chain, false)?;
                    chain.pop();
                    let primitive = matches!(out, Value::Number(_) | Value::Bool(_) | Value::Null);
                    return Ok(if primitive { out } else { Value::String(format!("{{{}}}", dotted(&token))) });
                }
                if whole || !primitive {
                    chain.pop();
                    return Ok(Value::String(format!("{{{}}}", dotted(&token))));
                }
                // A primitive token's value, in a field: the value, whether the pointer names the
                // token (`#/n/base`) or its `$value`.
                chain.pop();
                return Ok(value.cloned().expect("a primitive"));
            }
            let out = pointers(tree, found, at, chain, whole);
            chain.pop();
            out
        }
        Value::Object(entries) => Ok(Value::Object(
            entries
                .iter()
                .map(|(k, v)| Ok((k.clone(), pointers(tree, v, &format!("{at}.{k}"), chain, false)?)))
                .collect::<Result<_, String>>()?,
        )),
        Value::Array(items) => Ok(Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(n, v)| pointers(tree, v, &format!("{at}/{n}"), chain, false))
                .collect::<Result<_, String>>()?,
        )),
        other => Ok(other.clone()),
    }
}

/// A name in the file as CSS and Rust write it: `Brand Primary` and `brandPrimary` are
/// `brand-primary`, `FONT-SIZE` is `font-size`. Letters and digits are ASCII.
pub fn slug(name: &str, what: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            let boundary = c.is_ascii_uppercase() && previous.is_some_and(|p| p.is_ascii_lowercase());
            if boundary && !out.ends_with('-') {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else if c.is_alphanumeric() {
            return Err(format!("{what} {name:?}: a name's letters are ASCII, since it becomes a CSS and a Rust name"));
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        previous = Some(c);
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        return Err(format!("{what} {name:?} has no letter or digit to name it in CSS and Rust"));
    }
    Ok(out)
}

/// The tokens, typed and in CSS.
struct Book<'a> {
    raws: &'a [Raw<'a>],
    /// Each token's path as CSS and Rust name it.
    names: Vec<Vec<String>>,
    /// Each token's index by where a reference finds it.
    index: HashMap<&'a [String], usize>,
}

impl Book<'_> {
    fn find(&self, to: &[String], from: &[String]) -> Result<usize, String> {
        self.index
            .get(to)
            .copied()
            .ok_or_else(|| format!("{}: {{{}}} is not a token (a group's own value is its $root)", dotted(from), dotted(to)))
    }

    /// Each token's type: its own, or, for an alias, the type at the end of its chain, which each
    /// alias on the way that declares one must agree with. Each chain is walked once, in a loop,
    /// so a long one is neither slow nor deep in the compiler's stack.
    fn types(&self) -> Result<Vec<&str>, String> {
        let mut types: Vec<Option<&str>> = vec![None; self.raws.len()];
        for start in 0..self.raws.len() {
            if types[start].is_some() {
                continue;
            }
            let mut chain = vec![start];
            let mut on_chain = std::collections::HashSet::from([start]);
            let mut ty = loop {
                let i = *chain.last().expect("a link");
                if let Some(ty) = types[i] {
                    chain.pop();
                    break ty;
                }
                let raw = &self.raws[i];
                match &raw.body {
                    Body::Value(_) => {
                        break raw.declared.or(raw.inherited).ok_or_else(|| {
                            format!("{} has no $type, and no group around it has one; tools do not guess a type", dotted(&raw.at))
                        })?;
                    }
                    Body::Alias(to) => {
                        let target = self.find(to, &raw.at)?;
                        if !on_chain.insert(target) {
                            return Err(format!("{}: its references go round in a circle", dotted(&self.raws[start].at)));
                        }
                        chain.push(target);
                    }
                }
            };
            // From the end back: the alias nearest the end that declares another type is wrong.
            for &i in chain.iter().rev() {
                let raw = &self.raws[i];
                if let (Some(declared), Body::Alias(to)) = (raw.declared, &raw.body) {
                    if declared != ty {
                        return Err(format!("{}: $type is {declared}, and {{{}}} is {ty}", dotted(&raw.at), dotted(to)));
                    }
                }
                if let Body::Value(_) = raw.body {
                    ty = raw.declared.or(raw.inherited).expect("checked above");
                }
                types[i] = Some(ty);
            }
        }
        Ok(types.into_iter().map(|t| t.expect("every token typed")).collect())
    }

    fn var(&self, i: usize) -> String {
        format!("var(--{})", self.names[i].join("-"))
    }

    fn tokens(&self) -> Result<Vec<Token>, String> {
        let mut out = Vec::new();
        let mut seen: HashMap<String, usize> = HashMap::new();
        for (i, name) in self.names.iter().enumerate() {
            if let Some(&j) = seen.get(&name.join("-")) {
                return Err(format!(
                    "{} and {} are both --{} in CSS; rename one",
                    dotted(&self.raws[j].at),
                    dotted(&self.raws[i].at),
                    name.join("-")
                ));
            }
            seen.insert(name.join("-"), i);
        }
        let types = self.types()?;
        let mut edges: Vec<Vec<usize>> = Vec::new();
        for (i, raw) in self.raws.iter().enumerate() {
            let ty = types[i].to_string();
            let mut deps = Vec::new();
            let at = dotted(&raw.at);
            let css = match &raw.body {
                Body::Alias(to) => {
                    let target = self.find(to, &raw.at)?;
                    deps.push(target);
                    self.var(target)
                }
                Body::Value(value) => {
                    let mut refs = |to: &[String], want: &str, at: &str| -> Result<String, String> {
                        let target = self.find(to, &raw.at)?;
                        let ty = types[target];
                        if ty != want {
                            return Err(format!("{at}: {{{}}} is {ty}, and this is {want}", dotted(to)));
                        }
                        deps.push(target);
                        Ok(self.var(target))
                    };
                    css(&ty, value, &format!("{at}.$value"), &mut refs)?
                }
            };
            edges.push(deps);
            out.push(Token {
                path: self.names[i].clone(),
                group: raw.at[0].clone(),
                ty,
                css,
                description: raw.description.map(String::from),
            });
        }
        // A reference inside a value (a shadow that lists itself) is a circle CSS would drop. A
        // depth-first search with a stack of its own, not a call per token.
        let mut state = vec![0u8; edges.len()];
        for start in 0..edges.len() {
            if state[start] != 0 {
                continue;
            }
            let mut stack = vec![(start, 0usize)];
            state[start] = 1;
            while let Some((i, next)) = stack.pop() {
                let Some(&j) = edges[i].get(next) else {
                    state[i] = 2;
                    continue;
                };
                stack.push((i, next + 1));
                match state[j] {
                    1 => return Err(format!("{}: its references go round in a circle", dotted(&self.raws[j].at))),
                    0 => {
                        state[j] = 1;
                        stack.push((j, 0));
                    }
                    _ => {}
                }
            }
        }
        Ok(out)
    }
}

// Values as CSS.

type Refs<'r> = dyn FnMut(&[String], &str, &str) -> Result<String, String> + 'r;

/// A number as written, and its value.
fn number<'v>(value: &'v Value, at: &str) -> Result<(&'v str, f64), String> {
    match value {
        Value::Number(text) => match text.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok((text, n)),
            // CSS numbers are finite: a browser clamps a larger one to its own limit.
            _ => Err(format!("{at}: {text} is too large for CSS, which writes finite numbers")),
        },
        other => Err(format!("{at} is a number, not {}", other.kind())),
    }
}

/// An object's fields, checked against the ones its type has.
fn fields<'v>(value: &'v Value, at: &str, required: &[&str], optional: &[&str]) -> Result<&'v [(String, Value)], String> {
    let entries = object(value, at)?;
    for (key, _) in entries {
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return Err(format!("{at}: unknown field {key:?}; the fields are {}", [required, optional].concat().join(", ")));
        }
    }
    for key in required {
        if field(entries, key).is_none() {
            return Err(format!("{at} has no {key}"));
        }
    }
    Ok(entries)
}

fn css(ty: &str, value: &Value, at: &str, refs: &mut Refs) -> Result<String, String> {
    match ty {
        "color" => color(value, at),
        "dimension" => measure(value, at, &["px", "rem"]),
        "duration" => measure(value, at, &["ms", "s"]),
        "number" => Ok(number(value, at)?.0.to_string()),
        "fontWeight" => font_weight(value, at),
        "fontFamily" => font_family(value, at, refs),
        "cubicBezier" => cubic_bezier(value, at),
        "strokeStyle" => stroke_style(value, at, refs),
        "border" => {
            let entries = fields(value, at, &["color", "width", "style"], &[])?;
            let width = part(entries, "width", "dimension", at, refs)?;
            let style = part(entries, "style", "strokeStyle", at, refs)?;
            Ok(format!("{width} {style} {}", part(entries, "color", "color", at, refs)?))
        }
        "transition" => {
            let entries = fields(value, at, &["duration", "delay", "timingFunction"], &[])?;
            let duration = part(entries, "duration", "duration", at, refs)?;
            let timing = part(entries, "timingFunction", "cubicBezier", at, refs)?;
            Ok(format!("{duration} {timing} {}", part(entries, "delay", "duration", at, refs)?))
        }
        "shadow" => match value {
            Value::Array(items) if !items.is_empty() => {
                let mut out = Vec::new();
                for (n, item) in items.iter().enumerate() {
                    let at = format!("{at}/{n}");
                    out.push(match alias(item) {
                        Some(to) => refs(&to, "shadow", &at)?,
                        None => shadow(item, &at, refs)?,
                    });
                }
                Ok(out.join(","))
            }
            Value::Array(_) => Err(format!("{at} is an empty list of shadows")),
            _ => shadow(value, at, refs),
        },
        "gradient" => Err(format!(
            "{at}: a gradient token has no direction or shape, which CSS needs, so it has no CSS value; write the gradient in a stylesheet, from color tokens"
        )),
        "typography" => Err(format!(
            "{at}: a typography token is several CSS properties, so it has no one value; give its parts tokens of their own (fontFamily, dimension, fontWeight, number)"
        )),
        _ => unreachable!("a checked type"),
    }
}

/// A field of a composite value: its own value, or a reference to a token of its type. A zero
/// length is `0`, which every property a composite sets takes.
fn part(entries: &[(String, Value)], key: &str, ty: &str, at: &str, refs: &mut Refs) -> Result<String, String> {
    let value = field(entries, key).expect("a required field");
    let at = format!("{at}.{key}");
    if let Some(to) = alias(value) {
        return refs(&to, ty, &at);
    }
    let css = css(ty, value, &at, refs)?;
    Ok(if ty == "dimension" && number(field(object(value, &at)?, "value").expect("checked"), &at)?.1 == 0.0 {
        "0".into()
    } else {
        css
    })
}

fn shadow(value: &Value, at: &str, refs: &mut Refs) -> Result<String, String> {
    let entries = fields(value, at, &["color", "offsetX", "offsetY", "blur", "spread"], &["inset"])?;
    let inset = match field(entries, "inset") {
        None | Some(Value::Bool(false)) => "",
        Some(Value::Bool(true)) => "inset ",
        Some(other) => return Err(format!("{at}.inset is true or false, not {}", other.kind())),
    };
    let x = part(entries, "offsetX", "dimension", at, refs)?;
    let y = part(entries, "offsetY", "dimension", at, refs)?;
    let blur = part(entries, "blur", "dimension", at, refs)?;
    let spread = part(entries, "spread", "dimension", at, refs)?;
    let color = part(entries, "color", "color", at, refs)?;
    // A spread of 0 is CSS's own default.
    let spread = if spread == "0" { String::new() } else { format!(" {spread}") };
    Ok(format!("{inset}{x} {y} {blur}{spread} {color}"))
}

/// A dimension or a duration: `{"value": 4, "unit": "px"}` is `4px`.
fn measure(value: &Value, at: &str, units: &[&str]) -> Result<String, String> {
    let entries = fields(value, at, &["value", "unit"], &[])?;
    let (n, _) = number(field(entries, "value").expect("checked"), &format!("{at}.value"))?;
    let unit = string(field(entries, "unit").expect("checked"), &format!("{at}.unit"))?;
    if !units.contains(&unit) {
        return Err(format!("{at}.unit is {unit:?}; it is {}", units.join(" or ")));
    }
    Ok(format!("{n}{unit}"))
}

fn font_weight(value: &Value, at: &str) -> Result<String, String> {
    const NAMES: &[(u16, &[&str])] = &[
        (100, &["thin", "hairline"]),
        (200, &["extra-light", "ultra-light"]),
        (300, &["light"]),
        (400, &["normal", "regular", "book"]),
        (500, &["medium"]),
        (600, &["semi-bold", "demi-bold"]),
        (700, &["bold"]),
        (800, &["extra-bold", "ultra-bold"]),
        (900, &["black", "heavy"]),
        (950, &["extra-black", "ultra-black"]),
    ];
    match value {
        Value::String(name) => NAMES
            .iter()
            .find(|(_, names)| names.contains(&name.as_str()))
            .map(|(weight, _)| weight.to_string())
            .ok_or_else(|| format!("{at}: {name:?} is not a font weight's name (thin, normal, bold, ...)")),
        _ => match number(value, at)? {
            (text, n) if (1.0..=1000.0).contains(&n) => Ok(text.to_string()),
            (text, _) => Err(format!("{at}: {text} is not a font weight, which is from 1 to 1000")),
        },
    }
}

fn font_family(value: &Value, at: &str, refs: &mut Refs) -> Result<String, String> {
    const GENERIC: &[&str] = &[
        "serif",
        "sans-serif",
        "monospace",
        "cursive",
        "fantasy",
        "system-ui",
        "ui-serif",
        "ui-sans-serif",
        "ui-monospace",
        "ui-rounded",
        "math",
        "emoji",
        "fangsong",
    ];
    // A family name is a CSS string: `"` and `\` escaped, and `<` as `\3c ` so the
    // stylesheet cannot close a <style> element it is written into.
    let name = |value: &Value, at: &str| -> Result<String, String> {
        let name = string(value, at)?;
        if name.is_empty() || name.contains(char::is_control) {
            return Err(format!("{at}: {name:?} is not a font name (empty, or with a control character)"));
        }
        if GENERIC.contains(&name) {
            return Ok(name.to_string());
        }
        let mut quoted = String::from("\"");
        for c in name.chars() {
            match c {
                '"' | '\\' => {
                    quoted.push('\\');
                    quoted.push(c);
                }
                '<' => quoted.push_str("\\3c "),
                c => quoted.push(c),
            }
        }
        quoted.push('"');
        Ok(quoted)
    };
    match value {
        Value::Array(names) if !names.is_empty() => {
            // An item may be a reference to a fontFamily token: its var(), a list in a list.
            let names: Result<Vec<String>, String> = names
                .iter()
                .enumerate()
                .map(|(n, v)| match alias(v) {
                    Some(to) => refs(&to, "fontFamily", &format!("{at}/{n}")),
                    None => name(v, &format!("{at}/{n}")),
                })
                .collect();
            Ok(names?.join(","))
        }
        Value::Array(_) => Err(format!("{at} is an empty list of fonts")),
        _ => name(value, at),
    }
}

fn cubic_bezier(value: &Value, at: &str) -> Result<String, String> {
    let Value::Array(points) = value else { return Err(format!("{at} is an array of four numbers, not {}", value.kind())) };
    if points.len() != 4 {
        return Err(format!("{at} has {} numbers, not four", points.len()));
    }
    let mut out = Vec::new();
    for (n, point) in points.iter().enumerate() {
        let (text, value) = number(point, &format!("{at}/{n}"))?;
        if n % 2 == 0 && !(0.0..=1.0).contains(&value) {
            return Err(format!("{at}/{n}: an x coordinate is from 0 to 1, not {text}"));
        }
        out.push(text);
    }
    Ok(format!("cubic-bezier({})", out.join(",")))
}

/// A named style, or dashes and caps as `dashed`: CSS sets no dash pattern, and the Format
/// module gives `dashed` as the closest (9.3.3).
fn stroke_style(value: &Value, at: &str, refs: &mut Refs) -> Result<String, String> {
    const STYLES: &[&str] = &["solid", "dashed", "dotted", "double", "groove", "ridge", "outset", "inset"];
    match value {
        Value::String(style) if STYLES.contains(&style.as_str()) => Ok(style.clone()),
        Value::String(style) => Err(format!("{at}: {style:?} is not a stroke style ({})", STYLES.join(", "))),
        Value::Object(_) => {
            let entries = fields(value, at, &["dashArray", "lineCap"], &[])?;
            let cap = string(field(entries, "lineCap").expect("checked"), &format!("{at}.lineCap"))?;
            if !matches!(cap, "round" | "butt" | "square") {
                return Err(format!("{at}.lineCap: {cap:?} is not round, butt or square"));
            }
            match field(entries, "dashArray").expect("checked") {
                Value::Array(dashes) if !dashes.is_empty() => {
                    for (n, dash) in dashes.iter().enumerate() {
                        let at = format!("{at}.dashArray/{n}");
                        match alias(dash) {
                            Some(to) => refs(&to, "dimension", &at)?,
                            None => dimension(dash, &at)?,
                        };
                    }
                }
                other => return Err(format!("{at}.dashArray is a list of dimensions, not {}", other.kind())),
            }
            Ok("dashed".into())
        }
        other => Err(format!("{at} is a string, not {}", other.kind())),
    }
}

/// A color in CSS Color 4 syntax. An sRGB color whose components are whole 8-bit values, or
/// whose `hex` is them rounded to 8 bits, is the hex (`#1d1b26`, `rgb(29 27 38 / 0.05)` with an alpha).
fn color(value: &Value, at: &str) -> Result<String, String> {
    // A hex alone is not a color in the format (2025.10): the error is the object to write instead.
    let written_hex = match value {
        Value::String(text) => Some(text.as_str()),
        Value::Object(entries) if field(entries, "components").is_none() => match field(entries, "hex") {
            Some(Value::String(text)) => Some(text.as_str()),
            _ => None,
        },
        _ => None,
    };
    if let Some(text) = written_hex {
        return Err(match hex_object(text) {
            Some(object) => format!("{at}: a color is its components, with the hex as a fallback; write {object}"),
            None => format!(
                "{at}: {text:?} is not a color; a color is an object, {{\"colorSpace\": \"srgb\", \"components\": [1, 0, 0]}}"
            ),
        });
    }
    let entries = fields(value, at, &["colorSpace", "components"], &["alpha", "hex"])?;
    let space = string(field(entries, "colorSpace").expect("checked"), &format!("{at}.colorSpace"))?;
    const ANY: (f64, f64, bool) = (f64::NEG_INFINITY, f64::INFINITY, false);
    const UNIT: (f64, f64, bool) = (0.0, 1.0, false);
    const HUE: (f64, f64, bool) = (0.0, 360.0, true);
    const PERCENT: (f64, f64, bool) = (0.0, 100.0, false);
    const CHROMA: (f64, f64, bool) = (0.0, f64::INFINITY, false);
    let (ranges, function): ([(f64, f64, bool); 3], &str) = match space {
        "srgb" | "srgb-linear" | "display-p3" | "a98-rgb" | "prophoto-rgb" | "rec2020" | "xyz-d65" | "xyz-d50" => {
            ([UNIT; 3], "color")
        }
        "hsl" => ([HUE, PERCENT, PERCENT], "hsl"),
        "hwb" => ([HUE, PERCENT, PERCENT], "hwb"),
        "lab" => ([PERCENT, ANY, ANY], "lab"),
        "lch" => ([PERCENT, CHROMA, HUE], "lch"),
        "oklab" => ([UNIT, ANY, ANY], "oklab"),
        "oklch" => ([UNIT, CHROMA, HUE], "oklch"),
        other => {
            return Err(format!(
                "{at}.colorSpace: {other:?} is not a color space (srgb, srgb-linear, hsl, hwb, lab, lch, oklab, oklch, display-p3, a98-rgb, prophoto-rgb, rec2020, xyz-d65, xyz-d50)"
            ))
        }
    };
    let components = field(entries, "components").expect("checked");
    let Value::Array(components) = components else {
        return Err(format!("{at}.components is an array, not {}", components.kind()));
    };
    if components.len() != 3 {
        return Err(format!("{at}.components has {} values; {space} has three", components.len()));
    }
    // (as written, value); `none` has no value.
    let mut parts: Vec<(String, Option<f64>)> = Vec::new();
    for (n, (component, (min, max, open))) in components.iter().zip(ranges).enumerate() {
        let at = format!("{at}.components/{n}");
        if *component == Value::String("none".into()) {
            parts.push(("none".into(), None));
            continue;
        }
        let (text, value) = number(component, &at)?;
        if value < min || value > max || (open && value == max) {
            let close = if open { ")" } else { "]" };
            return Err(format!("{at}: {text} is outside {space}'s range, [{min}, {max}{close}"));
        }
        // The Color module writes these as numbers (0 to 100) in the file; in CSS they are
        // percentages, which every browser with space-separated hsl()/hwb() reads, the same color.
        let percent = matches!(function, "hsl" | "hwb") && n > 0;
        parts.push((if percent { format!("{text}%") } else { text.to_string() }, Some(value)));
    }
    let alpha = match field(entries, "alpha") {
        None => None,
        Some(alpha) => match number(alpha, &format!("{at}.alpha"))? {
            (_, 1.0) => None,
            (text, a) if (0.0..1.0).contains(&a) => Some(text),
            (text, _) => return Err(format!("{at}.alpha: {text} is not from 0 to 1")),
        },
    };
    let hex = match field(entries, "hex") {
        None => None,
        Some(hex) => {
            let text = string(hex, &format!("{at}.hex"))?;
            let digits = text.strip_prefix('#').filter(|d| d.len() == 6 && d.chars().all(|c| c.is_ascii_hexdigit()));
            let Some(digits) = digits else { return Err(format!("{at}.hex: {text:?} is not a 6-digit hex color, #rrggbb")) };
            Some((text, [0, 2, 4].map(|i| u8::from_str_radix(&digits[i..i + 2], 16).expect("hex digits"))))
        }
    };
    if space == "srgb" {
        if let [Some(r), Some(g), Some(b)] = [parts[0].1, parts[1].1, parts[2].1] {
            let bytes = [r, g, b].map(|c| (c * 255.0).round() as u8);
            let whole = [r, g, b].iter().all(|c| (c * 255.0 - (c * 255.0).round()).abs() < 1e-6);
            // In sRGB `hex` is the components at 8 bits. One step off in a channel is a rounding
            // the author's tool chose (and the components are the color); more is two colors, and
            // which one the page shows would be a guess.
            if let Some((text, written)) = hex {
                if written.iter().zip(bytes).any(|(w, b)| w.abs_diff(b) > 1) {
                    let components: Vec<&str> = parts.iter().map(|(text, _)| text.as_str()).collect();
                    return Err(format!(
                        "{at}.hex: {text} is not the components [{}], which are {} at 8 bits; correct one so they agree",
                        components.join(", "),
                        hex_text(bytes)
                    ));
                }
            }
            // When the hex is their color at 8 bits it is the shorter text, and when not, the
            // components are the color.
            let agrees = hex.is_some_and(|(_, written)| written == bytes);
            if whole || agrees {
                return Ok(match alpha {
                    None => hex_text(bytes),
                    Some(alpha) => format!("rgb({} {} {} / {alpha})", bytes[0], bytes[1], bytes[2]),
                });
            }
        }
    }
    let components: Vec<&str> = parts.iter().map(|(text, _)| text.as_str()).collect();
    let components = components.join(" ");
    let alpha = alpha.map(|a| format!(" / {a}")).unwrap_or_default();
    Ok(if function == "color" {
        format!("color({space} {components}{alpha})")
    } else {
        format!("{function}({components}{alpha})")
    })
}

/// The color object a hex stands for (`#1d1b26`, `#f00`, with an alpha `#1d1b2680`): sRGB
/// components to four places, which round back to the same 8-bit values, and the hex as written.
fn hex_object(text: &str) -> Option<String> {
    let digits = text.strip_prefix('#').filter(|d| d.chars().all(|c| c.is_ascii_hexdigit()))?;
    let long: String = match digits.len() {
        3 | 4 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => digits.to_string(),
        _ => return None,
    };
    let bytes: Vec<u8> =
        (0..long.len()).step_by(2).map(|i| u8::from_str_radix(&long[i..i + 2], 16).expect("hex digits")).collect();
    let unit = |b: u8| {
        let text = format!("{:.4}", f64::from(b) / 255.0);
        let text = text.trim_end_matches('0').trim_end_matches('.');
        if text.is_empty() {
            "0".to_string()
        } else {
            text.to_string()
        }
    };
    let components: Vec<String> = bytes[..3].iter().map(|b| unit(*b)).collect();
    let alpha = bytes.get(3).map(|a| format!(", \"alpha\": {}", unit(*a))).unwrap_or_default();
    Some(format!(
        r#"{{"colorSpace": "srgb", "components": [{}]{alpha}, "hex": "{}"}}"#,
        components.join(", "),
        hex_text([bytes[0], bytes[1], bytes[2]])
    ))
}

/// `#1d1b26`. Never the 3-digit form: measured on the watchlist's tokens, `#fff` for `#ffffff`
/// saves 6 bytes and costs 31 after brotli.
fn hex_text(bytes: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2])
}

/// A dimension (a breakpoint's width), for what Corten reads under its extension.
pub fn dimension(value: &Value, at: &str) -> Result<String, String> {
    measure(value, at, &["px", "rem"])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str) -> Result<Vec<(String, String)>, String> {
        let tokens = token_file(&json::parse(text).unwrap())?.tokens;
        Ok(tokens.iter().map(|t| (t.property(), t.css.clone())).collect())
    }

    fn one(ty: &str, value: &str) -> Result<String, String> {
        let text = format!(r#"{{"g": {{"t": {{"$type": "{ty}", "$value": {value}}}}}}}"#);
        tokens(&text).map(|mut t| t.remove(0).1)
    }

    #[test]
    fn colors_are_css_color_4() {
        let c = |space: &str, components: &str, rest: &str| {
            one("color", &format!(r#"{{"colorSpace": "{space}", "components": {components}{rest}}}"#))
        };
        assert_eq!(c("srgb", "[1, 0, 1]", "").unwrap(), "#ff00ff");
        assert_eq!(c("srgb", "[0.1137, 0.1059, 0.149]", r##", "hex": "#1d1b26""##).unwrap(), "#1d1b26");
        assert_eq!(
            c("srgb", "[0.1137, 0.1059, 0.149]", r##", "hex": "#1D1B26", "alpha": 0.05"##).unwrap(),
            "rgb(29 27 38 / 0.05)"
        );
        assert_eq!(c("srgb", "[0.1137, 0.1059, 0.149]", "").unwrap(), "color(srgb 0.1137 0.1059 0.149)");
        assert_eq!(c("srgb", r#"["none", 0, 0]"#, "").unwrap(), "color(srgb none 0 0)");
        assert_eq!(c("srgb", "[0, 0, 0]", r#", "alpha": 1"#).unwrap(), "#000000");
        assert_eq!(c("srgb", "[0, 0, 0]", r#", "alpha": 0"#).unwrap(), "rgb(0 0 0 / 0)", "alpha is [0, 1]");
        assert_eq!(c("hsl", r#"["none", 0, 100]"#, r#", "alpha": 0.5"#).unwrap(), "hsl(none 0% 100% / 0.5)");
        assert_eq!(c("oklch", "[0.7, 0.15, 250]", "").unwrap(), "oklch(0.7 0.15 250)");
        assert_eq!(c("display-p3", "[1, 0.2, 0]", "").unwrap(), "color(display-p3 1 0.2 0)");
        let error = |r: Result<String, String>| r.expect_err("an error");
        assert_eq!(
            c("srgb", "[0.5, 0, 0]", r##", "hex": "#7f0000""##).unwrap(),
            "color(srgb 0.5 0 0)",
            "a fallback one step from the components rounded leaves the components as the color"
        );
        assert!(error(c("srgb", "[0.1137, 0.1059, 0.149]", r##", "hex": "#ff0000""##)).ends_with(
            ".hex: #ff0000 is not the components [0.1137, 0.1059, 0.149], which are #1d1b26 at 8 bits; correct one so they agree"
        ));
        assert!(error(c("srgb", "[0.5, 0, 0]", r##", "hex": "#7d0000""##)).contains("which are #800000"), "two steps off");
        assert_eq!(
            c("display-p3", "[1, 0.2, 0]", r##", "hex": "#ff0000""##).unwrap(),
            "color(display-p3 1 0.2 0)",
            "outside sRGB the hex is a gamut-mapped fallback, not checked"
        );
        assert_eq!(c("srgb", r#"["none", 0, 0]"#, r##", "hex": "#ff0000""##).unwrap(), "color(srgb none 0 0)");
        // A hex alone: the error is the object to paste, which compiles to the same color.
        let pasted = error(one("color", r##""#1d1b26""##));
        assert!(
            pasted.ends_with(
                r##": a color is its components, with the hex as a fallback; write {"colorSpace": "srgb", "components": [0.1137, 0.1059, 0.149], "hex": "#1d1b26"}"##
            ),
            "{pasted}"
        );
        let object = pasted.split("write ").nth(1).unwrap();
        assert_eq!(one("color", object).unwrap(), "#1d1b26");
        assert!(error(one("color", r##"{"hex": "#F0A"}"##)).contains(r##"[1, 0, 0.6667], "hex": "#ff00aa"}"##));
        assert!(error(one("color", r##""#00000080""##)).contains(r##"[0, 0, 0], "alpha": 0.502, "hex": "#000000"}"##));
        assert_eq!(
            one("color", r##"{"colorSpace": "srgb", "components": [0, 0, 0], "alpha": 0.502, "hex": "#000000"}"##).unwrap(),
            "rgb(0 0 0 / 0.502)"
        );
        assert!(error(one("color", r#""red""#)).contains("\"red\" is not a color"));
        assert!(error(c("hsl", "[360, 0, 0]", "")).contains("outside hsl's range, [0, 360)"));
        assert!(error(c("srgb", "[1.2, 0, 0]", "")).contains("outside srgb's range"));
        assert!(error(c("rgb", "[1, 0, 0]", "")).contains("not a color space"));
        assert!(error(c("srgb", "[1, 0]", "")).contains("has 2 values"));
        assert!(error(c("srgb", "[1, 0, 0]", r##", "hex": "#f00""##)).contains("6-digit"));
        assert!(error(c("srgb", "[1, 0, 0]", r#", "alpha": 2"#)).contains("not from 0 to 1"));
    }

    #[test]
    fn the_other_types() {
        assert_eq!(one("dimension", r#"{"value": 0.5, "unit": "rem"}"#).unwrap(), "0.5rem");
        assert_eq!(
            one("dimension", r#"{"value": 0, "unit": "px"}"#).unwrap(),
            "0px",
            "a length alone keeps its unit, for calc()"
        );
        assert_eq!(one("duration", r#"{"value": 200, "unit": "ms"}"#).unwrap(), "200ms");
        assert_eq!(one("number", "1.5").unwrap(), "1.5");
        assert_eq!(one("fontWeight", r#""semi-bold""#).unwrap(), "600");
        assert_eq!(one("fontWeight", "350").unwrap(), "350");
        assert_eq!(
            one("fontFamily", r#"["Inter Variable", "system-ui", "sans-serif"]"#).unwrap(),
            "\"Inter Variable\",system-ui,sans-serif"
        );
        assert_eq!(one("cubicBezier", "[0.5, 0, 1, 1]").unwrap(), "cubic-bezier(0.5,0,1,1)");
        assert_eq!(one("strokeStyle", r#""dashed""#).unwrap(), "dashed");
        let px = |n: u32| format!(r#"{{"value": {n}, "unit": "px"}}"#);
        let black = r#"{"colorSpace": "srgb", "components": [0, 0, 0], "alpha": 0.25}"#;
        assert_eq!(
            one("border", &format!(r#"{{"color": {black}, "width": {}, "style": "solid"}}"#, px(1))).unwrap(),
            "1px solid rgb(0 0 0 / 0.25)"
        );
        let shadow = |spread: u32, inset: &str| {
            format!(
                r#"{{"color": {black}, "offsetX": {}, "offsetY": {}, "blur": {}, "spread": {}{inset}}}"#,
                px(0),
                px(1),
                px(2),
                px(spread)
            )
        };
        assert_eq!(one("shadow", &shadow(0, "")).unwrap(), "0 1px 2px rgb(0 0 0 / 0.25)");
        assert_eq!(
            one("shadow", &format!("[{}, {}]", shadow(0, ""), shadow(3, r#", "inset": true"#))).unwrap(),
            "0 1px 2px rgb(0 0 0 / 0.25),inset 0 1px 2px 3px rgb(0 0 0 / 0.25)"
        );
        assert_eq!(
            one("transition", r#"{"duration": {"value": 0.2, "unit": "s"}, "delay": {"value": 0, "unit": "ms"}, "timingFunction": [0, 0, 0.5, 1]}"#).unwrap(),
            "0.2s cubic-bezier(0,0,0.5,1) 0ms",
            "a time of 0 keeps its unit, which CSS requires"
        );
        let error = |r: Result<String, String>| r.expect_err("an error");
        assert!(error(one("dimension", r#"{"value": 1, "unit": "em"}"#)).contains("unit is \"em\"; it is px or rem"));
        assert!(error(one("dimension", r#"{"value": 1}"#)).contains("has no unit"));
        assert!(error(one("dimension", r#"{"value": 1, "unit": "px", "x": 1}"#)).contains("unknown field \"x\""));
        assert!(error(one("fontWeight", "1001")).contains("from 1 to 1000"));
        assert!(error(one("fontWeight", r#""Bold""#)).contains("not a font weight's name"));
        assert!(error(one("cubicBezier", "[2, 0, 1, 1]")).contains("x coordinate"));
        assert_eq!(one("fontFamily", r#""Font \"A\"; {b} <x>\\""#).unwrap(), r#""Font \"A\"; {b} \3c x>\\""#);
        assert!(error(one("fontFamily", r#""a\nb""#)).contains("not a font name"));
        assert!(error(one("gradient", "[]")).contains("no direction"));
        assert!(error(one("typography", "{}")).contains("several CSS properties"));
        let dashes = r#"{"dashArray": [{"value": 0.5, "unit": "rem"}, {"value": 0.25, "unit": "rem"}], "lineCap": "round"}"#;
        assert_eq!(one("strokeStyle", dashes).unwrap(), "dashed", "the Format module's fallback for CSS");
        assert!(
            error(one("strokeStyle", r#"{"dashArray": [], "lineCap": "round"}"#)).contains("dashArray is a list of dimensions")
        );
        assert!(error(one("strokeStyle", r#"{"dashArray": [{"value": 1, "unit": "px"}], "lineCap": "flat"}"#))
            .contains("not round, butt or square"));
        assert!(error(one("colour", "1")).contains("not a type"));
    }

    #[test]
    fn groups_types_and_references() {
        let found = tokens(
            r##"{
                "$schema": "https://www.designtokens.org/schemas/2025.10/format.json",
                "space": { "$type": "dimension", "1": { "$value": { "value": 4, "unit": "px" } } },
                "color": {
                    "$type": "color",
                    "accent": {
                        "$root": { "$value": { "colorSpace": "srgb", "components": [0, 0, 1] } },
                        "soft": { "$value": { "colorSpace": "srgb", "components": [0, 0, 1], "alpha": 0.5 } }
                    },
                    "link": { "$value": "{color.accent.$root}" },
                    "visited": { "$ref": "#/color/link/$value" }
                },
                "shadow": {
                    "ring": { "$type": "shadow", "$value": {
                        "color": "{color.link}", "offsetX": "{space.1}", "offsetY": { "value": 0, "unit": "px" },
                        "blur": { "value": 0, "unit": "px" }, "spread": "{space.1}"
                    } }
                }
            }"##,
        )
        .unwrap();
        let pairs: Vec<(&str, &str)> = found.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            pairs,
            [
                ("--space-1", "4px"),
                ("--color-accent", "#0000ff"),
                ("--color-accent-soft", "rgb(0 0 255 / 0.5)"),
                ("--color-link", "var(--color-accent)"),
                ("--color-visited", "var(--color-link)"),
                ("--shadow-ring", "var(--space-1) 0 0 var(--space-1) var(--color-link)"),
            ]
        );
        let error = |text: &str| tokens(text).expect_err("an error");
        assert!(error(r#"{"g": {"t": {"$value": 1}}}"#).contains("g.t has no $type"));
        assert_eq!(
            tokens(r#"{"t": {"$type": "number", "$value": 1}}"#).unwrap(),
            [("--t".to_string(), "1".to_string())],
            "a token at the top of the file"
        );
        assert!(error(r#"{"$root": {"$type": "number", "$value": 1}}"#).contains("$root at the top of the file"));
        assert!(error(r#"{"g": {"$root": {}}}"#).contains("g.$root has no $value"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$value": "{g.b}"}, "b": {"$value": "{g.a}"}}}"#).contains("circle"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$value": "{g.c}"}}}"#).contains("{g.c} is not a token"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$root": {"$value": 1}}, "b": {"$value": "{g.a}"}}}"#)
            .contains("{g.a} is not a token"));
        assert!(error(r#"{"g": {"a": {"$type": "number", "$value": 1}, "b": {"$type": "dimension", "$value": "{g.a}"}}}"#)
            .contains("$type is dimension, and {g.a} is number"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$value": 1, "b": {"$value": 2}}}}"#)
            .contains("a token ($value) and a group (b)"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$value": 1, "$tpye": "x"}}}"#).contains("unknown property \"$tpye\""));
        assert!(error(r##"{"g": {"$type": "number", "a": {"$ref": "#/g/b/$value/0"}, "b": {"$value": 1}}}"##)
            .contains("names nothing"));
        let shadow = r#"{"color": "{c.a}", "offsetX": "{c.a}", "offsetY": "{c.a}", "blur": "{c.a}", "spread": "{c.a}"}"#;
        assert!(error(&format!(
            r#"{{"c": {{"a": {{"$type": "color", "$value": {{"colorSpace": "srgb", "components": [0, 0, 0]}}}}}}, "s": {{"x": {{"$type": "shadow", "$value": {shadow}}}}}}}"#
        ))
        .contains("s.x.$value.offsetX: {c.a} is color, and this is dimension"));
        assert!(error(r#"{"s": {"$type": "shadow", "a": {"$value": ["{s.a}"]}}}"#).contains("circle"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$value": 1}, "$extensions": {"run.corten": {}}}}"#)
            .contains("goes on the root group"));
    }

    #[test]
    fn json_pointers_into_values_are_the_values_they_name() {
        let found = tokens(
            r##"{
                "base": { "$type": "dimension", "space": { "$value": { "value": 16, "unit": "px" } }, "half": { "$type": "number", "$value": 0.5 } },
                "color": { "$type": "color", "ink": { "$value": { "colorSpace": "srgb", "components": [0, 0.4, 0.8] } } },
                "derived": {
                    "rem": { "$type": "dimension", "$value": { "value": { "$ref": "#/base/space/$value/value" }, "unit": "rem" } },
                    "hue": { "$type": "number", "$ref": "#/color/ink/$value/components/2" },
                    "same": { "$type": "color", "$ref": "#/color/ink/$value" },
                    "whole": { "$ref": "#/color/ink" },
                    "red": { "$type": "color", "$value": { "colorSpace": "srgb", "components": [{ "$ref": "#/base/half/$value" }, { "$ref": "#/base/half" }, 0] } }
                }
            }"##,
        )
        .unwrap();
        let derived: Vec<String> = found[3..].iter().map(|(k, v)| format!("{k}:{v}")).collect();
        assert_eq!(
            derived,
            [
                "--derived-rem:16rem",
                "--derived-hue:0.8",
                "--derived-same:var(--color-ink)",
                "--derived-whole:var(--color-ink)",
                "--derived-red:color(srgb 0.5 0.5 0)"
            ],
            "a number token in a color's component is the number"
        );
        let error = |text: &str| tokens(text).expect_err("an error");
        assert!(error(r##"{"g": {"$type": "number", "a": {"$value": {"$ref": "#/g/b/$value/x"}}, "b": {"$value": 1}}}"##)
            .contains("names nothing"));
        let leading = r##"{"c": {"$type": "color", "k": {"$value": {"colorSpace": "srgb", "components": [0, 0.5, 0]}}},
            "n": {"$type": "number", "g": {"$ref": "#/c/k/$value/components/01"}}}"##;
        assert!(error(leading).contains("names nothing"), "an index with a leading zero is no index");
        let circle = r##"{"g": {"$type": "dimension",
            "a": {"$value": {"value": {"$ref": "#/g/b/$value/value"}, "unit": "px"}},
            "b": {"$value": {"value": {"$ref": "#/g/a/$value/value"}, "unit": "px"}}}}"##;
        assert!(error(circle).contains("circle"));
        assert!(error(r#"{"g": {"$type": "number", "a": {"$value": {"$ref": "x.json"}}}}"#).contains("not a JSON pointer"));

        // A pointer to a token that is itself a reference follows it to an explicit value
        // (Format 7.2.2), by `{...}` or by `$ref`, and a circle of them is an error.
        let chained = tokens(
            r##"{
                "n": { "$type": "number", "base": { "$value": 0.25 }, "alias": { "$value": "{n.base}" }, "by-ref": { "$ref": "#/n/alias" } },
                "c": { "$type": "color",
                    "a": { "$value": { "colorSpace": "srgb", "components": [{ "$ref": "#/n/alias/$value" }, 0, 0] } },
                    "b": { "$value": { "colorSpace": "srgb", "components": [{ "$ref": "#/n/by-ref" }, 0, 0] } } }
            }"##,
        )
        .unwrap();
        let colors: Vec<&str> = chained[3..].iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(colors, ["color(srgb 0.25 0 0)", "color(srgb 0.25 0 0)"]);
        let round = r##"{"n": {"$type": "number", "a": {"$value": "{n.b}"}, "b": {"$value": "{n.a}"}},
            "c": {"$type": "color", "x": {"$value": {"colorSpace": "srgb", "components": [{"$ref": "#/n/a/$value"}, 0, 0]}}}}"##;
        assert!(error(round).contains("circle"));
        // `{g.a}` names a token; a group's root is `{g.a.$root}`, followed or not.
        let group = r##"{"g": {"$type": "number", "a": {"$root": {"$value": 1}}, "b": {"$value": "{g.a}"}},
            "c": {"$type": "color", "x": {"$value": {"colorSpace": "srgb", "components": [{"$ref": "#/g/b/$value"}, 0, 0]}}}}"##;
        assert!(error(group).contains("{g.a} is not a token"));

        // A pointer is a URI fragment (RFC 6901, section 6): percent-escapes decoded, ~ escaped.
        let encoded = tokens(
            r##"{"g": {"$type": "number", "a b": {"$value": 2}, "c/d": {"$value": 3},
                "x": {"$ref": "#/g/a%20b"}, "y": {"$ref": "#/g/c~1d/$value"}}}"##,
        )
        .unwrap();
        assert_eq!(encoded[2..], [("--g-x".to_string(), "var(--g-a-b)".to_string()), ("--g-y".into(), "var(--g-c-d)".into())]);
        assert!(error(r##"{"g": {"$type": "number", "a": {"$value": 1}, "b": {"$ref": "#/g/a~2"}}}"##)
            .contains("~ is followed by 0 (~) or 1 (/)"));
        assert!(error(r##"{"g": {"$type": "number", "a": {"$value": 1}, "b": {"$ref": "#/g/%zz"}}}"##)
            .contains("% is followed by two hex digits"));
        assert!(error(r##"{"g": {"$type": "number", "a": {"$value": 1}, "b": {"$ref": "#/g/%FF"}}}"##).contains("not UTF-8"));

        // A font list may name a fontFamily token, by pointer or by `{...}`.
        let fonts = tokens(
            r##"{"f": {"$type": "fontFamily", "base": {"$value": "Inter"}, "stack": {"$value": ["Mono X", "monospace"]},
                "list": {"$value": [{"$ref": "#/f/base/$value"}, {"$ref": "#/f/stack"}, {"$ref": "#/f/stack/$value/1"}]}}}"##,
        )
        .unwrap();
        assert_eq!(fonts[2].1, "var(--f-base),var(--f-stack),monospace");
        assert!(error(
            r##"{"f": {"$type": "fontFamily", "w": {"$type": "fontWeight", "$value": "bold"}, "l": {"$value": [{"$ref": "#/f/w"}]}}}"##
        )
        .contains("is fontWeight, and this is fontFamily"));
        assert!(error(r#"{"f": {"$type": "fontFamily", "a": {"$value": "A"}, "l": {"$value": ["{f.a}"]}}}"#)
            .contains("f.l.$value/0: a font list names a token with {\"$ref\""));
    }

    #[test]
    fn names_become_css_and_rust_names() {
        let found = tokens(
            r#"{ "Brand Colors": { "$type": "number", "Primary": { "$value": 1 }, "darkSurface": { "$value": 2 }, "FONT_2X": { "$value": 3 } } }"#,
        )
        .unwrap();
        let names: Vec<&str> = found.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["--brand-colors-primary", "--brand-colors-dark-surface", "--brand-colors-font-2x"]);
        let error = |text: &str| tokens(text).expect_err("an error");
        assert!(error(r#"{"g": {"$type": "number", "font-size": {"$value": 1}, "FONT-SIZE": {"$value": 2}}}"#)
            .contains("g.font-size and g.FONT-SIZE are both --g-font-size in CSS"));
        assert!(error(r#"{"g": {"$type": "number", "café": {"$value": 1}}}"#).contains("ASCII"));
        assert!(error(r#"{"g": {"$type": "number", "--": {"$value": 1}}}"#).contains("no letter or digit"));
    }

    #[test]
    fn long_chains_end_in_a_value_or_an_error_not_a_stack_overflow() {
        // 3,000 aliases, each naming the one before: a loop, not a call per link.
        let mut text = String::from(r#"{"g": {"$type": "number", "t0": {"$value": 1}"#);
        for n in 1..3000 {
            text.push_str(&format!(r#", "t{n}": {{"$value": "{{g.t{}}}"}}"#, n - 1));
        }
        text.push_str("}}");
        assert_eq!(tokens(&text).unwrap().len(), 3000);
        // Pointers into values, each through the one before: 100 is longer than the old limit of
        // 32 and still a chain, not a circle; past the limit it says so.
        let pointers = |n: usize| {
            let mut text = String::from(r#"{"g": {"$type": "dimension", "d0": {"$value": {"value": 1, "unit": "px"}}"#);
            for k in 1..n {
                text.push_str(&format!(
                    r##", "d{k}": {{"$value": {{"value": {{"$ref": "#/g/d{}/$value/value"}}, "unit": "px"}}}}"##,
                    k - 1
                ));
            }
            text.push_str("}}");
            tokens(&text)
        };
        assert_eq!(pointers(100).unwrap()[99].1, "1px");
        assert!(pointers(400).unwrap_err().contains("a chain of more than 256 JSON pointers"));
        // Groups extending the next one.
        let mut text = String::from(r#"{"e0": {"$type": "number", "x": {"$value": 1}}"#);
        for n in 1..400 {
            text.push_str(&format!(r#", "e{n}": {{"$extends": "{{e{}}}"}}"#, n - 1));
        }
        text.push('}');
        assert!(tokens(&text).unwrap_err().contains("a chain of more than 256 groups"));
    }

    #[test]
    fn groups_extending_one_group_twice_at_each_level_stop_at_the_budget() {
        let mut text = String::from("{");
        for n in 0..20 {
            text.push_str(&format!(r#""g{n}": {{"x": {{"$extends": "{{g{0}}}"}}, "y": {{"$extends": "{{g{0}}}"}}}}, "#, n + 1));
        }
        text.push_str(r#""g20": {"$type": "number", "a": {"$value": 1}}}"#);
        let error = token_file(&json::parse(&text).unwrap()).err().expect("an error");
        assert!(error.contains("more than 100000 references, $extends and sets"), "{error}");
    }

    #[test]
    fn a_large_group_merges_in_linear_time() {
        // 50,000 tokens in one group, merged over themselves: an index, not a scan per key.
        let members: Vec<(String, Value)> = (0..50_000).map(|n| (format!("t{n}"), Value::Null)).collect();
        let mut into = members.clone();
        let started = std::time::Instant::now();
        merge(&mut into, members);
        assert_eq!(into.len(), 50_000);
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?}", started.elapsed());
    }

    #[test]
    fn a_group_extends_another() {
        let found = tokens(
            r#"{
                "button": { "$type": "number", "size": { "$value": 1 }, "inner": { "pad": { "$value": 2 }, "gap": { "$value": 3 } } },
                "large": { "$extends": "{button}", "size": { "$value": 10 }, "inner": { "gap": { "$value": 30 } } }
            }"#,
        )
        .unwrap();
        let names: Vec<String> = found.iter().map(|(k, v)| format!("{k}:{v}")).collect();
        assert_eq!(
            names,
            [
                "--button-size:1",
                "--button-inner-pad:2",
                "--button-inner-gap:3",
                "--large-size:10",
                "--large-inner-pad:2",
                "--large-inner-gap:30"
            ]
        );
        let error = |text: &str| tokens(text).expect_err("an error");
        assert!(error(r#"{"a": {"$extends": "{b}"}, "b": {"$extends": "{a}"}}"#).contains("extends this group"));
        assert!(
            error(r#"{"a": {"$type": "number", "x": {"$value": 1}, "b": {"$extends": "{a}"}}}"#).contains("a group around it")
        );
        assert!(error(r#"{"a": {"$extends": "{b.x}"}, "b": {"x": {"$type": "number", "$value": 1}}}"#).contains("not a group"));
    }

    fn resolver(text: &str) -> Result<Tokens, String> {
        resolver_document(&json::parse(text).unwrap(), None, &mut Files::default())
    }

    const BASE: &str = r#"{"color": {"$type": "color",
        "ink": {"$value": {"colorSpace": "srgb", "components": [0, 0, 0]}},
        "bg": {"$value": {"colorSpace": "srgb", "components": [1, 1, 1]}},
        "link": {"$value": "{color.ink}"}}}"#;
    const DARK: &str = r#"{"color": {"ink": {"$value": {"colorSpace": "srgb", "components": [1, 1, 1]}},
        "bg": {"$value": {"colorSpace": "srgb", "components": [0, 0, 0]}}}}"#;

    #[test]
    fn a_resolver_gives_the_defaults_and_what_each_context_changes() {
        let found = resolver(&format!(
            r##"{{
                "version": "2025.10",
                "sets": {{ "base": {{ "sources": [{BASE}] }} }},
                "modifiers": {{ "theme": {{
                    "contexts": {{ "light": [], "dark": [{DARK}] }}, "default": "light",
                    "$extensions": {{ "run.corten": {{ "media": {{ "dark": "(prefers-color-scheme: dark)" }} }} }}
                }} }},
                "resolutionOrder": [
                    {{ "$ref": "#/sets/base" }},
                    {{ "$ref": "#/modifiers/theme" }},
                    {{ "type": "modifier", "name": "contrast", "contexts": {{ "normal": [], "more": [{{ "color": {{ "link": {{ "$value": "{{color.bg}}" }} }} }}] }}, "default": "normal" }}
                ]
            }}"##
        ))
        .unwrap();
        let base: Vec<String> = found.tokens.iter().map(|t| format!("{}:{}", t.property(), t.css)).collect();
        assert_eq!(base, ["--color-ink:#000000", "--color-bg:#ffffff", "--color-link:var(--color-ink)"]);
        assert_eq!(found.modifiers.len(), 2);
        let theme = &found.modifiers[0];
        assert_eq!((theme.name.as_str(), theme.default.as_str()), ("theme", "light"));
        let light = vec![
            ("--color-ink".to_string(), "#000000".to_string()),
            ("--color-bg".into(), "#ffffff".into()),
            ("--color-link".into(), "var(--color-ink)".into()),
        ];
        let dark = vec![
            ("--color-ink".to_string(), "#ffffff".to_string()),
            ("--color-bg".into(), "#000000".into()),
            ("--color-link".into(), "var(--color-ink)".into()),
        ];
        assert_eq!(
            theme.contexts,
            [("light".to_string(), light), ("dark".to_string(), dark)],
            "the default declares what dark changes, and link, which reads ink, is declared again in each"
        );
        assert!(theme.extension.is_some());
        assert_eq!(
            found.modifiers[1].contexts,
            [
                ("normal".to_string(), vec![("--color-link".into(), "var(--color-ink)".into())]),
                ("more".to_string(), vec![("--color-link".into(), "var(--color-bg)".into())])
            ]
        );
    }

    /// Writes `files` to a folder of their own and reads `first` with [`read`].
    #[test]
    fn a_shadow_inset_may_be_a_pointer() {
        let found = tokens(r##"{"c": {"$type": "color", "k": {"$value": {"colorSpace": "srgb", "components": [0, 0, 0]}}},
            "d": {"$type": "dimension", "z": {"$value": {"value": 0, "unit": "px"}}},
            "s": {"$type": "shadow", "a": {"$value": {"color": "{c.k}", "offsetX": "{d.z}", "offsetY": "{d.z}", "blur": "{d.z}", "spread": "{d.z}", "inset": true}},
              "b": {"$value": {"color": "{c.k}", "offsetX": "{d.z}", "offsetY": "{d.z}", "blur": "{d.z}", "spread": "{d.z}", "inset": {"$ref": "#/s/a/$value/inset"}}}}}"##)
        .unwrap();
        assert_eq!(found[3].1, "inset var(--d-z) var(--d-z) var(--d-z) var(--d-z) var(--c-k)");
    }

    #[test]
    fn contexts_of_two_modifiers_together_give_what_the_resolver_gives() {
        let two = |dark: &str, compact: &str| {
            resolver(&format!(
                r#"{{"version": "2025.10", "resolutionOrder": [
                    {{"type": "set", "name": "base", "sources": [{{"n": {{"$type": "number", "$value": 1}}, "m": {{"$type": "number", "$value": 1}}, "x": {{"$type": "number", "$value": "{{n}}"}}}}]}},
                    {{"type": "modifier", "name": "theme", "default": "light", "contexts": {{"light": [], "dark": [{dark}]}}}},
                    {{"type": "modifier", "name": "density", "default": "normal", "contexts": {{"normal": [], "compact": [{compact}]}}}}]}}"#
            ))
        };
        // Each changes a token of its own, and `x` reads `n` by reference: right in any combination.
        two(r#"{"n": {"$type": "number", "$value": 3}}"#, r#"{"m": {"$type": "number", "$value": 2}}"#).unwrap();
        // Both change `n`: `normal` declares it again (it is what `compact` changes), so
        // `dark` with `normal` chosen would be 1, where the resolver gives 3.
        let error = two(r#"{"n": {"$type": "number", "$value": 3}}"#, r#"{"n": {"$type": "number", "$value": 2}}"#)
            .err()
            .expect("an error");
        assert!(error.contains("with theme dark and density normal, --n resolves to 3 and the CSS rules give 1"), "{error}");
        // `dark` sets `x` itself, and `compact`'s rule, declaring `x: var(--n)` again so it
        // follows `n`, would undo that with both on one element.
        let error = two(r#"{"x": {"$type": "number", "$value": 5}}"#, r#"{"n": {"$type": "number", "$value": 2}}"#)
            .err()
            .expect("an error");
        assert!(
            error.contains("with theme dark and density") && error.contains("--x resolves to 5 and the CSS rules give var(--n)"),
            "{error}"
        );
        // With density before theme: `compact` sets `x`, and `light` (theme's default) declares
        // `x: var(--n)` again, as `dark` changes `n`; chosen with `compact`, its later rule
        // would undo `compact`'s.
        let error = resolver(
            r#"{"version": "2025.10", "resolutionOrder": [
                {"type": "set", "name": "base", "sources": [{"n": {"$type": "number", "$value": 1}, "x": {"$type": "number", "$value": "{n}"}}]},
                {"type": "modifier", "name": "density", "default": "normal", "contexts": {"normal": [], "compact": [{"x": {"$type": "number", "$value": 7}}]}},
                {"type": "modifier", "name": "theme", "default": "light", "contexts": {"light": [], "dark": [{"n": {"$type": "number", "$value": 3}}]}}]}"#,
        )
        .err()
        .expect("an error");
        assert!(error.contains("with density compact and theme light, --x resolves to 7"), "{error}");
    }

    fn files(name: &str, files: &[(&str, &str)], first: &str) -> Result<Tokens, String> {
        let dir = std::env::temp_dir().join(format!("corten-dtcg-{name}-{}", std::process::id()));
        for (path, text) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let out = read(&dir.join(first));
        std::fs::remove_dir_all(&dir).unwrap();
        out
    }

    #[test]
    fn files_are_found_from_the_file_that_names_them() {
        let resolver = r##"{"version": "2025.10", "resolutionOrder": [{"$ref": "sub/sets.json#/sets/base"}]}"##;
        let sets = r#"{"sets": {"base": {"sources": [{"$ref": "tokens.json"}]}}}"#;
        let tokens = r#"{"g": {"$type": "number", "a": {"$value": 1}}}"#;
        // Named `themes.json`: a resolver is known by its resolutionOrder, not its name.
        let found =
            files("nested", &[("themes.json", resolver), ("sub/sets.json", sets), ("sub/tokens.json", tokens)], "themes.json")
                .unwrap();
        assert_eq!(found.tokens.len(), 1);
        assert_eq!(found.files.len(), 3, "each file read is a rebuild dependency");
        // A part of a file: its pointers name places in the file.
        let part = r##"{"other": {"x": {"$type": "number", "$value": 7}}, "tokens": {"$type": "number",
            "a": {"$value": 1}, "b": {"$ref": "#/tokens/a/$value"}, "c": {"$ref": "#/other/x/$value"}}}"##;
        let resolver = r#"{"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "s", "sources": [{"$ref": "t.json#/tokens"}]}]}"#;
        let found = files("part", &[("r.json", resolver), ("t.json", part)], "r.json").unwrap();
        let css: Vec<String> = found.tokens.iter().map(|t| format!("{}:{}", t.property(), t.css)).collect();
        assert_eq!(css, ["--a:1", "--b:var(--a)", "--c:7"]);
        let looping = r##"{"sets": {"a": {"sources": [{"$ref": "./a.json#/sets/a"}]}}}"##;
        let error = files(
            "loop",
            &[("r.json", r##"{"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "x", "sources": [{"$ref": "a.json#/sets/a"}]}]}"##), ("a.json", looping)],
            "r.json",
        )
        .err()
        .expect("an error");
        assert!(error.contains("includes itself"), "{error}");
        // A token file may have a group named `sets`: a part of it is tokens, not a set.
        let named = r#"{"sets": {"base": {"$type": "number", "a": {"$value": 1}}}}"#;
        let resolver = r##"{"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "s", "sources": [{"$ref": "t.json#/sets/base"}]}]}"##;
        let found = files("named", &[("r.json", resolver), ("t.json", named)], "r.json").unwrap();
        let css: Vec<String> = found.tokens.iter().map(|t| format!("{}:{}", t.property(), t.css)).collect();
        assert_eq!(css, ["--a:1"]);
        // A file is a path relative to the one naming it: no scheme, drive or absolute path.
        for target in ["https://example.com/t.json", "file:t.json", "C:/t.json", "/etc/t.json", "\\\\host\\t.json"] {
            let resolver = format!(
                r#"{{"version": "2025.10", "resolutionOrder": [{{"type": "set", "name": "s", "sources": [{{"$ref": {target:?}}}]}}]}}"#
            );
            let error = files("absolute", &[("r.json", &resolver)], "r.json").err().expect("an error");
            assert!(error.contains("not a path relative to the file that names it"), "{target}: {error}");
        }
        // A pointer outside the part to a whole token is its value, or what its `$ref` names.
        let part = r##"{"other": {"x": {"$type": "number", "$value": 7}, "y": {"$type": "number", "$ref": "#/other/x"}},
            "tokens": {"$type": "number", "a": {"$ref": "#/other/x"}, "b": {"$ref": "#/other/y"}}}"##;
        let resolver = r#"{"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "s", "sources": [{"$ref": "t.json#/tokens"}]}]}"#;
        let found = files("whole", &[("r.json", resolver), ("t.json", part)], "r.json").unwrap();
        let css: Vec<String> = found.tokens.iter().map(|t| format!("{}:{}", t.property(), t.css)).collect();
        assert_eq!(css, ["--a:7", "--b:7"]);
        // `$extensions` in a part keeps a `$ref` of a tool's as it is.
        let part = r##"{"tokens": {"$type": "number", "a": {"$value": 1, "$extensions": {"com.tool": {"$ref": "#/nowhere"}}}}}"##;
        let resolver = r#"{"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "s", "sources": [{"$ref": "t.json#/tokens"}]}]}"#;
        assert_eq!(files("vendor", &[("r.json", resolver), ("t.json", part)], "r.json").unwrap().tokens.len(), 1);
        // The Resolver module's `example.json#sets/MySet` (4.2) is `#/sets/MySet`.
        let resolver = r##"{"version": "2025.10", "resolutionOrder": [{"$ref": "example.json#sets/MySet"}]}"##;
        let example = r#"{"sets": {"MySet": {"sources": [{"g": {"$type": "number", "a": {"$value": 1}}}]}}}"#;
        let found = files("noslash", &[("r.json", resolver), ("example.json", example)], "r.json").unwrap();
        assert_eq!(found.tokens.len(), 1);
        // An array index has no leading zero (RFC 6901); a group may be named resolutionOrderX.
        let resolver = r##"{"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "s", "sources": [{"$ref": "t.json#/resolutionOrderX"}]}]}"##;
        let found = files(
            "prefix",
            &[("r.json", resolver), ("t.json", r#"{"resolutionOrderX": {"a": {"$type": "number", "$value": 1}}}"#)],
            "r.json",
        )
        .unwrap();
        assert_eq!(found.tokens.len(), 1);
        // `#/` is the whole file.
        let resolver =
            r##"{"version": "2025.10", "resolutionOrder": [{"type": "set", "name": "s", "sources": [{"$ref": "t.json#/"}]}]}"##;
        let found =
            files("root", &[("r.json", resolver), ("t.json", r#"{"a": {"$type": "number", "$value": 1}}"#)], "r.json").unwrap();
        assert_eq!(found.tokens.len(), 1);
    }

    #[test]
    fn resolver_mistakes_are_errors() {
        let error = |order: &str, rest: &str| {
            resolver(&format!(r#"{{"version": "2025.10", {rest} "resolutionOrder": [{order}]}}"#)).err().expect("an error")
        };
        let set = format!(r#"{{"type": "set", "name": "base", "sources": [{BASE}]}}"#);
        let theme = |contexts: &str, default: &str| {
            format!(r#"{{"type": "modifier", "name": "theme", "contexts": {{{contexts}}}{default}}}"#)
        };
        let adds = r#""light": [], "dark": [{"color": {"new": {"$value": "{color.ink}"}}}]"#;
        assert!(error(&format!("{set}, {}", theme(adds, r#", "default": "light""#)), "")
            .contains("modifier theme, context dark defines color.new, which the defaults do not"));
        let dark = format!(r#""light": [], "dark": [{DARK}]"#);
        assert!(error(&format!("{set}, {}", theme(&dark, "")), "").contains("has no default"));
        assert!(error(&format!("{set}, {}", theme(&dark, r#", "default": "dim""#)), "")
            .contains("\"dim\" is not one of its contexts"));
        assert!(error(&format!("{set}, {}", theme(r#""light": []"#, r#", "default": "light""#)), "").contains("has 1 context"));
        let retyped = r#""light": [], "dark": [{"color": {"link": {"$type": "number", "$value": 1}}}]"#;
        assert!(error(&format!("{set}, {}", theme(retyped, r#", "default": "light""#)), "")
            .contains("color.link is number, and color by default"));
        let typed = DARK.replacen(r#"{"color": {"#, r#"{"color": {"$type": "color", "#, 1);
        let only_light = format!(r#""light": [{BASE}], "dark": [{typed}]"#);
        assert!(error(&theme(&only_light, r#", "default": "light""#), "").contains("context dark has no color.link"));
        assert!(error(&format!("{set}, {set}"), "").contains("two items of resolutionOrder are named \"base\""));
        assert!(error(r##"{"$ref": "#/sets/missing"}"##, r#""sets": {},"#).contains("names nothing"));
        assert!(error(r#"{"type": "set", "name": "a", "sources": [{"$ref": "https://example.com/t.json"}]}"#, "")
            .contains("reads no URLs"));
        assert!(error(r#"{"$ref": "base.tokens.json"}"#, "").contains("is not a set or a modifier"));
        assert!(error(r##"{"type": "set", "name": "a", "sources": [{"$ref": "#/modifiers/m"}]}"##, "")
            .contains("only resolutionOrder may reference a modifier"));
        assert!(error(r##"{"$ref": "#/sets/a"}"##, r##""sets": {"a": {"sources": [{"$ref": "#/sets/a"}]}},"##)
            .contains("includes itself"));
        assert!(error(r##"{"type": "set", "name": "a", "sources": [], "$extensions": {"run.corten": {}}}"##, "")
            .contains("means nothing"));
        assert!(error(r##"{"type": "set", "name": "a", "sources": [{"$ref": "#/$defs/x"}]}"##, r#""$defs": {"x": 42},"#)
            .contains("is an object, not a number"));
        assert!(error(r#"{"type": "set", "name": "a", "sources": [42]}"#, "").contains("is an object, not a number"));
        assert!(
            error(r#"{"type": "set", "name": "a", "description": 42, "sources": []}"#, "").contains("description is a string")
        );
        assert!(error("", r#""description": [],"#).contains("description is a string"));
        let mut sets = String::new();
        for n in 0..300 {
            sets.push_str(&format!(r##""s{n}": {{"sources": [{{"$ref": "#/sets/s{}"}}]}}, "##, n + 1));
        }
        sets.push_str(r#""s300": {"sources": []}"#);
        assert!(error(r##"{"$ref": "#/sets/s0"}"##, &format!(r#""sets": {{{sets}}},"#)).contains("a chain of more than 256 sets"));
        // Each set naming the next twice: 2^20 copies, stopped by the budget, not the compiler.
        let mut twice = String::new();
        for n in 0..20 {
            twice.push_str(&format!(
                r##""s{n}": {{"sources": [{{"$ref": "#/sets/s{0}"}}, {{"$ref": "#/sets/s{0}"}}]}}, "##,
                n + 1
            ));
        }
        twice.push_str(r#""s20": {"sources": [{"g": {"$type": "number", "a": {"$value": 1}}}]}"#);
        assert!(error(r##"{"$ref": "#/sets/s0"}"##, &format!(r#""sets": {{{twice}}},"#)).contains("more than 100000 references"));
        // A set or modifier is checked whether or not resolutionOrder names it.
        assert!(error(r##"{"$ref": "#/sets/a"}"##, r#""sets": {"a": {"sources": []}, "unused": {"sources": 1}},"#)
            .contains("sets.unused: sources are an array, not a number"));
        assert!(error(r##"{"$ref": "#/sets/a"}"##, r#""sets": {"a": {"sources": []}, "unused": {"sources": [42]}},"#)
            .contains("sets.unused, source 0 is an object, not a number"));
        assert!(error(
            r##"{"$ref": "#/sets/a"}"##,
            r#""sets": {"a": {"sources": []}}, "modifiers": {"m": {"default": "x", "contexts": {"x": []}}},"#
        )
        .contains("modifiers.m has 1 context"));
        assert!(error(
            r##"{"$ref": "#/sets/a"}"##,
            r#""sets": {"a": {"sources": []}}, "modifiers": {"m": {"contexts": {"x": {}}}},"#
        )
        .contains("modifiers.m, context x: sources are an array, not an object"));
        assert!(error(r##"{"$ref": "#/sets/a"}"##, r#""sets": {"a": {"sources": []}, "b": {"sources": [], "name": "b"}},"#)
            .contains("sets.b: unknown property \"name\" of a set"));
        assert!(resolver(r#"{"version": "2025-10-01", "resolutionOrder": []}"#).err().unwrap().contains("reads \"2025.10\""));
        assert!(resolver(r#"{"resolutionOrder": []}"#).err().unwrap().contains("no version"));
    }
}
