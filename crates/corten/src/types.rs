//! TypeScript declarations for the data islands read, generated from the Rust types (feature
//! `types`), so island code is type-checked against the server.
//!
//! Every value that crosses into the browser as JSON is a Rust type: an island's `#[prop]`s, a
//! feed's rows, a screen's data. Derive `schemars::JsonSchema` on them next to `Serialize` (the
//! app depends on `schemars = "1"` as it does on `serde`), list them and the app's islands once,
//! and keep the generated file in sync from a test:
//!
//! ```ignore
//! #[test]
//! fn island_types_match_the_server() {
//!     corten::types::Types::new().add::<Card>().islands(views::ISLANDS).sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
//! }
//! ```
//!
//! `sync` rewrites the file when the Rust types changed and fails the test, so CI (which runs on
//! a clean checkout) fails until the new file is committed. For islands the file declares the
//! global `CortenIslands`: each island's props, the elements its refs are on and its actions'
//! events, as the markup declares them. `island("tag", ...)` is checked against it with no import,
//! and other types are imported in JSDoc (`/** @typedef {import("../types/app").Card} Card */`).
//! `tsc --checkJs` then fails on a renamed or retyped field, ref or action, not the page in
//! production. The file holds types only; the browser never loads it.
//!
//! Types describe the JSON as the server writes it (`skip_serializing_if` fields are optional,
//! `Option` is `T | null`). [`Types::json_schema`] gives the same definitions as JSON Schema,
//! for native code generators.

use std::{collections::BTreeMap, fmt::Write as _, path::Path};

use schemars::{generate::SchemaSettings, JsonSchema, SchemaGenerator};
use serde_json::{Map, Value};

/// The set of types one app sends to the browser.
pub struct Types {
    generator: SchemaGenerator,
    roots: BTreeMap<String, Value>,
    /// Each island's entry in `CortenIslands`, by tag.
    islands: BTreeMap<&'static str, String>,
}

impl Default for Types {
    fn default() -> Self {
        Self::new()
    }
}

impl Types {
    pub fn new() -> Self {
        Types {
            generator: SchemaSettings::draft2020_12().for_serialize().into_generator(),
            roots: BTreeMap::new(),
            islands: BTreeMap::new(),
        }
    }

    /// Adds `T` and every type it refers to, under their Rust names.
    pub fn add<T: JsonSchema>(mut self) -> Self {
        let schema = self.generator.subschema_for::<T>().to_value();
        // A type schemars inlines (no $ref) is exported under its own name.
        if schema.get("$ref").is_none() {
            self.roots.insert(T::schema_name().into_owned(), schema);
        }
        self
    }

