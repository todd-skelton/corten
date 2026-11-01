//! `corten ui`: the UI kit. `init` writes the theme tokens and `src/ui.rs`, `add` copies a
//! component's source into `src/ui/`, where it is the app's to change, and `diff` shows how a copy
//! differs from the kit this command carries. The files are embedded in the
//! binary (`kit/`), so this needs no network and the command stays standard-library only.

use std::path::{Path, PathBuf};

/// A component of the kit: its files, and the component `src/ui.rs` lists in `COMPONENTS` (the one
/// that owns the stylesheet).
struct Piece {
    name: &'static str,
    summary: &'static str,
    owner: &'static str,
    files: &'static [(&'static str, &'static str)],
    /// The corten-ui component it styles, which the app depends on and lists itself.
    styles: Option<&'static str>,
}

macro_rules! piece {
    ($name:literal, $owner:literal, $summary:literal, [$($file:literal),+]) => {
        Piece { name: $name, owner: $owner, summary: $summary, files: &[$(($file, include_str!(concat!("../kit/", $file)))),+], styles: None }
    };
    ($name:literal, $owner:literal, $summary:literal, [$($file:literal),+], styles $widget:literal) => {
        Piece { name: $name, owner: $owner, summary: $summary, files: &[$(($file, include_str!(concat!("../kit/", $file)))),+], styles: Some($widget) }
    };
}

const KIT: &[Piece] = &[
    piece!("accordion", "Accordion", "sections that open one at a time, on <details name>", ["accordion.rs", "accordion.css"]),
    piece!(
        "alert",
        "Alert",
        "a message the page calls out, neutral, info, success, warning or destructive",
        ["alert.rs", "alert.css"]
    ),
    piece!("avatar", "Avatar", "a person's picture over their initials", ["avatar.rs", "avatar.css"]),
    piece!("badge", "Badge", "a short label: a status, a count, a tag", ["badge.rs", "badge.css"]),
    piece!("breadcrumb", "Breadcrumb", "typed links to the pages above this one", ["breadcrumb.rs", "breadcrumb.css"]),
    piece!("button", "Button", "the look of a <button> or <a>, in six variants and four sizes", ["button.rs", "button.css"]),
    piece!("card", "Card", "a surface that groups content, with a header, content and footer", ["card.rs", "card.css"]),
    piece!("checkbox", "Checkbox", "a checkbox and its label", ["checkbox.rs", "checkbox.css"]),
    piece!("combobox", "Combobox", "the look of corten-ui's ComboBox: a filtered list under an input", ["combobox.rs", "combobox.css"], styles "ComboBox"),
    piece!("dialog", "Dialog", "a modal <dialog>, opened by command=\"show-modal\" with no script", ["dialog.rs", "dialog.css"]),
    piece!("dropdown_menu", "DropdownMenu", "the look of corten-ui's MenuButton", ["dropdown_menu.rs", "dropdown_menu.css"], styles "MenuButton"),
    piece!("field", "Field", "a label, its control and a hint or the server's error", ["field.rs", "field.css"]),
    piece!("input", "Input", "the look of a text <input>, invalid state included", ["input.rs", "input.css"]),
    piece!(
        "meter",
        "Meter",
        "the look of a <meter>: a measure in a known range, colored by its state",
        ["meter.rs", "meter.css"]
    ),
    piece!("pagination", "Pagination", "typed links to the pages of a list", ["pagination.rs", "pagination.css"]),
    piece!("popover", "Popover", "content over the page beside its button, on popover", ["popover.rs", "popover.css"]),
    piece!("radio_group", "RadioGroup", "a fieldset of radio buttons and their labels", ["radio_group.rs", "radio_group.css"]),
    piece!(
        "select",
        "Select",
        "the look of a <select>, its open list where base-select is supported",
        ["select.rs", "select.css"]
    ),
    piece!("separator", "Separator", "a line between sections, across or upright", ["separator.rs", "separator.css"]),
    piece!("sheet", "Sheet", "a <dialog> that slides in from an edge of the window", ["sheet.rs", "sheet.css"]),
    piece!("skeleton", "Skeleton", "a placeholder's shape while a part loads", ["skeleton.rs", "skeleton.css"]),
    piece!("slider", "Slider", "the look of an <input type=\"range\">", ["slider.rs", "slider.css"]),
    piece!(
        "stack",
        "Stack",
        "the space between a screen's parts: a column (Stack) or a wrapping row (Cluster)",
        ["stack.rs", "stack.css"]
    ),
    piece!("stats", "Stats", "labelled figures for a dashboard's summary, on a <dl>", ["stats.rs", "stats.css"]),
    piece!(
        "status_dot",
        "StatusDot",
        "a dot in a status's color, before the words that name it",
        ["status_dot.rs", "status_dot.css"]
    ),
    piece!("switch", "Switch", "an on and off setting: a checkbox with role=\"switch\"", ["switch.rs", "switch.css"]),
    piece!("table", "Table", "the look of a <table>, scrolling sideways when it is wide", ["table.rs", "table.css"]),
    piece!("tabs", "Tabs", "the look of corten-ui's TabGroup: a segmented tab list", ["tabs.rs", "tabs.css"], styles "TabGroup"),
    piece!("textarea", "Textarea", "the look of a <textarea>", ["textarea.rs", "textarea.css"]),
    piece!("toasts", "Toasts", "the look and place of corten-ui's Toaster", ["toasts.rs", "toasts.css"], styles "Toaster"),
];

/// What `init` writes, from the app's root.
const INIT: &[(&str, &str)] = &[
    ("tokens/ui.tokens.json", include_str!("../kit/tokens/ui.tokens.json")),
    ("tokens/ui-dark.tokens.json", include_str!("../kit/tokens/ui-dark.tokens.json")),
    ("tokens/ui.resolver.json", include_str!("../kit/tokens/ui.resolver.json")),
    ("src/ui.rs", include_str!("../kit/ui.rs")),
];

/// Added to `static/style.css` by `init`: the page's own colors, and the theme form controls and
/// scrollbars follow, which are properties of the page rather than tokens.
const PAGE_CSS: &str = include_str!("../kit/page.css");

const COMPONENTS: &str = "pub const COMPONENTS: &[Component] =";

/// The line in `src/ui.rs` the first `pub mod` goes before.
const TOKENS_LINE: &str = "corten::style::tokens!(";

/// A theme in the app's own code: a `tokens!` call, however it is imported.
const TOKENS_CALL: &str = "tokens!(";

/// `src/ui.rs` for an app with a theme of its own, which the components read: no tokens of the kit.
const OWN_THEME_MODULE: &str = "// corten ui {{version}}
//! The UI kit: the components `corten ui add` copied into `src/ui/`, reading their colors from the
//! app's own theme. They are the app's to change; `corten ui list` names the others.

use corten::Component;

/// Each component with a stylesheet: `App::components(ui::COMPONENTS)`.
pub const COMPONENTS: &[Component] = &[];
";

/// Above `pub mod ui;` in `src/lib.rs`.
const MODULE_DOC: &str = "/// The UI kit (`corten ui`): public, so a component not used yet is not a warning.";

const USAGE: &str = "usage: corten ui init [--kit-theme] | corten ui add <name>.. [--force] | corten ui add --all [--force] | corten ui list | corten ui diff [name]..";

pub fn run(args: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    match args.first().map(String::as_str) {
        Some("init") => init(&root, &args[1..]),
        Some("add") => add(&root, &args[1..]),
        Some("list") => {
            print!("{}", list());
            Ok(())
        }
        Some("diff") => diff(&root, &args[1..]),
        _ => Err(USAGE.to_string()),
    }
}

fn list() -> String {
    let width = KIT.iter().map(|p| p.name.len()).max().unwrap_or(0);
    let mut out = String::new();
    for piece in KIT {
        out.push_str(&format!("  {:width$}  {}\n", piece.name, piece.summary));
    }
    out
}

/// The text a kit file is written with. Its first line says where it came from, the version and
/// a hash of the rest as written (`// corten ui 0.1.0: button 9a3f..`), so `corten ui diff` can tell
/// a copy the app edited from one the kit changed since. Files with no hash place (`src/ui.rs`,
/// the JSON tokens) only get the version.
fn stamped(text: &str) -> String {
    let text = text.replace("{{version}}", env!("CARGO_PKG_VERSION"));
    let Some((first, rest)) = text.split_once('\n') else { return text };
    let hash = format!("{:016x}", fnv(rest));
    if let Some(head) = first.strip_suffix(" */") {
        format!("{head} {hash} */\n{rest}")
    } else if first.starts_with("// corten ui ") && first.contains(": ") {
        format!("{first} {hash}\n{rest}")
    } else {
        text
    }
}

