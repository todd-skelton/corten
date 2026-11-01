//! `corten new`: writes the starter app from `template/`, embedded in the binary.

use std::path::{Path, PathBuf};

use crate::option;

const FILES: &[(&str, &str)] = &[
    ("Cargo.toml", include_str!("../template/Cargo.toml.txt")),
    ("src/lib.rs", include_str!("../template/src/lib.rs")),
    ("src/main.rs", include_str!("../template/src/main.rs")),
    ("src/views.rs", include_str!("../template/src/views.rs")),
    ("static/style.css", include_str!("../template/static/style.css")),
    ("static/favicon.svg", include_str!("../template/static/favicon.svg")),
    ("src/click_counter.js", include_str!("../template/src/click_counter.js")),
    ("src/click_counter.css", include_str!("../template/src/click_counter.css")),
    ("AGENTS.md", include_str!("../template/AGENTS.md")),
    ("README.md", include_str!("../template/README.md")),
    ("Dockerfile", include_str!("../template/Dockerfile")),
    (".gitignore", include_str!("../template/gitignore.txt")),
    (".dockerignore", include_str!("../template/dockerignore.txt")),
];

pub fn run(args: &[String]) -> Result<(), String> {
    let dir = args.first().filter(|a| !a.starts_with('-')).ok_or("usage: corten new <dir>")?;
    let dir = PathBuf::from(dir);
    let name = match option(args, "--name")? {
        Some(name) => name.to_string(),
        None => crate_name(&dir)?,
    };
    check_name(&name)?;
    let dependency = dependency(option(args, "--corten-path")?)?;
    if dir.exists() && std::fs::read_dir(&dir).map_err(|e| e.to_string())?.next().is_some() {
        return Err(format!("{} exists and is not empty", dir.display()));
    }
    for (path, text) in FILES {
        let text =
            text.replace("{{name}}", &name).replace("{{crate}}", &name.replace('-', "_")).replace("{{corten}}", &dependency);
        write(&dir.join(path), &text)?;
    }
    println!("Created {name} in {}.\n\n  cd {}\n  corten dev\n\nthen open http://localhost:3000.", dir.display(), dir.display());
    Ok(())
}

/// The app is a library its `main.rs` names (`my_app::app()`), so its name with `-` as `_` must
/// be a path that reaches it: not a keyword, a crate the compiler provides, or one of the
/// template's dependencies.
fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name.starts_with(|c: char| c.is_ascii_lowercase())
        || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(format!(
            "{name:?} is not a crate name: lowercase letters, digits, - and _, starting with a letter (use --name)"
        ));
    }
    const KEYWORDS: &[&str] = &[
        "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate", "do", "dyn", "else", "enum",
        "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "macro", "match", "mod", "move",
        "mut", "override", "priv", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type",
        "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
    ];
    const TAKEN: &[&str] = &["std", "core", "alloc", "test", "proc_macro", "corten", "serde", "tokio", "tower", "http_body_util"];
    let path = name.replace('-', "_");
    if KEYWORDS.contains(&path.as_str()) {
        return Err(format!("{name:?} is a Rust keyword, and the app's main.rs names it as a path (use --name)"));
    }
    if TAKEN.contains(&path.as_str()) {
        return Err(format!(
            "{name:?} is the name of a crate the app already uses, and its main.rs names the app by it (use --name)"
        ));
    }
    Ok(())
}

/// The app's `corten` dependency: the release this command was built with (the crates share one
/// version), or a local checkout with `--corten-path`: the corten crate, or a checkout's root,
/// whose `crates/corten` it is. Anything else is refused before a file is written.
fn dependency(path: Option<&str>) -> Result<String, String> {
    match path {
        Some(given) => {
            let path = std::fs::canonicalize(given).map_err(|e| format!("--corten-path {given}: {e}"))?;
            let is_corten = |dir: &Path| {
                std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|toml| package_name(&toml).as_deref() == Some("corten"))
            };
            let path = if is_corten(&path) {
                path
            } else if is_corten(&path.join("crates").join("corten")) {
                path.join("crates").join("corten")
            } else {
                return Err(format!(
                    "--corten-path {given}: not the corten crate (its Cargo.toml names another package or none) nor a checkout with crates/corten"
                ));
            };
            Ok(format!("path = {:?}", path.to_string_lossy().trim_start_matches(r"\\?\")))
        }
        None => Ok(format!("version = {:?}", env!("CARGO_PKG_VERSION"))),
    }
}

/// The `name` in a manifest's `[package]` table, if it has one.
fn package_name(toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in toml.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[package]";
        } else if let Some(value) =
            line.strip_prefix("name").map(str::trim_start).and_then(|l| l.strip_prefix('=')).filter(|_| in_package)
        {
            return Some(value.split('#').next().unwrap_or("").trim().trim_matches('"').to_string());
        }
    }
    None
}

fn crate_name(dir: &Path) -> Result<String, String> {
    let base = dir.file_name().and_then(|n| n.to_str()).ok_or("the directory needs a name")?;
    Ok(base.to_ascii_lowercase().replace(' ', "-"))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{check_name, dependency};

    #[test]
    fn a_name_the_app_cannot_be_reached_by_is_refused() {
        for name in ["shop", "demo-app", "a_b-c", "views", "main", "app2"] {
            assert_eq!(check_name(name), Ok(()), "{name}");
        }
        for name in [
            "",
            "2app",
            "My-App",
            "fn",
            "self",
            "async",
            "test",
            "std",
            "core",
            "tokio",
            "serde",
            "tower",
            "corten",
            "http-body-util",
        ] {
            assert!(check_name(name).unwrap_err().ends_with("(use --name)"), "{name}");
        }
    }

    #[test]
    fn a_new_app_depends_on_the_release_the_command_came_from() {
        assert_eq!(dependency(None).unwrap(), format!("version = \"{}\"", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn corten_path_points_the_app_at_a_local_checkout() {
        let corten = std::fs::canonicalize(concat!(env!("CARGO_MANIFEST_DIR"), "/../corten")).unwrap();
        let expected = format!("path = {:?}", corten.to_string_lossy().trim_start_matches(r"\\?\"));
        assert_eq!(dependency(Some(concat!(env!("CARGO_MANIFEST_DIR"), "/../corten"))).unwrap(), expected);
        // A checkout's root is a workspace: its crates/corten is the crate.
        assert_eq!(dependency(Some(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).unwrap(), expected);
        let other = dependency(Some(env!("CARGO_MANIFEST_DIR"))).unwrap_err();
        assert!(other.contains("not the corten crate"), "{other}");
        assert!(dependency(Some("no/such/dir")).unwrap_err().starts_with("--corten-path no/such/dir: "));
    }
}
