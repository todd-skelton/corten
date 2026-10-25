//! `corten check`'s type check of the app's scripts: TypeScript's `tsc`, in JavaScript mode, over
//! every `.js` file in `src/` and `static/`, against the DOM's types, the `corten` runtime's
//! JSDoc and the app's generated `types/app.d.ts` (feed rows, `corten/events`, `corten/views`).
//!
//! A handler's call is checked when the crate compiles; this checks its body. TypeScript is not
//! a dependency of an app: the check uses `tsc` from the app's `node_modules` or the `PATH`,
//! and says so when there is none instead of passing silently.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::dev::json_string_field;

/// Directories that hold no app scripts.
const IGNORED: &[&str] = &["target", "node_modules", ".git"];

pub fn check(root: &Path) -> Result<(), String> {
    let mut scripts = Vec::new();
    for dir in ["src", "static"] {
        collect(&root.join(dir), &mut scripts);
    }
    if scripts.is_empty() {
        return Ok(());
    }
    let Some(tsc) = find_tsc(root) else {
        eprintln!(
            "corten: the app's {} script(s) were not type-checked: TypeScript's tsc is not installed. \
             Install it (npm install --global typescript, or in the app's package.json) and corten check runs it.",
            scripts.len()
        );
        return Ok(());
    };
    let metadata = cargo_metadata(root)?;
    let runtime = corten_js_dir(&metadata).ok_or("could not find the corten crate's js/ directory in cargo metadata")?;
    let target = json_string_field(&metadata, "target_directory").ok_or("cargo metadata has no target_directory")?;
    let dir = PathBuf::from(target).join("corten");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let config = dir.join("jsconfig.json");
    std::fs::write(&config, jsconfig(root, &runtime)).map_err(|e| format!("write {}: {e}", config.display()))?;
    let status = Command::new(&tsc)
        .arg("-p")
        .arg(&config)
        .current_dir(root)
        .status()
        .map_err(|e| format!("could not run {}: {e}", tsc.display()))?;
    if status.success() { Ok(()) } else { Err("the app's scripts do not type-check (tsc, above)".into()) }
}

/// Every `.js` file under `dir`.
fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if !IGNORED.contains(&name.to_string_lossy().as_ref()) {
                collect(&path, files);
            }
        } else if path.extension().is_some_and(|e| e == "js") {
            files.push(path);
        }
    }
}

/// `tsc` from the app's `node_modules`, else from the `PATH` (`tsc.cmd` on Windows).
fn find_tsc(root: &Path) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["tsc.cmd", "tsc.exe", "tsc"] } else { &["tsc"] };
    let local = root.join("node_modules").join(".bin");
    let candidates = names.iter().map(|n| local.join(n)).chain(names.iter().map(PathBuf::from));
    candidates.into_iter().find(|tsc| Command::new(tsc).arg("--version").output().is_ok_and(|o| o.status.success()))
}

fn cargo_metadata(root: &Path) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err("cargo metadata failed".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The `js/` directory of the `corten` crate the app builds against: the package whose manifest
/// is in a directory named `corten` (a checkout) or `corten-{version}` (crates.io) and has
/// `js/corten.js`.
fn corten_js_dir(metadata: &str) -> Option<PathBuf> {
    manifest_paths(metadata).into_iter().find_map(|manifest| {
        let dir = Path::new(&manifest).parent()?;
        let name = dir.file_name()?.to_str()?;
        let named = name == "corten" || name.strip_prefix("corten-").is_some_and(|v| v.starts_with(|c: char| c.is_ascii_digit()));
        let js = dir.join("js");
        (named && js.join("corten.js").is_file()).then_some(js)
    })
}

/// Every `"manifest_path"` value in cargo metadata's output.
fn manifest_paths(metadata: &str) -> Vec<String> {
    const KEY: &str = r#""manifest_path":""#;
    let mut paths = Vec::new();
    let mut rest = metadata;
    while let Some(at) = rest.find(KEY) {
        if let Some(path) = json_string_field(&rest[at..], "manifest_path") {
            paths.push(path);
        }
        rest = &rest[at + KEY.len()..];
    }
    paths
}

/// The config `tsc` runs with: the same settings Corten checks its own scripts with.
fn jsconfig(root: &Path, runtime: &Path) -> String {
    let path = |p: PathBuf| json_string(&p.to_string_lossy().replace('\\', "/"));
    let include = [
        path(root.join("src").join("**").join("*.js")),
        path(root.join("static").join("**").join("*.js")),
        path(root.join("types").join("*.d.ts")),
        path(runtime.join("events.d.ts")),
        path(runtime.join("views.d.ts")),
    ]
    .join(", ");
    format!(
        r#"{{
  "//": "Written by corten check, which type-checks the app's scripts with it. Do not edit.",
  "compilerOptions": {{
    "checkJs": true,
    "allowJs": true,
    "noEmit": true,
    "strict": true,
    "target": "es2022",
    "module": "esnext",
    "moduleResolution": "bundler",
    "lib": ["es2022", "dom", "dom.iterable"],
    "types": [],
    "paths": {{ "corten": [{runtime}] }}
  }},
  "include": [{include}]
}}
"#,
        runtime = path(runtime.join("corten.js"))
    )
}

/// `text` as a JSON string.
fn json_string(text: &str) -> String {
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_corten_crate_from_a_checkout_or_crates_io() {
        let base = std::env::temp_dir().join(format!("corten-check-scripts-{}", std::process::id()));
        for dir in ["registry/corten-0.1.0/js", "registry/corten-macros-0.1.0/js", "checkout/crates/corten/js"] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
            std::fs::write(base.join(dir).join("corten.js"), "").unwrap();
        }
        let manifest = |dir: &str| json_string(&base.join(dir).join("Cargo.toml").to_string_lossy());
        let metadata = |dirs: &[&str]| {
            let packages: Vec<String> =
                dirs.iter().map(|d| format!(r#"{{"name":"x","manifest_path":{}}}"#, manifest(d))).collect();
            format!(r#"{{"packages":[{}],"target_directory":"/t"}}"#, packages.join(","))
        };
        let found = corten_js_dir(&metadata(&["registry/corten-macros-0.1.0", "registry/corten-0.1.0"])).unwrap();
        assert!(found.ends_with("corten-0.1.0/js"), "{found:?}");
        let found = corten_js_dir(&metadata(&["checkout/crates/corten"])).unwrap();
        assert!(found.ends_with("crates/corten/js"), "{found:?}");
        assert_eq!(corten_js_dir(&metadata(&["registry/corten-macros-0.1.0"])), None);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_config_maps_corten_and_includes_the_app_and_its_types() {
        let config = jsconfig(Path::new("/apps/shop"), Path::new("/reg/corten-0.1.0/js"));
        assert!(config.contains(r#""paths": { "corten": ["/reg/corten-0.1.0/js/corten.js"] }"#), "{config}");
        for part in
            ["/apps/shop/src/**/*.js", "/apps/shop/static/**/*.js", "/apps/shop/types/*.d.ts", "/reg/corten-0.1.0/js/views.d.ts"]
        {
            assert!(config.contains(&format!("\"{part}\"")), "{part} in {config}");
        }
    }
}
