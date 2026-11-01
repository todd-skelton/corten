//! `corten ui`: the UI kit. `init` writes the theme tokens and `src/ui.rs`, `add` copies a
//! component's source into `src/ui/`, where it is the app's to change. The files are embedded in the
//! binary (`kit/`), so this needs no network and the command stays standard-library only.

use std::path::{Path, PathBuf};

/// A component of the kit: its files, and the component `src/ui.rs` lists in `COMPONENTS` (the one
/// that owns the stylesheet).
struct Piece {
    name: &'static str,
    summary: &'static str,
    owner: &'static str,
    files: &'static [(&'static str, &'static str)],
}

macro_rules! piece {
    ($name:literal, $owner:literal, $summary:literal, [$($file:literal),+]) => {
        Piece { name: $name, owner: $owner, summary: $summary, files: &[$(($file, include_str!(concat!("../kit/", $file)))),+] }
    };
}

const KIT: &[Piece] = &[
    piece!("alert", "Alert", "a message the page calls out, neutral or destructive", ["alert.rs", "alert.css"]),
    piece!("badge", "Badge", "a short label: a status, a count, a tag", ["badge.rs", "badge.css"]),
    piece!("button", "Button", "the look of a <button> or <a>, in six variants and four sizes", ["button.rs", "button.css"]),
    piece!("card", "Card", "a surface that groups content, with a header, content and footer", ["card.rs", "card.css"]),
    piece!("input", "Input", "the look of a text <input>, invalid state included", ["input.rs", "input.css"]),
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

/// Above `pub mod ui;` in `src/lib.rs`.
const MODULE_DOC: &str = "/// The UI kit (`corten ui`): public, so a component not used yet is not a warning.";

const USAGE: &str = "usage: corten ui init | corten ui add <name>.. [--force] | corten ui add --all [--force] | corten ui list";

pub fn run(args: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    match args.first().map(String::as_str) {
        Some("init") => init(&root),
        Some("add") => add(&root, &args[1..]),
        Some("list") => {
            print!("{}", list());
            Ok(())
        }
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

/// The text a kit file is written with: the version it came from, for `corten ui diff`.
fn stamped(text: &str) -> String {
    text.replace("{{version}}", env!("CARGO_PKG_VERSION"))
}

fn init(root: &Path) -> Result<(), String> {
    for (path, _) in INIT {
        if root.join(path).exists() {
            return Err(format!("{path} exists: the kit is already set up here (corten ui add <name> adds a component)"));
        }
    }
    for (path, text) in INIT {
        write(&root.join(path), &stamped(text))?;
        println!("  wrote {path}");
    }
    let mut todo = Vec::new();
    match wire_lib(&root.join("src/lib.rs"))? {
        true => println!("  edited src/lib.rs: pub mod ui; and .components(ui::COMPONENTS) on the App"),
        false => todo.push("declare pub mod ui; in src/lib.rs and add .components(ui::COMPONENTS) to the App"),
    }
    let style = root.join("static/style.css");
    match std::fs::read_to_string(&style) {
        Ok(text) => {
            let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
            write(&style, &format!("{text}{sep}{}", stamped(PAGE_CSS)))?;
            println!("  edited static/style.css: the page's colors from the theme");
        }
        Err(_) => todo.push("copy the page colors from corten ui into your page stylesheet (see docs/guide/kit.md)"),
    }
    for step in &todo {
        println!("  to do: {step}");
    }
    println!(
        "\nThe theme is light and dark (data-theme=\"dark\", or the system's). Add components with corten ui add <name>;\ncorten ui list names them."
    );
    Ok(())
}

/// Declares `pub mod ui;` among the `mod` lines and lists its components on the `App::new(..)` chain.
/// False, changing nothing, when the file does not have one of each to place them by.
fn wire_lib(path: &Path) -> Result<bool, String> {
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(false) };
    if text.lines().any(|l| matches!(l.trim(), "mod ui;" | "pub mod ui;")) {
        return Ok(false);
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let Some(last) = lines.iter().rposition(|l| is_mod_line(l)) else { return Ok(false) };
    // Among the declarations around the last one, in name order, as rustfmt sorts them.
    let mut first = last;
    while first > 0 && is_mod_line(&lines[first - 1]) {
        first -= 1;
    }
    let name = |l: &str| l.trim_start_matches("pub ").trim_start_matches("mod ").trim_end_matches(';').to_string();
    let mod_at = (first..=last).find(|&i| name(&lines[i]).as_str() > "ui").unwrap_or(last + 1);
    let news: Vec<usize> =
        lines.iter().enumerate().filter(|(_, l)| l.trim_start().starts_with("App::new(")).map(|(i, _)| i).collect();
    let [at] = news[..] else { return Ok(false) };
    // `App::new(..)` alone on its line, so the method goes on the next one.
    let line = &lines[at];
    if !line.trim_end().ends_with(')') || line.matches('(').count() != line.matches(')').count() {
        return Ok(false);
    }
    let indent = line.len() - line.trim_start().len();
    let components = format!("{}.components(ui::COMPONENTS)", " ".repeat(indent + 4));
    let module = [MODULE_DOC.to_string(), "pub mod ui;".to_string()];
    // The later line first, so the earlier insertion does not move it.
    if mod_at > at {
        lines.splice(mod_at..mod_at, module);
        lines.insert(at + 1, components);
    } else {
        lines.insert(at + 1, components);
        lines.splice(mod_at..mod_at, module);
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    write(path, &out)?;
    Ok(true)
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
    let ui = root.join("src/ui.rs");
    let module = std::fs::read_to_string(&ui).map_err(|_| "src/ui.rs is missing: run corten ui init first".to_string())?;
    // Every file is checked before any is written, so a refusal leaves the app as it was.
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
    let mut listed = Some(module.clone());
    for piece in &pieces {
        let mut written = Vec::new();
        for (file, text) in piece.files {
            let path = PathBuf::from("src/ui").join(file);
            write(&root.join(&path), &stamped(text))?;
            written.push(slash(&path));
        }
        println!("  added {}: {}", piece.name, written.join(", "));
        listed = listed.and_then(|m| list_in(&m, piece));
    }
    match listed {
        Some(text) if text != module => {
            write(&ui, &text)?;
            println!("  edited src/ui.rs");
        }
        Some(_) => {}
        None => {
            println!("  src/ui.rs has no COMPONENTS list to edit; add these to it:");
            for piece in &pieces {
                println!("    pub mod {};   and {}::{} in COMPONENTS", piece.name, piece.name, piece.owner);
            }
        }
    }
    Ok(())
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
            // The first: before the tokens, apart from them.
            None => {
                let at = lines.iter().position(|l| l.starts_with(TOKENS_LINE))?;
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

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{INIT, KIT, Piece, list_in, stamped, wire_lib};

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
        assert!(module.contains("pub const COMPONENTS: &[Component] = &[\n    TOKENS,\n    alert::Alert,\n"), "{module}");
        assert!(module.ends_with("    input::Input,\n];\n"), "{module}");
    }

    #[test]
    fn init_declares_the_module_in_order_and_lists_it_on_the_app() {
        let dir = std::env::temp_dir().join(format!("corten-ui-wire-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lib = dir.join("lib.rs");
        std::fs::write(
            &lib,
            "mod alpha;\nmod views;\n\npub fn app() -> App {\n    App::new(url::router())\n        .assets(views::Static)\n}\n",
        )
        .unwrap();
        assert!(wire_lib(&lib).unwrap());
        let text = std::fs::read_to_string(&lib).unwrap();
        assert_eq!(
            text,
            "mod alpha;\n/// The UI kit (`corten ui`): public, so a component not used yet is not a warning.\npub mod ui;\nmod views;\n\npub fn app() -> App {\n    App::new(url::router())\n        .components(ui::COMPONENTS)\n        .assets(views::Static)\n}\n"
        );
        assert!(!wire_lib(&lib).unwrap(), "a second run changes nothing");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// What `App::router` checks when the app starts, here without compiling one: each `var(--x)`
    /// a kit stylesheet reads is a token of the kit's theme, in both of its files.
    #[test]
    fn every_property_a_component_reads_is_a_token_of_the_theme() {
        let tokens = |path: &str| INIT.iter().find(|(p, _)| *p == path).unwrap().1;
        let (light, dark) = (tokens("tokens/ui.tokens.json"), tokens("tokens/ui-dark.tokens.json"));
        for piece in KIT {
            for (file, text) in piece.files.iter().filter(|(f, _)| f.ends_with(".css")) {
                for read in text.split("var(--").skip(1) {
                    let name = &read[..read.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap()];
                    assert!(light.contains(&format!("\n  \"{name}\": ")), "{file} reads --{name}, which the theme lacks");
                    if name != "radius" {
                        assert!(dark.contains(&format!("\n  \"{name}\": ")), "{file} reads --{name}, which the dark theme lacks");
                    }
                }
            }
        }
    }
}