/// FNV-1a, 64 bits, over the text with CRLF read as LF (a checkout may convert line ends): stable
/// across Rust releases, as `std`'s hasher is not.
fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes().filter(|&b| b != b'\r') {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// A copy's first line: the kit version and the hash of the file as it was written.
fn stamp_of(text: &str) -> Option<(String, u64)> {
    let first = text.lines().next()?;
    let rest = first.strip_prefix("// corten ui ").or_else(|| first.strip_prefix("/* corten ui "))?;
    let rest = rest.trim_end_matches(" */");
    let (version, rest) = rest.split_once(": ")?;
    let hash = rest.rsplit_once(' ')?.1;
    Some((version.to_string(), u64::from_str_radix(hash, 16).ok()?))
}

fn init(root: &Path, args: &[String]) -> Result<(), String> {
    let kit_theme = match args {
        [] => false,
        [flag] if flag == "--kit-theme" => true,
        _ => return Err(USAGE.to_string()),
    };
    for (path, _) in INIT {
        if root.join(path).exists() {
            return Err(format!("{path} exists: the kit is already set up here (corten ui add <name> adds a component)"));
        }
    }
    let app = sources(root);
    // An app with colors of its own keeps them, unless it asks for the kit's: the kit's components
    // read their tokens from its theme. One whose theme has no colors (spacing alone) gets the
    // kit's; one whose theme cannot be read is told why, rather than given a second theme.
    let mut theme = None;
    if !kit_theme {
        for (path, text) in app.iter().filter(|(path, text)| !is_kit_file(path, text) && text.contains(TOKENS_CALL)) {
            match own_theme(root, text) {
                Ok(Some(found)) if found.colors => {
                    theme = Some((path.clone(), found));
                    break;
                }
                Ok(_) => {}
                Err(why) => {
                    return Err(format!(
                        "cannot read the theme {path}'s tokens! names ({why}), so the tokens it lacks cannot be named: \
                         fix it and run this again, or run corten ui init --kit-theme to add the kit's theme beside it"
                    ));
                }
            }
        }
    }
    let mut files: Vec<(String, String)> = match &theme {
        None => INIT.iter().map(|(path, text)| (path.to_string(), stamped(text))).collect(),
        Some(_) => vec![("src/ui.rs".to_string(), stamped(OWN_THEME_MODULE))],
    };
    let mut said: Vec<String> = files.iter().map(|(path, _)| format!("  wrote {path}")).collect();
    let mut todo: Vec<String> = Vec::new();
    match wire_app(&app) {
        Ok(edits) => {
            for (path, text) in edits {
                let was = app.iter().find(|(p, _)| *p == path).map_or("", |(_, t)| t.as_str());
                let added: Vec<&str> =
                    text.lines().map(str::trim).filter(|l| is_wiring(l) && !was.lines().any(|w| w.trim() == *l)).collect();
                said.push(format!("  edited {path}: {}", added.join(" and ")));
                files.push((path, text));
            }
        }
        Err(why) => todo.push(why),
    }
    match (&theme, std::fs::read_to_string(root.join("static/style.css"))) {
        (Some(_), _) => {}
        (None, Ok(text)) if text.contains("color-scheme") => todo.push(
            "static/style.css sets color-scheme already, so it is left as it is: give the page the theme's colors \
             (:root { background: var(--background); color: var(--foreground); }) and follow data-theme as kit/page.css does"
                .to_string(),
        ),
        (None, Ok(text)) => {
            let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
            files.push(("static/style.css".into(), format!("{text}{sep}{}", stamped(PAGE_CSS))));
            said.push("  edited static/style.css: the page's colors from the theme".into());
        }
        (None, Err(_)) => {
            todo.push("copy the page colors from corten ui into your page stylesheet (see docs/guide/kit.md)".to_string())
        }
    }
    apply(root, &files)?;
    for line in said {
        println!("{line}");
    }
    for step in &todo {
        println!("  to do: {step}");
    }
    match theme {
        None => println!(
            "\nThe theme is light and dark (data-theme=\"dark\", or the system's). Add components with corten ui add <name>;\ncorten ui list names them."
        ),
        Some((path, theme)) => {
            let missing: Vec<String> = role_tokens().into_iter().filter(|t| !theme.properties.contains(t)).collect();
            let taken: Vec<&String> = missing.iter().filter(|t| theme.groups.contains(t)).collect();
            if missing.is_empty() {
                println!("\nThe components read their colors from your theme ({path}), which has every token they read.");
            } else {
                println!(
                    "\nThe components read their colors from your theme ({path}), which lacks these. Declare each at the top\n\
                     of a token file, most as a reference to one of yours, so it follows your other themes too\n\
                     (\"card\": {{ \"$type\": \"color\", \"$value\": \"{{color.surface}}\" }}):\n  {}\n\
                     The app does not start while a component reads one no stylesheet declares, and says which.",
                    missing.join(", ")
                );
            }
            if !taken.is_empty() {
                println!(
                    "A group of your theme is named {}, so a token cannot take that name there: rename the group,\n\
                     or declare --{} in static/style.css (:root {{ --radius: 0.5rem; }}).",
                    names_of(&taken),
                    taken.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", --")
                );
            }
            println!("To take the kit's theme instead, beside yours: corten ui init --kit-theme.");
            println!("Add components with corten ui add <name>; corten ui list names them.");
        }
    }
    Ok(())
}

/// The tokens the kit's components read: every one of its theme, in the theme's order.
fn role_tokens() -> Vec<String> {
    match crate::json::parse(INIT[0].1) {
        Ok(crate::json::Value::Object(fields)) => {
            fields.into_iter().map(|(name, _)| name).filter(|n| !n.starts_with('$')).collect()
        }
        _ => unreachable!("the kit's theme is a JSON object"),
    }
}

/// What an app's own theme declares, from the files its `tokens!` names (a resolver's sources too).
#[derive(Debug, Default, PartialEq)]
struct Theme {
    /// Each custom property its tokens make, without the `--`: `radius-sm` for `radius.sm`.
    properties: Vec<String>,
    /// The groups at the top of its token files (`radius`), whose names a token there cannot take.
    groups: Vec<String>,
    /// Whether any token is a color.
    colors: bool,
}

/// The theme the `tokens!` call in `code` names. None when `code` has no call with a path in it
/// (one in a comment); an error, naming the file, when a file it reaches cannot be read or is not
/// JSON, so nothing is guessed.
fn own_theme(root: &Path, code: &str) -> Result<Option<Theme>, String> {
    let Some(at) = code.find(TOKENS_CALL).map(|at| at + TOKENS_CALL.len()) else { return Ok(None) };
    let Some(path) = code[at..].trim_start().strip_prefix('"').and_then(|rest| rest.split('"').next()) else {
        return Ok(None);
    };
    let mut theme = Theme::default();
    // Each (file, pointer into it) still to read, and those read.
    let mut parts = vec![(root.join(path), String::new())];
    let mut read: Vec<(PathBuf, String)> = Vec::new();
    while let Some((file, pointer)) = parts.pop() {
        if read.contains(&(file.clone(), pointer.clone())) {
            continue;
        }
        let shown = slash(file.strip_prefix(root).unwrap_or(&file));
        let text = std::fs::read_to_string(&file).map_err(|e| format!("{shown}: {e}"))?;
        let whole = crate::json::parse(&text).map_err(|e| format!("{shown}: {e}"))?;
        let value = part(&whole, &pointer).ok_or_else(|| format!("{shown}#{pointer} names nothing"))?;
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        read.push((file, pointer));
        if is_structure(value) {
            // A resolver, a set or a modifier: the files it names are the theme.
            refs(value, &mut |to| {
                if let Some((file, pointer)) = to.split_once('#').or(Some((to, ""))).filter(|(file, _)| !file.is_empty()) {
                    parts.push((dir.join(file), pointer.to_string()));
                }
            });
        } else {
            tokens(value, &mut Vec::new(), &mut theme);
        }
    }
    Ok(Some(theme))
}

/// A line `init` adds to the app's code: the module, or the list on the `App`.
fn is_wiring(line: &str) -> bool {
    line == "pub mod ui;" || line == ".components(ui::COMPONENTS)" || line == ".components(crate::ui::COMPONENTS)"
}

/// `a`, `a and b`, `a, b and c`.
fn names_of(names: &[&String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ")),
    }
}

/// A token name as its custom property writes it, as `tokens!` does (`slug` in corten-macros'
/// `dtcg.rs`): `Brand Primary` and `brandPrimary` are `brand-primary`, `FONT-SIZE` is `font-size`.
fn css_name(name: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() && previous.is_some_and(|p| p.is_ascii_lowercase()) && !out.ends_with('-') {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        previous = Some(c);
    }
    out.trim_end_matches('-').to_string()
}

