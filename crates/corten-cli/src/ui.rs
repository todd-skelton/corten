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
    piece!("alert", "Alert", "a message the page calls out, neutral or destructive", ["alert.rs", "alert.css"]),
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

/// Above `pub mod ui;` in `src/lib.rs`.
const MODULE_DOC: &str = "/// The UI kit (`corten ui`): public, so a component not used yet is not a warning.";

const USAGE: &str = "usage: corten ui init | corten ui add <name>.. [--force] | corten ui add --all [--force] | corten ui list | corten ui diff [name]..";

pub fn run(args: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    match args.first().map(String::as_str) {
        Some("init") => init(&root),
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
        if let Some(widget) = piece.styles {
            println!("    it styles corten_ui::{widget}: the app depends on corten-ui and lists corten_ui::COMPONENTS");
        }
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
        let line = |kind: char| 1 + ops[..start].iter().filter(|(op, _)| *op == ' ' || *op == kind).count();
        let count = |kind: char| ops[start..end].iter().filter(|(op, _)| *op == ' ' || *op == kind).count();
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
    use super::{INIT, KIT, Piece, fnv, list_in, report, stamp_of, stamped, unified, wire_lib};

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
    }
}
