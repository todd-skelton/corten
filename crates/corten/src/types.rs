//! TypeScript declarations for the data scripts read, generated from the Rust types (feature
//! `types`), so handler code is type-checked against the server.
//!
//! Every value that crosses into the browser as JSON is a Rust type: a handler's arguments, a
//! typed event's fields, a feed's rows, a screen's data. Derive `schemars::JsonSchema` on them next to `Serialize` (the
//! app depends on `schemars = "1"` as it does on `serde`), list them and the app's components once,
//! and keep the generated file in sync from a test:
//!
//! ```ignore
//! #[test]
//! fn script_types_match_the_server() {
//!     corten::types::Types::new().add::<Card>().components(views::COMPONENTS).sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
//! }
//! ```
//!
//! `sync` rewrites the file when the Rust types changed and fails the test, so CI (which runs on
//! a clean checkout) fails until the new file is committed. The app's typed events (`#[event]`)
//! are declared as the classes of the module `corten/events`, with their fields, so a script
//! that dispatches `new RowToggled({ id, selected })` is checked against the Rust struct. Other
//! types are imported in JSDoc (`/** @typedef {import("../types/app").Card} Card */`). `tsc
//! --checkJs` then fails on a renamed or retyped field, not the page in production. Handler
//! parameters need nothing generated: the markup's calls are checked against the handler's own
//! JSDoc when the crate compiles. The file holds types only; the browser never loads it.
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
    /// Each typed event's class in `corten/events`, by name.
    events: BTreeMap<&'static str, String>,
    /// Each view's class in `corten/views`, by name.
    views: BTreeMap<&'static str, &'static str>,
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
            events: BTreeMap::new(),
            views: BTreeMap::new(),
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

    /// Adds the typed events among the app's components (the list it gives `App::components`)
    /// and those their scripts import, to `corten/events`, with the types of their fields, and the
    /// views (`#[component(view)]`) to `corten/views`.
    pub fn components(mut self, components: &[crate::Component]) -> Self {
        for view in components.iter().filter_map(|c| c.view) {
            self.views.insert(view.class, view.ts);
        }
        let imported = components.iter().filter_map(|c| c.script).flat_map(|s| s.events);
        for event in components.iter().chain(imported).filter_map(|c| c.event) {
            let mut fields = String::new();
            let mut init = Vec::new();
            for field in event.fields {
                let schema = (field.ts)(&mut self.generator);
                let ty = ts(&schema, "    ");
                let _ = write!(fields, "\n    readonly {}: {ty};", field.name);
                init.push(format!("{}: {ty}", field.name));
            }
            let init = if init.is_empty() { "init?: {}".to_string() } else { format!("init: {{ {} }}", init.join("; ")) };
            let entry = format!(
                "  /** The `{}` event, from #[event] {}. */\n  export class {} extends Event {{\n    constructor({init});{fields}\n  }}\n",
                event.name, event.class, event.class
            );
            self.events.insert(event.class, entry);
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
        if !self.events.is_empty() {
            out.push_str("\n/** The app's typed events (#[event]), dispatched by scripts and listened for with on:Name. */\ndeclare module \"corten/events\" {\n");
            for entry in self.events.values() {
                out.push_str(entry);
            }
            out.push_str("}\n");
        }
        if !self.views.is_empty() {
            out.push_str("\n/** The app's views (#[component(view)]): each one's holes, read from and written to the page. */\ndeclare module \"corten/views\" {\n");
            out.push_str(crate::script::VIEW_TYPES);
            for ts in self.views.values() {
                out.push_str(ts);
            }
            out.push_str("}\n");
        }
        if !self.events.is_empty() || !self.views.is_empty() {
            out.push_str("\nexport {};\n");
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

    /// A calendar day is its text in a row's declaration.
    #[test]
    fn a_day_is_a_string() {
        #[derive(Serialize, JsonSchema)]
        #[allow(dead_code)]
        struct Week {
            start: crate::i18n::Day,
            end: Option<crate::i18n::Day>,
        }
        let ts = Types::new().add::<Week>().typescript();
        assert!(ts.contains("end: string | null;\n  start: string;"), "{ts}");
    }

    /// A card was picked (named apart from the script tests' `RowToggled`: one event of a name per program).
    #[crate::script::event]
    #[allow(dead_code)]
    struct CardPicked {
        id: String,
        kind: Kind,
    }

    #[test]
    fn typed_events_declare_their_classes() {
        let ts = Types::new().components(&[CardPicked]).typescript();
        let expected = r#"
/** The app's typed events (#[event]), dispatched by scripts and listened for with on:Name. */
declare module "corten/events" {
  /** The `card-picked` event, from #[event] CardPicked. */
  export class CardPicked extends Event {
    constructor(init: { id: string; kind: Kind });
    readonly id: string;
    readonly kind: Kind;
  }
}

export {};
"#;
        assert!(ts.ends_with(expected), "{ts}");
        assert!(ts.contains("export type Kind = "), "a field's type is declared with the rest: {ts}");
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