/// The value a JSON pointer (`/sets/base`, or empty for the whole) names in `value`.
fn part<'a>(value: &'a crate::json::Value, pointer: &str) -> Option<&'a crate::json::Value> {
    let mut at = value;
    for step in pointer.split('/').skip(1).filter(|s| !s.is_empty()) {
        let step = step.replace("~1", "/").replace("~0", "~");
        at = match at {
            crate::json::Value::Object(fields) => &fields.iter().find(|(k, _)| *k == step)?.1,
            crate::json::Value::Array(items) => items.get(step.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(at)
}

/// A resolver (`resolutionOrder`), set (`sources`) or modifier (`contexts`) rather than tokens.
fn is_structure(value: &crate::json::Value) -> bool {
    matches!(value, crate::json::Value::Object(fields)
        if fields.iter().any(|(k, _)| matches!(k.as_str(), "resolutionOrder" | "sources" | "contexts")))
}

/// Calls `each` with every `$ref` in `value` that names another file, outside `$extensions`.
fn refs(value: &crate::json::Value, each: &mut impl FnMut(&str)) {
    match value {
        crate::json::Value::Object(fields) => {
            for (k, v) in fields {
                match (k.as_str(), v.as_str()) {
                    ("$extensions", _) => {}
                    ("$ref", Some(to)) if !to.starts_with('#') => each(to),
                    _ => refs(v, each),
                }
            }
        }
        crate::json::Value::Array(items) => items.iter().for_each(|v| refs(v, each)),
        _ => {}
    }
}

/// Adds what a token file's group at `path` declares to `theme`: a token's custom property (its
/// path joined by `-`, a group's `$root` the group's own), each group at the top, and whether a
/// `$type` is a color.
fn tokens(value: &crate::json::Value, path: &mut Vec<String>, theme: &mut Theme) {
    let crate::json::Value::Object(fields) = value else { return };
    for (key, v) in fields {
        if key == "$type" && v.as_str() == Some("color") {
            theme.colors = true;
        }
        let crate::json::Value::Object(inner) = v else { continue };
        let token = inner.iter().any(|(k, _)| k == "$value" || k == "$ref");
        if key == "$root" && token && !path.is_empty() {
            theme.properties.push(path.iter().map(|p| css_name(p)).collect::<Vec<_>>().join("-"));
        }
        if key.starts_with('$') {
            continue;
        }
        path.push(key.clone());
        if token {
            theme.properties.push(path.iter().map(|p| css_name(p)).collect::<Vec<_>>().join("-"));
            if inner.iter().any(|(k, t)| k == "$type" && t.as_str() == Some("color")) {
                theme.colors = true;
            }
        } else {
            if path.len() == 1 {
                theme.groups.push(css_name(key));
            }
            tokens(v, path, theme);
        }
        path.pop();
    }
}

/// `pub mod ui;` in the crate's root (`src/lib.rs`, else `src/main.rs`) and `.components(..)`
/// on the one `App::new(..)` the crate has, wherever it is: each changed file. An error says
/// what to do by hand when one cannot be placed; a second run changes nothing.
fn wire_app(app: &[(String, String)]) -> Result<Vec<(String, String)>, String> {
    let by_hand = |why: &str| {
        format!("{why}: declare pub mod ui; in src/lib.rs (or src/main.rs) and add .components(ui::COMPONENTS) to the App")
    };
    let root = ["src/lib.rs", "src/main.rs"]
        .into_iter()
        .find(|p| app.iter().any(|(path, _)| path == p))
        .ok_or_else(|| by_hand("there is no src/lib.rs or src/main.rs"))?;
    let news: Vec<&str> = app
        .iter()
        .filter(|(path, text)| !is_kit_file(path, text) && text.lines().any(|l| l.trim_start().starts_with("App::new(")))
        .map(|(path, _)| path.as_str())
        .collect();
    let at = match news[..] {
        [one] => one,
        [] => return Err(by_hand("no file in src/ has an App::new( line")),
        _ => return Err(by_hand(&format!("{} have App::new( lines", news.join(" and ")))),
    };
    let text_of = |p: &str| app.iter().find(|(path, _)| path == p).map(|(_, t)| t.clone()).unwrap_or_default();
    let mut edits: Vec<(String, String)> = Vec::new();
    let root_text = text_of(root);
    if let Some(text) = declare_ui(&root_text) {
        edits.push((root.to_string(), text));
    } else if !root_text.lines().any(|l| matches!(l.trim(), "mod ui;" | "pub mod ui;")) {
        return Err(by_hand(&format!("{root} has no mod lines to put pub mod ui; among")));
    }
    // A binary's file (`src/main.rs` beside `src/lib.rs`, `src/bin/`) reaches the library by its
    // crate's name, which is the app's to write.
    if root == "src/lib.rs" && (at == "src/main.rs" || at.starts_with("src/bin/")) {
        return Err(by_hand(&format!("{at} builds the App outside the library that declares ui")));
    }
    let path = if at == root { "ui::COMPONENTS" } else { "crate::ui::COMPONENTS" };
    let app_text = edits.iter().find(|(p, _)| p == at).map(|(_, t)| t.clone()).unwrap_or_else(|| text_of(at));
    if !app_text.contains(".components(ui::COMPONENTS)") && !app_text.contains(".components(crate::ui::COMPONENTS)") {
        let text =
            list_on_app(&app_text, path).ok_or_else(|| by_hand(&format!("{at}'s App::new( line goes on past its line")))?;
        match edits.iter_mut().find(|(p, _)| p == at) {
            Some(edit) => edit.1 = text,
            None => edits.push((at.to_string(), text)),
        }
    }
    Ok(edits)
}

/// Writes every file, or none. Each text is first written to a new file next to its own
/// (`.<name>.corten-ui`; refused if the app has one by that name), which touches nothing the app
/// has; only when all are written does each replace its file (`rename`, which replaces an
/// existing file on Windows too), and
/// when one cannot, those replaced before it are put back as they were (a new one removed), so a
/// retry starts where the first try did. A file that cannot be put back is named in the error.
/// Each path is relative to `root`.
fn apply(root: &Path, files: &[(String, String)]) -> Result<(), String> {
    let mut made: Vec<PathBuf> = Vec::new();
    let mut staged: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (path, text) in files {
        let path = root.join(path);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let temp = path.with_file_name(format!(".{name}.corten-ui"));
        if let Err(error) = make_dirs(&path, &mut made) {
            let left = undo(&[], &staged, &made);
            return Err(rolled_back(&error, &left));
        }
        // Only a file this call created is staged, so only it is removed on the way back.
        match stage(&temp, text) {
            Ok(()) => staged.push((path, temp)),
            Err((error, created)) => {
                if created {
                    staged.push((path, temp));
                }
                let left = undo(&[], &staged, &made);
                return Err(rolled_back(&error, &left));
            }
        }
    }
    let mut done: Vec<(PathBuf, Option<Vec<u8>>)> = Vec::new();
    for (path, temp) in &staged {
        let before = std::fs::read(path).ok();
        if let Err(error) = std::fs::rename(temp, path) {
            let left = undo(&done, &staged, &made);
            return Err(rolled_back(&format!("{}: {error}", path.display()), &left));
        }
        done.push((path.clone(), before));
    }
    Ok(())
}

/// Writes `text` to `temp`, a file that must not exist yet. The error says whether the file was
/// created (and so is this call's to remove).
fn stage(temp: &Path, text: &str) -> Result<(), (String, bool)> {
    use std::io::Write;
    let mut out = std::fs::OpenOptions::new().write(true).create_new(true).open(temp).map_err(|e| {
        let why = if e.kind() == std::io::ErrorKind::AlreadyExists {
            "is in the way of the file this command writes beside its own; move it and run it again".to_string()
        } else {
            e.to_string()
        };
        (format!("{}: {why}", temp.display()), false)
    })?;
    out.write_all(text.as_bytes()).and_then(|()| out.sync_all()).map_err(|e| (format!("{}: {e}", temp.display()), true))
}

/// Creates the directories `path` needs, adding each one made to `made`, outermost first.
fn make_dirs(path: &Path, made: &mut Vec<PathBuf>) -> Result<(), String> {
    let Some(parent) = path.parent() else { return Ok(()) };
    let mut missing: Vec<PathBuf> =
        parent.ancestors().take_while(|d| !d.as_os_str().is_empty() && !d.exists()).map(Path::to_path_buf).collect();
    missing.reverse();
    for dir in missing {
        std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        made.push(dir);
    }
    Ok(())
}

/// Puts back each replaced file (`done`, with what it held), removes the staged texts and the
/// directories made for them, and returns the paths it could not restore.
fn undo(done: &[(PathBuf, Option<Vec<u8>>)], staged: &[(PathBuf, PathBuf)], made: &[PathBuf]) -> Vec<PathBuf> {
    let mut left = Vec::new();
    for (path, before) in done.iter().rev() {
        let back = match before {
            Some(bytes) => std::fs::write(path, bytes),
            None => std::fs::remove_file(path),
        };
        if back.is_err() {
            left.push(path.clone());
        }
    }
    for (_, temp) in staged {
        if temp.exists() && std::fs::remove_file(temp).is_err() {
            left.push(temp.clone());
        }
    }
    for dir in made.iter().rev() {
        if dir.exists() && std::fs::remove_dir(dir).is_err() {
            left.push(dir.clone());
        }
    }
    left
}

fn rolled_back(error: &str, left: &[PathBuf]) -> String {
    if left.is_empty() {
        return format!("{error}; nothing was changed");
    }
    let names: Vec<String> = left.iter().map(|p| p.display().to_string()).collect();
    format!("{error}; these could not be put back as they were, so fix them by hand: {}", names.join(", "))
}

/// The file with `pub mod ui;` among its `mod` lines, in name order as rustfmt sorts them. None
/// when it declares it already or has no `mod` line to place it by.
fn declare_ui(text: &str) -> Option<String> {
    if text.lines().any(|l| matches!(l.trim(), "mod ui;" | "pub mod ui;")) {
        return None;
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let last = lines.iter().rposition(|l| is_mod_line(l))?;
    let mut first = last;
    while first > 0 && is_mod_line(&lines[first - 1]) {
        first -= 1;
    }
    let name = |l: &str| l.trim_start_matches("pub ").trim_start_matches("mod ").trim_end_matches(';').to_string();
    let mod_at = (first..=last).find(|&i| name(&lines[i]).as_str() > "ui").unwrap_or(last + 1);
    lines.splice(mod_at..mod_at, [MODULE_DOC.to_string(), "pub mod ui;".to_string()]);
    Some(joined(&lines, text))
}

/// The file with `.components(<list>)` on its one `App::new(..)` line, which ends on that line.
fn list_on_app(text: &str, list: &str) -> Option<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let news: Vec<usize> =
        lines.iter().enumerate().filter(|(_, l)| l.trim_start().starts_with("App::new(")).map(|(i, _)| i).collect();
    let [at] = news[..] else { return None };
    // `App::new(..)` alone on its line, so the method goes on the next one.
    let line = &lines[at];
    if !line.trim_end().ends_with(')') || line.matches('(').count() != line.matches(')').count() {
        return None;
    }
    let indent = line.len() - line.trim_start().len();
    lines.insert(at + 1, format!("{}.components({list})", " ".repeat(indent + 4)));
    Some(joined(&lines, text))
}

/// The lines as one text, ending as `was` ends.
fn joined(lines: &[String], was: &str) -> String {
    let mut out = lines.join("\n");
    if was.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn is_mod_line(line: &str) -> bool {
    let rest = line.strip_prefix("pub ").unwrap_or(line);
    rest.strip_prefix("mod ")
        .is_some_and(|name| name.ends_with(';') && name[..name.len() - 1].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
}

fn add(root: &Path, args: &[String]) -> Result<(), String> {
    let force = args.iter().any(|a| a == "--force");
    let names: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.starts_with("--")).collect();
    let pieces: Vec<&Piece> = if args.iter().any(|a| a == "--all") {
        KIT.iter().collect()
    } else {
        if names.is_empty() {
            return Err(USAGE.to_string());
        }
        let mut found = Vec::new();
        for name in &names {
            let piece =
                KIT.iter().find(|p| p.name == *name).ok_or_else(|| format!("the kit has no {name:?}; it has:\n{}", list()))?;
            if !found.iter().any(|p: &&Piece| p.name == piece.name) {
                found.push(piece);
            }
        }
        found
    };
    let module = std::fs::read_to_string(root.join("src/ui.rs"))
        .map_err(|_| "src/ui.rs is missing: run corten ui init first".to_string())?;
    // The files this writes are the kit's whatever they hold now (a copy whose stamp line was lost).
    let writes: Vec<String> =
        pieces.iter().flat_map(|p| p.files.iter().map(|(file, _)| slash(&PathBuf::from("src/ui").join(file)))).collect();
    let app: Vec<(String, String)> = sources(root).into_iter().filter(|(path, _)| !writes.contains(path)).collect();
    let clash = clashes(&app, &pieces);
    if !clash.is_empty() {
        let mut why = String::from(
            "a component's classes are named after it, for the whole page, so two with one name and a stylesheet each do not link:\n",
        );
        for (path, name, piece) in &clash {
            why.push_str(&format!("  {path} has {name}, as the kit's {piece} does\n"));
        }
        why.push_str(
            "rename the app's component and run this again, or leave the kit's out (corten ui add with the other names)",
        );
        return Err(why);
    }
    // Every file is checked, and every change worked out, before any is written.
    if !force {
        for piece in &pieces {
            for (file, _) in piece.files {
                let path = PathBuf::from("src/ui").join(file);
                if root.join(&path).exists() {
                    return Err(format!("{} exists; corten ui add --force {} replaces it", slash(&path), piece.name));
                }
            }
        }
    }
    let mut files = Vec::new();
    let mut said = Vec::new();
    let mut todo = Vec::new();
    let mut listed = Some(module.clone());
    for piece in &pieces {
        let mut written = Vec::new();
        for (file, text) in piece.files {
            let path = slash(&PathBuf::from("src/ui").join(file));
            files.push((path.clone(), stamped(text)));
            written.push(path);
        }
        said.push(format!("  added {}: {}", piece.name, written.join(", ")));
        listed = listed.and_then(|m| list_in(&m, piece));
    }
    match listed {
        Some(text) if text != module => {
            files.push(("src/ui.rs".into(), text));
            said.push("  edited src/ui.rs".into());
        }
        Some(_) => {}
        None => {
            todo.push("src/ui.rs has no COMPONENTS list to edit; add each component's pub mod and owner to it".to_string());
        }
    }
    // A component that styles a corten-ui widget needs the widget: the dependency, and its
    // components listed, as the app's own are.
    if let Some(widget) = pieces.iter().find_map(|p| p.styles) {
        match std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| e.to_string()).and_then(|t| depend_on_ui(&t)) {
            Ok(Some(text)) => {
                files.push(("Cargo.toml".into(), text));
                said.push("  edited Cargo.toml: corten-ui, the widgets these components style".into());
            }
            Ok(None) => {}
            Err(why) => todo.push(format!(
                "add corten-ui to Cargo.toml's [dependencies] ({why}): corten_ui::{widget} is what the component styles"
            )),
        }
        let listed = app.iter().any(|(path, text)| !is_kit_file(path, text) && text.contains("corten_ui::COMPONENTS"));
        match app.iter().find_map(|(path, text)| Some((path, list_ui_widgets(text)?))) {
            _ if listed => {}
            Some((path, text)) => {
                said.push(format!("  edited {path}: .components(corten_ui::COMPONENTS) on the App"));
                files.push((path.clone(), text));
            }
            None => todo.push("add .components(corten_ui::COMPONENTS) to the App, after .components(ui::COMPONENTS)".to_string()),
        }
    }
    apply(root, &files)?;
    for line in said {
        println!("{line}");
    }
    for step in todo {
        println!("  to do: {step}");
    }
    Ok(())
}

/// `Cargo.toml` with `corten-ui` after `corten` in `[dependencies]`, from the same place, as the
/// two must be one source for their `Component`s to be one type: the `corten-ui` next to a local
/// `corten` (`--corten-path`), the same repository and revision as a Git `corten`, else this
/// command's release, which `corten`'s is too. None when it is there already; an error saying
/// why it cannot be placed.
fn depend_on_ui(text: &str) -> Result<Option<String>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| l.trim() == "[dependencies]").ok_or("it has no [dependencies]")?;
    let end = lines[start + 1..].iter().position(|l| l.trim_start().starts_with('[')).map_or(lines.len(), |i| start + 1 + i);
    let section = &lines[start + 1..end];
    if section.iter().any(|l| l.trim_start().starts_with("corten-ui")) {
        return Ok(None);
    }
    let at = section.iter().position(|l| l.trim_start().starts_with("corten ") || l.trim_start().starts_with("corten="));
    let at = start + 1 + at.ok_or("it has no corten line in [dependencies]")?;
    let fields = dependency_fields(lines[at])?;
    let get = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let dependency = if let Some(path) = get("path") {
        // The value as written, quotes and escapes kept: `corten` is its last segment.
        let (quote, body) = path.split_at(1);
        let parent = body
            .strip_suffix(quote)
            .and_then(|p| p.strip_suffix("corten"))
            .ok_or("corten's path does not end at the corten crate")?;
        format!("corten-ui = {{ path = {quote}{parent}corten-ui{quote} }}")
    } else if let Some(git) = get("git") {
        let mut keys = vec![format!("git = {git}")];
        keys.extend(["branch", "tag", "rev"].into_iter().filter_map(|key| get(key).map(|v| format!("{key} = {v}"))));
        format!("corten-ui = {{ {} }}", keys.join(", "))
    } else if get("workspace").is_some() || get("registry").is_some() {
        return Err("corten comes from the workspace or another registry, so corten-ui must come from the same place".into());
    } else {
        format!("corten-ui = \"{}\"", env!("CARGO_PKG_VERSION"))
    };
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    out.insert(at + 1, dependency);
    let mut text_out = out.join("\n");
    if text.ends_with('\n') {
        text_out.push('\n');
    }
    Ok(Some(text_out))
}

