//! `corten new`: writes the starter app from `template/`, embedded in the binary.

use std::path::{Path, PathBuf};

use crate::option;

const FILES: &[(&str, &str)] = &[
    ("Cargo.toml", include_str!("../template/Cargo.toml.txt")),
    ("src/main.rs", include_str!("../template/src/main.rs")),
    ("src/views.rs", include_str!("../template/src/views.rs")),
    ("static/style.css", include_str!("../template/static/style.css")),
    ("src/click_counter.js", include_str!("../template/src/click_counter.js")),
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
    if name.is_empty()
        || !name.starts_with(|c: char| c.is_ascii_lowercase())
        || !name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(format!(
            "{name:?} is not a crate name: lowercase letters, digits, - and _, starting with a letter (use --name)"
        ));
    }
    let dependency = match option(args, "--corten-path")? {
        Some(path) => {
            let path = std::fs::canonicalize(path).map_err(|e| format!("--corten-path {path}: {e}"))?;
            format!("path = {:?}", path.to_string_lossy().trim_start_matches(r"\\?\"))
        }
        None => r#"git = "https://github.com/todd-skelton/corten""#.to_string(),
    };
    if dir.exists() && std::fs::read_dir(&dir).map_err(|e| e.to_string())?.next().is_some() {
        return Err(format!("{} exists and is not empty", dir.display()));
    }
    for (path, text) in FILES {
        let text = text.replace("{{name}}", &name).replace("{{corten}}", &dependency);
        write(&dir.join(path), &text)?;
    }
    println!("Created {name} in {}.\n\n  cd {}\n  corten dev\n\nthen open http://localhost:3000.", dir.display(), dir.display());
    Ok(())
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