    /// Adds the app's islands (`#[island]` components' `ISLAND`) to `CortenIslands`, with the
    /// types of their props.
    pub fn islands(mut self, islands: &[crate::Island]) -> Self {
        let tags = |tags: &[&str]| union(tags.iter().map(|t| format!("{t:?}")).collect());
        for island in islands {
            let mut entry = format!(
                "    /** `<{}>`: {} */
    {:?}: {{
      props: {{",
                island.tag, island.file, island.tag
            );
            for prop in island.props {
                let schema = (prop.ts)(&mut self.generator);
                let _ = write!(entry, " {}: {};", prop.name, ts(&schema, "      "));
            }
            entry.push_str(if island.props.is_empty() {
                "};
      refs: {"
            } else {
                " };
      refs: {"
            });
            for r in island.refs {
                let _ = write!(entry, " {}: {};", r.name, tags(r.tags));
            }
            entry.push_str(if island.refs.is_empty() {
                "};
      actions: {"
            } else {
                " };
      actions: {"
            });
            for a in island.actions {
                let _ = write!(entry, " {}: {{ on: {}; el: {} }};", a.name, tags(a.events), tags(a.tags));
            }
            entry.push_str(if island.actions.is_empty() {
                "};
    };
"
            } else {
                " };
    };
"
            });
            self.islands.insert(island.tag, entry);
        }
        self
    }

    fn definitions(&self) -> BTreeMap<String, Value> {
        let mut defs: BTreeMap<String, Value> =
            self.generator.definitions().iter().map(|(name, schema)| (name.clone(), schema.clone())).collect();
        defs.extend(self.roots.iter().map(|(k, v)| (k.clone(), v.clone())));
        defs
    }

    /// Every definition as JSON Schema (draft 2020-12): `{"$defs": {...}}`.
    pub fn json_schema(&self) -> Value {
        let defs: Map<String, Value> = self.definitions().into_iter().collect();
        serde_json::json!({ "$schema": "https://json-schema.org/draft/2020-12/schema", "$defs": defs })
    }

    /// The declarations file: one `export` per type, in name order.
    pub fn typescript(&self) -> String {
        let mut out = String::from(
            "// Generated by corten::types from the Rust types; do not edit. The test that calls\n// Types::sync rewrites this file when they change.\n",
        );
        let mut blocks = BTreeMap::new();
        for (name, schema) in self.definitions() {
            let mut block = String::new();
            if let Some(text) = schema.get("description").and_then(Value::as_str) {
                doc(&mut block, "", text);
            }
            let is_interface = schema.get("type") == Some(&Value::from("object"))
                && schema.get("properties").is_some()
                && schema.get("additionalProperties").is_none_or(|v| v == &Value::Bool(false));
            if is_interface {
                let _ = writeln!(block, "export interface {name} {}", object(&schema, ""));
            } else {
                let _ = writeln!(block, "export type {name} = {};", ts(&schema, ""));
            }
            blocks.insert(name, block);
        }
        for block in blocks.values() {
            out.push('\n');
            out.push_str(block);
        }
        if !self.islands.is_empty() {
            out.push_str("\ndeclare global {\n  /** The app's islands, from its #[island] components: what island(\"tag\", ...) is checked against. */\n  interface CortenIslands {\n");
            for entry in self.islands.values() {
                out.push_str(entry);
            }
            out.push_str("  }\n}\n\nexport {};\n");
        }
        out
    }

    /// Writes [`Types::typescript`] to `path` when it differs from what is there, and then panics
    /// with the file name, so a test that calls it fails until the new file is committed.
    pub fn sync(&self, path: impl AsRef<Path>) {
        let path = path.as_ref();
        let text = self.typescript();
        if std::fs::read_to_string(path).ok().as_deref() == Some(text.as_str()) {
            return;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("create the types directory");
        }
        std::fs::write(path, &text).expect("write the types file");
        panic!("{} was out of date with the Rust types and has been rewritten; run npm run check and commit it", path.display());
    }
}

fn doc(out: &mut String, indent: &str, text: &str) {
    let text = text.replace("*/", "*\\/");
    if text.contains('\n') {
        let _ = writeln!(out, "{indent}/**");
        for line in text.lines() {
            let _ = writeln!(out, "{indent} * {line}");
        }
        let _ = writeln!(out, "{indent} */");
    } else {
        let _ = writeln!(out, "{indent}/** {text} */");
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn object(schema: &Value, indent: &str) -> String {
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let inner = format!("{indent}  ");
    let mut out = String::from("{\n");
    if let Some(Value::Object(properties)) = schema.get("properties") {
        for (name, property) in properties {
            if let Some(text) = property.get("description").and_then(Value::as_str) {
                doc(&mut out, &inner, text);
            }
            let key = if is_identifier(name) { name.clone() } else { serde_json::to_string(name).expect("a string serializes") };
            let optional = if required.contains(&name.as_str()) { "" } else { "?" };
            let _ = writeln!(out, "{inner}{key}{optional}: {};", ts(property, &inner));
        }
    }
    match schema.get("additionalProperties") {
        Some(Value::Bool(false)) | None => {}
        Some(Value::Bool(true)) => {
            let _ = writeln!(out, "{inner}[key: string]: unknown;");
        }
        Some(value) => {
            let _ = writeln!(out, "{inner}[key: string]: {};", ts(value, &inner));
        }
    }
    out.push_str(indent);
    out.push('}');
    out
}

/// One schema as a TypeScript type. Covers what schemars emits for serde types.
fn ts(schema: &Value, indent: &str) -> String {
    let schema = match schema {
        Value::Bool(true) => return "unknown".into(),
        Value::Bool(false) => return "never".into(),
        Value::Object(map) => map,
        _ => return "unknown".into(),
    };
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().unwrap_or("unknown").to_string();
    }
    if let Some(value) = schema.get("const") {
        return serde_json::to_string(value).expect("JSON serializes");
    }
    if let Some(Value::Array(values)) = schema.get("enum") {
        return union(values.iter().map(|v| serde_json::to_string(v).expect("JSON serializes")).collect());
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(options)) = schema.get(key) {
            return union(options.iter().map(|o| ts(o, indent)).collect());
        }
    }
    if let Some(Value::Array(parts)) = schema.get("allOf") {
        return parts.iter().map(|p| ts(p, indent)).collect::<Vec<_>>().join(" & ");
    }
    let one = |kind: &str| -> String {
        match kind {
            "string" => "string".into(),
            "integer" | "number" => "number".into(),
            "boolean" => "boolean".into(),
            "null" => "null".into(),
            "array" => match (schema.get("prefixItems"), schema.get("items")) {
                (Some(Value::Array(items)), _) => {
                    format!("[{}]", items.iter().map(|i| ts(i, indent)).collect::<Vec<_>>().join(", "))
                }
                (_, Some(items)) => {
                    let item = ts(items, indent);
                    if item.contains(' ') {
                        format!("({item})[]")
                    } else {
                        format!("{item}[]")
                    }
                }
                _ => "unknown[]".into(),
            },
            "object" => {
                if schema.get("properties").is_some() {
                    object(&Value::Object(schema.clone()), indent)
                } else {
                    match schema.get("additionalProperties") {
                        Some(Value::Bool(false)) => "Record<string, never>".into(),
                        Some(value) if value != &Value::Bool(true) => format!("Record<string, {}>", ts(value, indent)),
                        _ => "Record<string, unknown>".into(),
                    }
                }
            }
            _ => "unknown".into(),
        }
    };
    match schema.get("type") {
        Some(Value::String(kind)) => one(kind),
        Some(Value::Array(kinds)) => union(kinds.iter().filter_map(Value::as_str).map(one).collect()),
        _ => "unknown".into(),
    }
}

fn union(mut parts: Vec<String>) -> String {
    parts.dedup();
    match parts.len() {
        0 => "never".into(),
        1 => parts.remove(0),
        _ => parts.join(" | "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    /// A card on the watchlist.
    #[derive(Serialize, JsonSchema)]
    #[allow(dead_code)]
    struct Card {
        id: String,
        /// Price in cents.
        price_cents: u32,
        rating: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        tags: Vec<String>,
        kind: Kind,
        grades: BTreeMap<String, f64>,
        pair: (u8, String),
        #[serde(rename = "data-x")]
        data_x: bool,
    }

    #[derive(Serialize, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    #[allow(dead_code)]
    enum Kind {
        Single,
        SealedBox,
    }

    #[derive(Serialize, JsonSchema)]
    #[serde(tag = "op", rename_all = "lowercase")]
    #[allow(dead_code)]
    enum Change {
        Upsert { id: String, value: Card },
        Remove { id: String },
    }

    #[test]
    fn declarations_follow_the_json_the_server_writes() {
        let ts = Types::new().add::<Change>().typescript();
        let expected = r#"
/** A card on the watchlist. */
export interface Card {
  "data-x": boolean;
  grades: Record<string, number>;
  id: string;
  kind: Kind;
  note?: string | null;
  pair: [number, string];
  /** Price in cents. */
  price_cents: number;
  rating: string | null;
  tags: string[];
}

export type Change = {
  id: string;
  op: "upsert";
  value: Card;
} | {
  id: string;
  op: "remove";
};

export type Kind = "single" | "sealed_box";
"#;
        let body = ts.split_once("change.\n").map(|(_, rest)| rest).unwrap_or(&ts);
        assert_eq!(body, expected, "{ts}");
        // A doc comment on the type becomes the declaration's doc comment.
        assert!(Types::new().add::<Card>().typescript().contains("/** A card on the watchlist. */\nexport interface Card {"));
        assert!(Types::new().add::<Card>().json_schema()["$defs"]["Kind"]["enum"].is_array());
    }

    #[crate::island]
    #[allow(dead_code)]
    fn PriceBoard<'a>(title: &'a str, #[prop] card: &'a Card) -> impl crate::Render {
        crate::html! { <h2 ref={Title} on:click={Pick}>{title}</h2><template ref={Rest}></template> }
    }

    #[test]
    fn islands_declare_their_props_refs_and_actions() {
        let ts = Types::new().add::<Kind>().islands(&[PriceBoard::ISLAND]).typescript();
        let expected = r#"
declare global {
  /** The app's islands, from its #[island] components: what island("tag", ...) is checked against. */
  interface CortenIslands {
    /** `<price-board>`: price_board.js */
    "price-board": {
      props: { card: Card; };
      refs: { title: "h2"; rest: "template"; };
      actions: { pick: { on: "click"; el: "h2" }; };
    };
  }
}

export {};
"#;
        assert!(ts.ends_with(expected), "{ts}");
        assert!(ts.contains("export interface Card {"), "a prop's type is declared with the rest: {ts}");
    }

    #[test]
    fn sync_rewrites_a_stale_file_and_fails_then_passes() {
        let path = std::env::temp_dir().join(format!("corten-types-{}/app.d.ts", std::process::id()));
        let types = Types::new().add::<Card>();
        let stale = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| types.sync(&path)));
        assert!(stale.is_err(), "a missing file fails once");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), types.typescript());
        types.sync(&path);
    }
}