/// The keys of a `corten = ..` line, each with its value as written (a string with its quotes):
/// none for a version alone (`corten = "0.1"`). A comment is not read. An error for what this
/// does not read exactly, a table that goes on past its line among them, so the caller says to
/// add `corten-ui` by hand rather than add the wrong one.
fn dependency_fields(line: &str) -> Result<Vec<(String, String)>, String> {
    let parts = split_outside_quotes(line, '#')?;
    let code = parts[0].trim();
    let value = code.split_once('=').map(|(_, v)| v.trim()).ok_or("corten's line has no value")?;
    if value.starts_with(['"', '\'']) {
        return Ok(Vec::new());
    }
    let inner = value
        .strip_prefix('{')
        .and_then(|v| v.strip_suffix('}'))
        .ok_or("corten's table goes on past its line, or is not an inline table")?;
    let mut fields = Vec::new();
    for part in split_outside_quotes(inner, ',')? {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) =
            part.split_once('=').ok_or_else(|| format!("corten's table has {part:?}, which is not key = value"))?;
        let key = key.trim().trim_matches(['"', '\'']).to_string();
        fields.push((key, value.trim().to_string()));
    }
    Ok(fields)
}

/// `text` split at each `at` outside a string or an array (`[..]`, as `features` is). An error
/// when a string is not closed.
fn split_outside_quotes(text: &str, at: char) -> Result<Vec<&str>, String> {
    let (mut parts, mut start, mut quote, mut depth) = (Vec::new(), 0, None, 0usize);
    let mut chars = text.char_indices();
    while let Some((i, c)) = chars.next() {
        match (quote, c) {
            // A basic string's escape: the next character is part of it.
            (Some('"'), '\\') => {
                chars.next();
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '[') => depth += 1,
            (None, ']') => depth = depth.saturating_sub(1),
            (None, c) if c == at && depth == 0 => {
                parts.push(&text[start..i]);
                start = i + c.len_utf8();
                if at == '#' {
                    // A comment runs to the end of the line.
                    return Ok(parts);
                }
            }
            _ => {}
        }
    }
    if quote.is_some() {
        return Err("corten's line has a string that is not closed".into());
    }
    parts.push(&text[start..]);
    Ok(parts)
}

/// The file with `.components(corten_ui::COMPONENTS)` after its `.components(ui::COMPONENTS)`
/// (or `crate::ui::COMPONENTS`) line; None when it has no such line.
fn list_ui_widgets(text: &str) -> Option<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let at =
        lines.iter().position(|l| matches!(l.trim(), ".components(ui::COMPONENTS)" | ".components(crate::ui::COMPONENTS)"))?;
    let indent = lines[at].len() - lines[at].trim_start().len();
    lines.insert(at + 1, format!("{}.components(corten_ui::COMPONENTS)", " ".repeat(indent)));
    Some(joined(&lines, text))
}

