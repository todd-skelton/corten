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
/// version), or a local checkout with `--corten-path`: the corten crate, or the root of the
/// workspace it is in. Anything else is refused before a file is written.
fn dependency(path: Option<&str>) -> Result<String, String> {
    match path {
        Some(given) => {
            let path = std::fs::canonicalize(given).map_err(|e| format!("--corten-path {given}: {e}"))?;
            let corten = corten_crate(&path).map_err(|e| format!("--corten-path {given}: {e}"))?;
            Ok(format!("path = {:?}", corten.to_string_lossy().trim_start_matches(r"\\?\")))
        }
        None => Ok(format!("version = {:?}", env!("CARGO_PKG_VERSION"))),
    }
}

/// The directory of the `corten` package that `dir` is, or whose workspace root `dir` is, read
/// by `cargo metadata` as `corten check` reads an app's packages. Its directory is named
/// `corten`, since `corten ui add` finds corten-ui beside it.
fn corten_crate(dir: &Path) -> Result<PathBuf, String> {
    let manifest = dir.join("Cargo.toml");
    if !manifest.is_file() {
        return Err("has no Cargo.toml".into());
    }
    let output = std::process::Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1", "--manifest-path"])
        .arg(&manifest)
        .output()
        .map_err(|e| format!("could not run cargo metadata: {e}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo metadata could not read it: {}", error.trim()));
    }
    let metadata = crate::json::parse(&String::from_utf8_lossy(&output.stdout)).map_err(|e| format!("cargo metadata: {e}"))?;
    let canonical = |p: &str| std::fs::canonicalize(p).ok();
    let root = metadata.get("workspace_root").and_then(|v| v.as_str()).and_then(canonical);
    let found = metadata.get("packages").map_or(&[][..], |p| p.items()).iter().find_map(|package| {
        (package.get("name")?.as_str()? == "corten")
            .then(|| canonical(package.get("manifest_path")?.as_str()?)?.parent().map(Path::to_path_buf))?
    });
    match found {
        Some(corten) if corten == dir || root.as_deref() == Some(dir) => match corten.file_name() {
            Some(name) if name == "corten" => Ok(corten),
            _ => Err(format!(
                "its corten crate is in {}, and corten ui add finds corten-ui beside a directory named corten",
                corten.display()
            )),
        },
        _ => Err("not the corten crate nor the root of a workspace that has it".into()),
    }
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
        // A workspace like a checkout, read as cargo reads it: a literal-string name and a
        // commented table header are a manifest like any other.
        let base = std::env::temp_dir().join(format!("corten-new-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let write = |path: &str, text: &str| {
            let path = base.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("ws/Cargo.toml", "[workspace]\nmembers = [\"crates/corten\", \"crates/other\"]\nresolver = \"2\"\n");
        write(
            "ws/crates/corten/Cargo.toml",
            "[ package ] # the framework\nname = 'corten'\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        );
        write("ws/crates/corten/src/lib.rs", "");
        write("ws/crates/other/Cargo.toml", "[package]\nname = \"other\"\nversion = \"0.0.0\"\nedition = \"2021\"\n");
        write("ws/crates/other/src/lib.rs", "");
        write("fork/Cargo.toml", "[workspace]\n\n[package]\nname = \"corten\"\nversion = \"0.0.0\"\nedition = \"2021\"\n");
        write("fork/src/lib.rs", "");
        std::fs::create_dir_all(base.join("empty")).unwrap();
        let at = |p: &str| base.join(p).to_string_lossy().into_owned();
        let corten = std::fs::canonicalize(base.join("ws/crates/corten")).unwrap();
        let expected = format!("path = {:?}", corten.to_string_lossy().trim_start_matches(r"\\?\"));
        assert_eq!(dependency(Some(&at("ws/crates/corten"))).unwrap(), expected);
        assert_eq!(dependency(Some(&at("ws"))).unwrap(), expected, "a checkout's root: its corten crate");
        let other = dependency(Some(&at("ws/crates/other"))).unwrap_err();
        assert!(other.contains("not the corten crate"), "another member is not its workspace's root: {other}");
        assert!(dependency(Some(&at("empty"))).unwrap_err().ends_with("has no Cargo.toml"));
        let fork = dependency(Some(&at("fork"))).unwrap_err();
        assert!(fork.contains("a directory named corten"), "{fork}");
        assert!(dependency(Some("no/such/dir")).unwrap_err().starts_with("--corten-path no/such/dir: "));
        let _ = std::fs::remove_dir_all(&base);
    }
}