/// For each copied file of the components named (all that `src/ui/` has, by default): whether
/// the app edited it, whether the kit changed it since the copy, and the lines that differ from
/// the kit this command carries. Writes nothing.
fn diff(root: &Path, args: &[String]) -> Result<(), String> {
    let names: Vec<&str> = args.iter().map(String::as_str).filter(|a| !a.starts_with("--")).collect();
    for name in &names {
        if !KIT.iter().any(|p| p.name == *name) {
            return Err(format!("the kit has no {name:?}; it has:\n{}", list()));
        }
    }
    let mut seen = 0;
    for piece in KIT.iter().filter(|p| names.is_empty() || names.contains(&p.name)) {
        for (file, kit) in piece.files {
            let path = PathBuf::from("src/ui").join(file);
            let Ok(copy) = std::fs::read_to_string(root.join(&path)) else {
                if !names.is_empty() {
                    println!("{}: not copied (corten ui add {})", slash(&path), piece.name);
                }
                continue;
            };
            seen += 1;
            let kit = stamped(kit);
            print!("{}", report(&slash(&path), &copy, &kit));
        }
    }
    if seen == 0 && names.is_empty() {
        println!("src/ui/ has no kit components (corten ui add <name>)");
    }
    Ok(())
}

/// What `diff` says about one file: `copy` as the app has it, `kit` as this command writes it.
fn report(path: &str, copy: &str, kit: &str) -> String {
    let lf = |t: &str| t.replace("\r\n", "\n");
    let body = |t: &str| t.split_once('\n').map_or(String::new(), |(_, rest)| lf(rest));
    let version = env!("CARGO_PKG_VERSION");
    let sides = format!("(- this copy, + the kit's {version})");
    let Some((from, hash)) = stamp_of(copy) else {
        // Without the line, all of the copy is the app's: compare it whole.
        return format!(
            "{path}: no corten ui line at its top, so where it came from is unknown {sides}:\n{}",
            unified(&lf(copy), &body(kit))
        );
    };
    let (copy_body, kit_body) = (body(copy), body(kit));
    let edited = fnv(&copy_body) != hash;
    let kit_changed = stamp_of(kit).is_some_and(|(_, kit_hash)| kit_hash != hash);
    let state = match (edited, kit_changed) {
        (false, false) => return format!("{path}: as copied from {from}, the same as the kit\n"),
        (true, false) => format!("edited here; the kit has not changed it since {from} {sides}:"),
        (false, true) => format!("changed in the kit since {from}, not edited here, so corten ui add --force takes it {sides}:"),
        (true, true) => format!("edited here, and changed in the kit since {from}: take the kit's changes by hand {sides}:"),
    };
    format!("{path}: {state}\n{}", unified(&copy_body, &kit_body))
}

/// A unified diff of two texts' lines, from `old` to `new`, with three lines of context.
fn unified(old: &str, new: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    // The longest common subsequence, from the end: kit files are a few hundred lines at most.
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    // Each line: ' ' in both, '-' only in old, '+' only in new.
    let mut ops: Vec<(char, &str)> = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            ops.push((' ', a[i]));
            (i, j) = (i + 1, j + 1);
        } else if i < a.len() && (j == b.len() || lcs[i + 1][j] >= lcs[i][j + 1]) {
            // A removal first, as git writes a replaced line.
            ops.push(('-', a[i]));
            i += 1;
        } else {
            ops.push(('+', b[j]));
            j += 1;
        }
    }
    let changed: Vec<usize> = ops.iter().enumerate().filter(|(_, (op, _))| *op != ' ').map(|(k, _)| k).collect();
    let mut out = String::new();
    let mut k = 0;
    while k < changed.len() {
        // A hunk: the changes within six lines of each other, with three lines around them.
        let start = changed[k].saturating_sub(3);
        let mut end = changed[k];
        while k < changed.len() && changed[k] <= end + 6 {
            end = changed[k];
            k += 1;
        }
        let end = (end + 4).min(ops.len());
        let count = |kind: char| ops[start..end].iter().filter(|(op, _)| *op == ' ' || *op == kind).count();
        // A side's first line; for a side with no lines, the line before the hunk, as diff writes it.
        let line = |kind: char| {
            let before = ops[..start].iter().filter(|(op, _)| *op == ' ' || *op == kind).count();
            before + usize::from(count(kind) > 0)
        };
        out.push_str(&format!("@@ -{},{} +{},{} @@\n", line('-'), count('-'), line('+'), count('+')));
        for (op, text) in &ops[start..end] {
            out.push_str(&format!("{op}{text}\n"));
        }
    }
    out
}

/// `src/ui.rs` with `pub mod <name>;` and `<name>::<Owner>` in `COMPONENTS`, each in name order and laid out as
/// rustfmt lays them out, so `corten check` passes. None when the file is not in the shape `init`
/// wrote.
fn list_in(module: &str, piece: &Piece) -> Option<String> {
    let start = module.find(COMPONENTS)?;
    if start > 0 && !module[..start].ends_with('\n') {
        return None;
    }
    // `= &[` and the items on the line, or one a line.
    let after = &module[start + COMPONENTS.len()..];
    let body_start = start + COMPONENTS.len() + (after.len() - after.trim_start().len()) + "&[".len();
    if !after.trim_start().starts_with("&[") {
        return None;
    }
    let end = body_start + module[body_start..].find("];")?;
    let mut items: Vec<String> =
        module[body_start..end].split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if items.iter().any(|i| !i.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b':')) {
        return None;
    }
    let entry = format!("{}::{}", piece.name, piece.owner);
    if !items.contains(&entry) {
        items.push(entry);
    }
    // The tokens first, as the stylesheet needs them before the rules that read them.
    let (mut first, mut rest): (Vec<String>, Vec<String>) = items.into_iter().partition(|i| !i.contains("::"));
    rest.sort();
    first.append(&mut rest);
    let mut out = String::with_capacity(module.len() + 64);
    out.push_str(&module[..start]);
    out.push_str(&layout(&first));
    out.push_str(&module[end + 2..]);
    // The module declaration, among the others in name order.
    let decl = format!("pub mod {};", piece.name);
    if out.lines().any(|l| l == decl) {
        return Some(out);
    }
    let mut lines: Vec<&str> = out.lines().collect();
    let mods: Vec<usize> =
        lines.iter().enumerate().filter(|(_, l)| l.starts_with("pub mod ") && l.ends_with(';')).map(|(i, _)| i).collect();
    let at = match mods.iter().find(|&&i| lines[i] > decl.as_str()) {
        Some(&i) => i,
        None => match mods.last() {
            Some(&i) => i + 1,
            // The first: before the tokens, apart from them, or before the list and its comment.
            None => {
                let list = lines.iter().position(|l| l.starts_with(COMPONENTS))?;
                let doc = if list > 0 && lines[list - 1].starts_with("///") { list - 1 } else { list };
                let at = lines.iter().position(|l| l.starts_with(TOKENS_LINE)).unwrap_or(doc);
                lines.insert(at, "");
                at
            }
        },
    };
    lines.insert(at, &decl);
    let mut text = lines.join("\n");
    text.push('\n');
    Some(text)
}

/// The list as rustfmt writes it with its default settings: on one line when the array is at most
/// 60 columns (its `array_width`) and the line fits in 100, else one item a line.
fn layout(items: &[String]) -> String {
    let array = format!("&[{}]", items.join(", "));
    let one = format!("{COMPONENTS} {array};");
    if array.len() <= 60 && one.len() <= 100 {
        return one;
    }
    let mut out = format!("{COMPONENTS} &[\n");
    for item in items {
        out.push_str(&format!("    {item},\n"));
    }
    out.push_str("];");
    out
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Every `.rs` file under `src/`, as (its path from the root, with `/`, and its text), in path
/// order.
fn sources(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut dirs = vec![root.join("src")];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && let (Ok(text), Ok(at)) = (std::fs::read_to_string(&path), path.strip_prefix(root))
            {
                out.push((slash(at), text));
            }
        }
    }
    out.sort();
    out
}

/// The kit's files in the app: `src/ui.rs` and each copy `corten ui add` wrote, which starts with
/// its stamp (`// corten ui 0.1.0: button ..`) wherever the app keeps it.
fn is_kit_file(path: &str, text: &str) -> bool {
    path == "src/ui.rs" || text.starts_with("// corten ui ")
}

/// The components a file declares with a stylesheet of their own (`#[component(css)]`, with
/// `js` or not, by any path to the attribute): each styles the classes its name makes and exports a
/// symbol of that name, so two of one name in a program fail to link. A part (`css = Owner`)
/// styles its owner's. Comments (a doc example) are not read.
fn styled_components(text: &str) -> Vec<String> {
    let code = |line: &str| line.split("//").next().unwrap_or("").trim().to_string();
    let mut out = Vec::new();
    let mut lines = text.lines().map(code);
    while let Some(line) = lines.next() {
        let attr = line.strip_prefix("#[").and_then(|l| l.strip_suffix("]")).map(|l| l.trim_start_matches("::"));
        let Some(args) = attr
            .and_then(|a| a.strip_prefix("corten::").or(Some(a)))
            .and_then(|a| a.strip_prefix("component("))
            .and_then(|a| a.strip_suffix(')'))
        else {
            continue;
        };
        if !args.split(',').any(|a| a.trim() == "css") {
            continue;
        }
        // The function after its attributes.
        let Some(item) = lines.find(|l| !l.is_empty() && !l.starts_with("#[")) else { break };
        let mut rest = item.as_str();
        if let Some(after) = rest.strip_prefix("pub") {
            rest = match after.trim_start().strip_prefix('(') {
                Some(scope) => scope.split_once(')').map_or("", |(_, r)| r),
                None => after,
            }
            .trim_start();
        }
        let Some(rest) = rest.strip_prefix("fn ") else { continue };
        let name: String = rest.trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out
}

/// For each component the pieces bring that an app's own component of the same name, with its
/// own stylesheet, would clash with: (the app's file, the name, the kit's piece).
fn clashes(app: &[(String, String)], pieces: &[&Piece]) -> Vec<(String, String, &'static str)> {
    let mine: Vec<(String, &str)> = app
        .iter()
        .filter(|(path, text)| !is_kit_file(path, text))
        .flat_map(|(path, text)| styled_components(text).into_iter().map(move |name| (name, path.as_str())))
        .collect();
    let mut out = Vec::new();
    for piece in pieces {
        for (_, text) in piece.files.iter().filter(|(f, _)| f.ends_with(".rs")) {
            for name in styled_components(text) {
                for (_, path) in mine.iter().filter(|(n, _)| *n == name) {
                    out.push((path.to_string(), name.clone(), piece.name));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        INIT, KIT, OWN_THEME_MODULE, Piece, apply, clashes, depend_on_ui, fnv, list_in, list_ui_widgets, own_theme, report,
        role_tokens, stamp_of, stamped, styled_components, unified, wire_app,
    };

    fn piece(name: &str) -> &'static Piece {
        KIT.iter().find(|p| p.name == name).unwrap()
    }

    #[test]
    fn add_lists_each_component_in_name_order_after_the_tokens() {
        let mut module = stamped(include_str!("../kit/ui.rs"));
        for name in ["card", "alert", "button", "card"] {
            module = list_in(&module, piece(name)).unwrap();
        }
        assert!(
            module
                .contains("use corten::Component;\n\npub mod alert;\npub mod button;\npub mod card;\n\ncorten::style::tokens!("),
            "{module}"
        );
        assert!(
            module.contains("pub const COMPONENTS: &[Component] = &[TOKENS, alert::Alert, button::Button, card::Card];\n"),
            "{module}"
        );
    }

    #[test]
    fn a_long_list_is_laid_out_as_rustfmt_does() {
        let mut module = stamped(include_str!("../kit/ui.rs"));
        for piece in KIT {
            module = list_in(&module, piece).unwrap();
        }
        assert!(module.contains("pub const COMPONENTS: &[Component] = &[\n    TOKENS,\n    accordion::Accordion,\n"), "{module}");
        assert!(module.ends_with("    toasts::Toasts,\n];\n"), "{module}");
    }

    fn app(files: &[(&str, &str)]) -> Vec<(String, String)> {
        files.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect()
    }

    #[test]
    fn init_declares_the_module_in_order_and_lists_it_on_the_app() {
        let lib =
            "mod alpha;\nmod views;\n\npub fn app() -> App {\n    App::new(url::router())\n        .assets(views::Static)\n}\n";
        let edits = wire_app(&app(&[("src/lib.rs", lib)])).unwrap();
        assert_eq!(
            edits,
            [(
                "src/lib.rs".to_string(),
                "mod alpha;\n/// The UI kit (`corten ui`): public, so a component not used yet is not a warning.\npub mod ui;\nmod views;\n\npub fn app() -> App {\n    App::new(url::router())\n        .components(ui::COMPONENTS)\n        .assets(views::Static)\n}\n"
                    .to_string()
            )]
        );
        assert_eq!(wire_app(&app(&[("src/lib.rs", &edits[0].1)])).unwrap(), [], "a second run changes nothing");
    }

    /// An app that builds its `App` in another module (`src/app.rs`) gets the module declared in
    /// its root and the list on that `App`, by its path from there.
    #[test]
    fn init_finds_the_app_where_it_is_built() {
        let main = "mod app;\nmod views;\n\nfn main() {}\n";
        let built = "use corten::App;\n\npub fn app() -> App {\n    App::new(url::router())\n        .components(corten_ui::COMPONENTS)\n}\n";
        let files = app(&[("src/app.rs", built), ("src/main.rs", main)]);
        let edits = wire_app(&files).unwrap();
        assert_eq!(edits.len(), 2, "{edits:?}");
        assert!(edits[0].0 == "src/main.rs" && edits[0].1.contains("mod app;\n/// The UI kit"), "{edits:?}");
        assert!(
            edits[1].0 == "src/app.rs"
                && edits[1].1.contains("        .components(crate::ui::COMPONENTS)\n        .components(corten_ui")
        );
        let again = app(&[("src/app.rs", &edits[1].1), ("src/main.rs", &edits[0].1)]);
        assert_eq!(wire_app(&again).unwrap(), [], "a second run changes nothing");
        let two = app(&[("src/a.rs", built), ("src/b.rs", built), ("src/main.rs", main)]);
        assert!(wire_app(&two).unwrap_err().contains("src/a.rs and src/b.rs have App::new( lines"));
        assert!(wire_app(&app(&[("src/main.rs", main)])).unwrap_err().contains("no file in src/ has an App::new( line"));
        let lib = "mod views;\n";
        let binary = app(&[("src/lib.rs", lib), ("src/main.rs", built)]);
        assert!(wire_app(&binary).unwrap_err().contains("src/main.rs builds the App outside the library"), "by hand");
    }

    /// An app with a theme of its own gets the kit's module without the kit's tokens, and the names
    /// of the tokens the components read; add lists components in it as in the kit's.
    #[test]
    fn an_app_with_its_own_theme_keeps_it() {
        let names = role_tokens();
        for name in ["background", "card", "card-foreground", "success", "warning-foreground", "ring", "radius"] {
            assert!(names.iter().any(|n| n == name), "{names:?}");
        }
        assert_eq!(names.len(), 26, "{names:?}");
        let module = list_in(&stamped(OWN_THEME_MODULE), piece("card")).unwrap();
        let module = list_in(&module, piece("alert")).unwrap();
        assert!(module.contains("use corten::Component;\n\npub mod alert;\npub mod card;\n\n/// Each component"), "{module}");
        assert!(module.contains("pub const COMPONENTS: &[Component] = &[alert::Alert, card::Card];\n"), "{module}");
        assert!(!module.contains("tokens!"));
    }

    /// The app's theme is read from the files its `tokens!` names, a resolver's sources (a part of a
    /// file, `#/pointer`) included: the custom properties its tokens make, as `tokens!` names them,
    /// its top groups, and whether it has colors. A group's name is not a token of that name.
    #[test]
    fn init_reads_the_apps_own_theme() {
        let dir = std::env::temp_dir().join(format!("corten-ui-theme-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("tokens")).unwrap();
        let base = r##"{ "color": { "$type": "color", "ink": { "$value": "#000" }, "brand": { "$root": { "$value": "#00f" } } },
            "card": { "$type": "color", "$value": "{color.ink}" }, "mutedForeground": { "$type": "color", "$value": "{color.ink}" },
            "radius": { "$type": "dimension", "sm": { "$value": { "value": 4, "unit": "px" } } },
            "$extensions": { "run.corten": { "x": { "$ref": "nowhere.json" } } } }"##;
        std::fs::write(dir.join("tokens/base.tokens.json"), base).unwrap();
        std::fs::write(
            dir.join("tokens/dark.tokens.json"),
            r##"{ "theme": { "border": { "$type": "color", "$value": "#111" } } }"##,
        )
        .unwrap();
        std::fs::write(
            dir.join("tokens/theme.resolver.json"),
            r##"{ "sets": { "base": { "sources": [{ "$ref": "base.tokens.json#/" }] } },
                "modifiers": { "theme": { "contexts": { "light": [], "dark": [{ "$ref": "dark.tokens.json#/theme" }] } } },
                "resolutionOrder": [{ "$ref": "#/sets/base" }, { "$ref": "#/modifiers/theme" }] }"##,
        )
        .unwrap();
        std::fs::write(
            dir.join("tokens/space.tokens.json"),
            r##"{ "space": { "$type": "dimension", "1": { "$value": { "value": 4, "unit": "px" } } } }"##,
        )
        .unwrap();
        let theme = own_theme(&dir, "corten::style::tokens!(\"tokens/theme.resolver.json\");").unwrap().unwrap();
        assert!(theme.colors);
        for name in ["color-ink", "color-brand", "card", "muted-foreground", "radius-sm", "border"] {
            assert!(theme.properties.iter().any(|p| p == name), "{name}: {theme:?}");
        }
        assert!(!theme.properties.iter().any(|p| p == "radius" || p == "color"), "a group is not a token: {theme:?}");
        assert_eq!(theme.groups, ["color", "radius"]);
        let space = own_theme(&dir, "tokens!(\"tokens/space.tokens.json\");").unwrap().unwrap();
        assert!(!space.colors && space.properties == ["space-1"], "{space:?}");
        assert_eq!(own_theme(&dir, "// no theme here").unwrap(), None);
        let missing = own_theme(&dir, "tokens!(\"tokens/missing.json\");").unwrap_err();
        assert!(missing.starts_with("tokens/missing.json: "), "{missing}");
        std::fs::write(dir.join("tokens/r.json"), r##"{ "resolutionOrder": [{ "$ref": "base.tokens.json#/nope" }] }"##).unwrap();
        assert!(own_theme(&dir, "tokens!(\"tokens/r.json\");").unwrap_err().contains("base.tokens.json#/nope names nothing"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A kit component and an app's of one name, each with a stylesheet, would style one class
    /// prefix and fail to link: add says which before it writes anything. One without a
    /// stylesheet, a part (`css = Owner`), a doc example and the app's copies of the kit do not count.
    #[test]
    fn add_refuses_a_component_the_app_has_by_that_name() {
        let layout = "use corten::prelude::*;\n\n#[component(css)]\npub fn Field(label: &str) -> impl Render {\n    html! { <div class={Field}></div> }\n}\n\n#[component(js, css)]\n#[allow(unused)]\nfn CardHeader() -> impl Render {}\n";
        assert_eq!(styled_components(layout), ["Field", "CardHeader"]);
        let forms = "#[corten::component(css)]\npub(crate) fn Table() {}\n#[::corten::component(js, css)] // the sort\npub(super) fn Tabs() {}\n#[component(css)]\n/// Docs between.\n#[allow(dead_code)]\nfn Badge() {}\n";
        assert_eq!(styled_components(forms), ["Table", "Tabs", "Badge"]);
        let copy = stamped(piece("field").files[0].1);
        let files = app(&[("src/layout.rs", layout), ("src/views/ui/field.rs", &copy), ("src/ui/mine.rs", layout)]);
        let found = clashes(&files, &[piece("field"), piece("card"), piece("button")]);
        assert_eq!(
            found,
            [
                ("src/layout.rs".to_string(), "Field".to_string(), "field"),
                ("src/ui/mine.rs".to_string(), "Field".to_string(), "field")
            ],
            "a copy of the kit's is not the app's, wherever it is; an app's own in src/ui/ is; CardHeader is a part of Card"
        );
        let plain = "#[component]\npub fn Field() -> impl Render {}\n/// #[component(css)]\n/// fn Button() -> impl Render {}\n#[component(css = Panel)]\nfn Card() {}\n";
        assert_eq!(clashes(&app(&[("src/a.rs", plain)]), &[piece("field"), piece("button"), piece("card")]), []);
    }

    #[test]
    fn a_widget_component_brings_corten_ui_from_where_corten_comes() {
        let local = "[package]\nname = \"a\"\n\n[dependencies]\ncorten = { path = \"D:\\\\src\\\\crates\\\\corten\", features = [\"compress\"] }\nserde = \"1\"\n";
        let text = depend_on_ui(local).unwrap().unwrap();
        assert!(text.contains("compress\"] }\ncorten-ui = { path = \"D:\\\\src\\\\crates\\\\corten-ui\" }\nserde"), "{text}");
        assert_eq!(depend_on_ui(&text).unwrap(), None, "a second run changes nothing");
        let release = "[dependencies]\ncorten = { version = \"0.1.0\", features = [\"compress\"] }\n\n[dev-dependencies]\ntower = \"0.5\"\n";
        let text = depend_on_ui(release).unwrap().unwrap();
        assert!(text.contains(&format!("corten-ui = \"{}\"\n\n[dev-dependencies]", env!("CARGO_PKG_VERSION"))), "{text}");
        assert!(depend_on_ui("[dependencies]\nserde = \"1\"\n").is_err());
        let git = "[dependencies]\ncorten = { git = \"https://github.com/todd-skelton/corten\", rev = \"abc123\", features = [\"compress\"] }\n";
        let text = depend_on_ui(git).unwrap().unwrap();
        assert!(
            text.ends_with("}\ncorten-ui = { git = \"https://github.com/todd-skelton/corten\", rev = \"abc123\" }\n"),
            "a Git corten brings corten-ui from the same revision: {text}"
        );
        let branch = "[dependencies]\ncorten = { git = \"https://example.com/c\", branch = \"main\" }\n";
        assert!(
            depend_on_ui(branch).unwrap().unwrap().contains("corten-ui = { git = \"https://example.com/c\", branch = \"main\" }")
        );
        assert!(depend_on_ui("[dependencies]\ncorten = { workspace = true }\n").is_err());
        let quoted = "[dependencies]\ncorten = { git = 'https://example.com/c', tag = 'v0.2' } # path = \"nope\"\n";
        assert!(
            depend_on_ui(quoted).unwrap().unwrap().contains("\ncorten-ui = { git = 'https://example.com/c', tag = 'v0.2' }\n"),
            "single quotes, and a comment is not read"
        );
        let commented = "[dependencies]\ncorten = \"0.1\" # { git = \"https://example.com/c\" }\n";
        assert!(depend_on_ui(commented).unwrap().unwrap().contains(&format!("corten-ui = \"{}\"", env!("CARGO_PKG_VERSION"))));
        // What this does not read exactly is the app's to write, never a guess.
        let spans = "[dependencies]\ncorten = { git = \"https://example.com/c\",\n    rev = \"abc\" }\n";
        assert!(depend_on_ui(spans).is_err(), "a table that goes on past its line");
        assert!(depend_on_ui("[dependencies]\n\n[dependencies.corten]\ngit = \"https://example.com/c\"\n").is_err());
        let features = "[dependencies]\ncorten = { path = \"../corten\", features = [\"a\", \"b\"] }\n";
        assert!(depend_on_ui(features).unwrap().unwrap().contains("corten-ui = { path = \"../corten-ui\" }"));
        let lib = "    App::new(url::router())\n        .components(ui::COMPONENTS)\n        .assets(views::Static)\n";
        let text = list_ui_widgets(lib).unwrap();
        assert!(text.contains(".components(ui::COMPONENTS)\n        .components(corten_ui::COMPONENTS)\n"), "{text}");
        let elsewhere = list_ui_widgets("    App::new(url::router())\n        .components(crate::ui::COMPONENTS)\n").unwrap();
        assert!(
            elsewhere.ends_with(".components(crate::ui::COMPONENTS)\n        .components(corten_ui::COMPONENTS)\n"),
            "{elsewhere}"
        );
        assert_eq!(list_ui_widgets("fn app() {}\n"), None);
    }

    #[test]
    fn a_write_replaces_a_file_that_is_there_and_leaves_nothing_beside_it() {
        let dir = std::env::temp_dir().join(format!("corten-ui-replace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "old").unwrap();
        apply(&dir, &[("a.txt".to_string(), "new".to_string()), ("b/c.txt".to_string(), "made".to_string())]).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(dir.join("b/c.txt")).unwrap(), "made");
        let mut names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["a.txt", "b"]);
        // A file of the app's where one is staged is refused, and kept as it was.
        std::fs::write(dir.join(".a.txt.corten-ui"), "the app's").unwrap();
        let error = apply(&dir, &[("a.txt".to_string(), "newer".to_string())]).unwrap_err();
        assert!(error.contains("is in the way") && error.contains("nothing was changed"), "{error}");
        assert_eq!(std::fs::read_to_string(dir.join(".a.txt.corten-ui")).unwrap(), "the app's");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "new");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_write_that_fails_puts_back_what_the_command_wrote_before_it() {
        let dir = std::env::temp_dir().join(format!("corten-ui-apply-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("taken")).unwrap();
        std::fs::write(dir.join("old.txt"), "old").unwrap();
        // A directory where the last file goes: that write fails.
        let files = [
            ("old.txt".to_string(), "new".to_string()),
            ("made.txt".to_string(), "x".to_string()),
            ("taken".to_string(), "y".to_string()),
        ];
        let error = apply(&dir, &files).unwrap_err();
        assert!(error.contains("nothing was changed"), "{error}");
        assert_eq!(std::fs::read_to_string(dir.join("old.txt")).unwrap(), "old");
        assert!(!dir.join("made.txt").exists());
        assert!(dir.join("taken").is_dir());
        // A file that cannot be staged (its directory is a file): nothing is replaced, and the
        // directory made for an earlier one is gone.
        let files = [("new/a.txt".to_string(), "x".to_string()), ("old.txt/b".to_string(), "y".to_string())];
        let error = apply(&dir, &files).unwrap_err();
        assert!(error.contains("nothing was changed"), "{error}");
        assert!(!dir.join("new").exists());
        let mut names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["old.txt", "taken"], "no staged text is left behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_property_a_component_reads_is_a_token_of_the_theme() {
        let tokens = |path: &str| INIT.iter().find(|(p, _)| *p == path).unwrap().1;
        let (light, dark) = (tokens("tokens/ui.tokens.json"), tokens("tokens/ui-dark.tokens.json"));
        for piece in KIT {
            for (file, text) in piece.files.iter().filter(|(f, _)| f.ends_with(".css")) {
                for read in text.split("var(--").skip(1) {
                    let name = &read[..read.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap()];
                    // One the stylesheet sets itself (a badge's `--tone`) is not the theme's.
                    if text.contains(&format!("--{name}:")) {
                        continue;
                    }
                    assert!(light.contains(&format!("\n  \"{name}\": ")), "{file} reads --{name}, which the theme lacks");
                    if name != "radius" {
                        assert!(dark.contains(&format!("\n  \"{name}\": ")), "{file} reads --{name}, which the dark theme lacks");
                    }
                }
            }
        }
    }

    /// The theme's text on its backgrounds meets WCAG AA in both themes, as the guide says: each
    /// `-foreground` on its color (4.5:1), a tone's text on a card (an alert's), the muted text on
    /// the page and a card, and a field's border on the page (3:1, a control's boundary).
    #[test]
    fn the_themes_text_meets_wcag_aa() {
        let tokens = |path: &str| INIT.iter().find(|(p, _)| *p == path).unwrap().1;
        for file in ["tokens/ui.tokens.json", "tokens/ui-dark.tokens.json"] {
            let text = tokens(file);
            let hex = |name: &str| {
                let line = text
                    .lines()
                    .find(|l| l.starts_with(&format!("  \"{name}\": ")))
                    .unwrap_or_else(|| panic!("{file} has no {name}"));
                let at = line.find("\"hex\": \"#").unwrap() + "\"hex\": \"#".len();
                u32::from_str_radix(&line[at..at + 6], 16).unwrap()
            };
            let luminance = |rgb: u32| {
                let channel = |c: u32| {
                    let c = f64::from(c & 255) / 255.0;
                    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
                };
                0.2126 * channel(rgb >> 16) + 0.7152 * channel(rgb >> 8) + 0.0722 * channel(rgb)
            };
            let ratio = |a: &str, b: &str| {
                let (x, y) = (luminance(hex(a)), luminance(hex(b)));
                (x.max(y) + 0.05) / (x.min(y) + 0.05)
            };
            let mut pairs: Vec<(String, String, f64)> = Vec::new();
            for line in text.lines() {
                if let Some(name) =
                    line.trim_start().strip_prefix('"').and_then(|l| l.split_once("-foreground\"")).map(|(n, _)| n)
                {
                    pairs.push((format!("{name}-foreground"), name.to_string(), 4.5));
                }
            }
            for tone in ["info", "success", "warning", "destructive"] {
                pairs.push((tone.into(), "card".into(), 4.5));
            }
            pairs.push(("muted-foreground".into(), "background".into(), 4.5));
            pairs.push(("muted-foreground".into(), "card".into(), 4.5));
            pairs.push(("input".into(), "background".into(), 3.0));
            assert!(pairs.len() > 12, "{file}: {pairs:?}");
            for (text_color, on, least) in pairs {
                let r = ratio(&text_color, &on);
                assert!(r >= least, "{file}: {text_color} on {on} is {r:.2}:1, under {least}:1");
            }
            // A badge's text, in each tone (`--tone` in badge.css) and emphasis: solid is the tone's
            // `-foreground` pair above; soft is 85% of the tone in the foreground on 12% of it in the
            // card, mixed in sRGB as `color-mix(in srgb, ..)` mixes; outline is the tone on the page
            // and on a card.
            let channels = |rgb: u32| [(rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255].map(|c| f64::from(c) / 255.0);
            let linear = |c: f64| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
            let lum = |c: [f64; 3]| 0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2]);
            let contrast = |a: [f64; 3], b: [f64; 3]| {
                let (x, y) = (lum(a), lum(b));
                (x.max(y) + 0.05) / (x.min(y) + 0.05)
            };
            let badge = include_str!("../kit/badge.css");
            assert!(
                badge.contains(".soft { background: color-mix(in srgb, var(--tone) 12%, var(--card)); color: color-mix(in srgb, var(--tone) 85%, var(--foreground)); }"),
                "the soft badge's mix is the one checked here"
            );
            for tone in ["neutral", "primary", "info", "success", "warning", "destructive"] {
                let rule = badge.lines().find(|l| l.starts_with(&format!(".{tone} {{ --tone: var(--"))).expect(tone);
                let color = &rule[rule.find("var(--").unwrap() + 6..rule.find(");").unwrap()];
                let (ink, card, fg) = (channels(hex(color)), channels(hex("card")), channels(hex("foreground")));
                let soft = [0, 1, 2].map(|i| ink[i] * 0.12 + card[i] * 0.88);
                let soft_text = [0, 1, 2].map(|i| ink[i] * 0.85 + fg[i] * 0.15);
                for (name, r) in [
                    ("a soft badge", contrast(soft_text, soft)),
                    ("the page", contrast(ink, channels(hex("background")))),
                    ("a card", contrast(ink, card)),
                ] {
                    assert!(r >= 4.5, "{file}: a {tone} badge's text on {name} is {r:.2}:1, under 4.5:1");
                }
            }
        }
    }

    /// A meter's fill, in each state's color, stands out from its track by 3:1 in both themes, the
    /// contrast a part that shows a value needs (WCAG 1.4.11), and each engine's rule for each state
    /// fills with that state's color.
    #[test]
    fn a_meters_fill_stands_out_from_its_track() {
        let meter = include_str!("../kit/meter.css");
        let rule =
            |start: &str| meter.lines().find(|l| l.starts_with(start)).unwrap_or_else(|| panic!("meter.css has no {start}"));
        assert!(rule(".meter::-webkit-meter-bar {").ends_with("background: var(--border); }"), "the track");
        let block = &meter[meter.find(".meter {").unwrap()..];
        assert!(block[..block.find('}').unwrap()].contains("background: var(--border);"), "the track Firefox draws");
        let states = [
            (".meter::-webkit-meter-optimum-value {", "primary"),
            (".meter:is([low], [high])::-webkit-meter-optimum-value {", "success"),
            (".meter::-webkit-meter-suboptimum-value {", "warning"),
            (".meter::-webkit-meter-even-less-good-value {", "destructive"),
            (".meter::-moz-meter-bar {", "primary"),
            (".meter:is([low], [high]):-moz-meter-optimum::-moz-meter-bar {", "success"),
            (".meter:-moz-meter-sub-optimum::-moz-meter-bar {", "warning"),
            (".meter:-moz-meter-sub-sub-optimum::-moz-meter-bar {", "destructive"),
        ];
        for (state, fill) in states {
            assert!(rule(state).ends_with(&format!("background: var(--{fill}); }}")), "{state} fills with {fill}");
        }
        let tokens = |path: &str| INIT.iter().find(|(p, _)| *p == path).unwrap().1;
        for file in ["tokens/ui.tokens.json", "tokens/ui-dark.tokens.json"] {
            let text = tokens(file);
            let luminance = |name: &str| {
                let line = text.lines().find(|l| l.starts_with(&format!("  \"{name}\": "))).unwrap();
                let at = line.find("\"hex\": \"#").unwrap() + "\"hex\": \"#".len();
                let rgb = u32::from_str_radix(&line[at..at + 6], 16).unwrap();
                let channel = |c: u32| {
                    let c = f64::from(c & 255) / 255.0;
                    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
                };
                0.2126 * channel(rgb >> 16) + 0.7152 * channel(rgb >> 8) + 0.0722 * channel(rgb)
            };
            for (_, fill) in states {
                let (x, y) = (luminance(fill), luminance("border"));
                let r = (x.max(y) + 0.05) / (x.min(y) + 0.05);
                assert!(r >= 3.0, "{file}: a {fill} meter on its track is {r:.2}:1, under 3:1");
            }
        }
    }

    /// A stack's children take its width whatever the app's element rules say (a new app's
    /// `form { align-items: end }`), and a figure is the same surface as a card.
    #[test]
    fn a_stack_stretches_and_a_figure_is_a_card() {
        let block = |css: &'static str, start: &str| {
            let at = css.find(start).unwrap_or_else(|| panic!("no {start}"));
            let rest = &css[at + start.len()..];
            rest[..rest.find('}').unwrap()].to_string()
        };
        assert!(block(include_str!("../kit/stack.css"), ".stack {").contains("align-items: stretch;"));
        let (stat, card) =
            (block(include_str!("../kit/stats.css"), ".stat {"), block(include_str!("../kit/card.css"), ".card {"));
        let declarations =
            |b: &str| -> Vec<String> { b.split(';').map(|d| d.trim().to_string()).filter(|d| !d.is_empty()).collect() };
        for property in ["background:", "color:", "border:", "border-radius:", "box-shadow:"] {
            let of = |b: &str| declarations(b).into_iter().find(|d| d.starts_with(property));
            assert_eq!(of(&stat), of(&card), "a figure's {property} is a card's");
        }
    }

    /// `corten check` in an app runs `corten fmt --check`, so each kit file is laid out as
    /// `corten fmt` lays it out at rustfmt's default width. (rustfmt's own layout is checked by the
    /// `corten new` check, which formats an app with every component.)
    #[test]
    fn every_kit_file_is_laid_out_as_corten_fmt_does() {
        for piece in KIT {
            for (file, text) in piece.files.iter().filter(|(f, _)| f.ends_with(".rs")) {
                let (formatted, warnings) = crate::fmt::format_source(text, 100);
                assert!(warnings.is_empty(), "{file}: {warnings:?}");
                assert!(formatted == *text, "{file} is not laid out as corten fmt does");
            }
        }
    }

    #[test]
    fn a_copy_says_where_it_came_from_and_what_it_was() {
        let text = stamped(include_str!("../kit/button.rs"));
        let first = text.lines().next().unwrap();
        let (version, hash) = stamp_of(&text).unwrap();
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
        assert!(first.starts_with(&format!("// corten ui {version}: button ")), "{first}");
        assert_eq!(hash, fnv(text.split_once('\n').unwrap().1));
        let css = stamped(include_str!("../kit/button.css"));
        assert!(css.lines().next().unwrap().ends_with(&format!(" {hash:016x} */", hash = stamp_of(&css).unwrap().1)));
        // A checkout that turned the line ends into CRLF has not edited it.
        assert_eq!(fnv("a\r\nb\r\n"), fnv("a\nb\n"));
    }

    #[test]
    fn diff_tells_the_apps_edits_from_the_kits_changes() {
        let kit = stamped("// corten ui {{version}}: thing\none\ntwo\nthree\n");
        let old_kit = stamped("// corten ui {{version}}: thing\none\n2\nthree\n").replace(env!("CARGO_PKG_VERSION"), "0.0.9");
        assert!(report("x.rs", &kit, &kit).ends_with("the same as the kit\n"));
        let edited = kit.replace("three", "THREE");
        let said = report("x.rs", &edited, &kit);
        assert!(said.contains("edited here; the kit has not changed it since"), "{said}");
        assert!(said.contains("-THREE\n+three\n"), "{said}");
        let said = report("x.rs", &old_kit, &kit);
        assert!(said.contains("changed in the kit since 0.0.9, not edited here"), "{said}");
        assert!(said.contains("-2\n+two\n"), "{said}");
        let both = old_kit.replace("one", "ONE");
        assert!(report("x.rs", &both, &kit).contains("edited here, and changed in the kit since 0.0.9"));
        let said = report("x.rs", "no stamp\nline\n", &kit);
        assert!(said.contains("where it came from is unknown"), "{said}");
        assert!(said.contains("-no stamp\n"), "an unstamped copy is compared whole: {said}");
    }

    #[test]
    fn unified_shows_each_change_with_its_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\n";
        let new = old.replace("b\n", "B\n").replace("m\n", "");
        assert_eq!(unified(old, &new), "@@ -1,5 +1,5 @@\n a\n-b\n+B\n c\n d\n e\n@@ -10,5 +10,4 @@\n j\n k\n l\n-m\n n\n");
        assert_eq!(unified("x\n", "x\n"), "");
        assert_eq!(unified("", "a\n"), "@@ -0,0 +1,1 @@\n+a\n", "an empty side starts at line 0");
        assert_eq!(unified("a\n", ""), "@@ -1,1 +0,0 @@\n-a\n");
    }
}
