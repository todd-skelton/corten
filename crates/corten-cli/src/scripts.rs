//! `corten check`'s type check of the app's scripts: TypeScript's `tsc`, in JavaScript mode, over
//! every `.js` file in `src/` and `static/`, against the DOM's types, the `corten` runtime's
//! JSDoc and the app's generated `types/app.d.ts` (feed rows, `corten/events`, `corten/views`).
//!
//! A handler's call is checked when the crate compiles; this checks its body. TypeScript is not
//! a dependency of an app: the check uses `tsc` from the app's `node_modules` or the `PATH`,
//! and says so when there is none instead of passing silently.
//!
//! At a workspace root it checks each member that depends on `corten`, with that member's own
//! `types/`, as `cargo test` there tests each of them.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use crate::json::{self, Value};

/// Directories that hold no app scripts.
const IGNORED: &[&str] = &["target", "node_modules", ".git"];

/// A workspace member that depends on `corten`, in or under the directory `corten check` runs in.
#[derive(Debug, PartialEq)]
struct App {
    name: String,
    root: PathBuf,
}

/// Type-checks the scripts of every app `corten check` covers from `dir`: the app in `dir`, or at
/// a workspace root each member that depends on `corten`, as `cargo test` covers each of them.
pub fn check(dir: &Path) -> Result<(), String> {
    let metadata = json::parse(&cargo_metadata(dir)?).map_err(|e| format!("cargo metadata: {e}"))?;
    let apps: Vec<(App, Vec<PathBuf>)> = apps(&metadata, dir)
        .into_iter()
        .filter_map(|app| {
            let mut scripts = Vec::new();
            for sub in ["src", "static"] {
                collect(&app.root.join(sub), &mut scripts);
            }
            (!scripts.is_empty()).then_some((app, scripts))
        })
        .collect();
    if apps.is_empty() {
        return Ok(());
    }
    let mut homes: Vec<&Path> = apps.iter().map(|(app, _)| app.root.as_path()).collect();
    homes.push(dir);
    let Some(tsc) = find_tsc(&homes) else {
        let count: usize = apps.iter().map(|(_, scripts)| scripts.len()).sum();
        let whose = if apps.len() == 1 {
            format!("the app's {count} script(s)")
        } else {
            let names: Vec<&str> = apps.iter().map(|(app, _)| app.name.as_str()).collect();
            format!("the {count} script(s) of {}", names.join(", "))
        };
        eprintln!(
            "corten: {whose} were not type-checked: TypeScript's tsc is not installed. \
             Install it (npm install --global typescript, or in the app's package.json) and corten check runs it."
        );
        return Ok(());
    };
    let runtime = corten_js_dir(&metadata).ok_or("could not find the corten crate's js/ directory in cargo metadata")?;
    let target = metadata.get("target_directory").and_then(Value::as_str).ok_or("cargo metadata has no target_directory")?;
    let mut failed = Vec::new();
    for (app, _) in &apps {
        let dir = PathBuf::from(target).join("corten").join(&app.name);
        std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        let config = dir.join("jsconfig.json");
        std::fs::write(&config, jsconfig(&app.root, &runtime)).map_err(|e| format!("write {}: {e}", config.display()))?;
        if apps.len() > 1 {
            eprintln!("corten: type-checking {}'s scripts", app.name);
        }
        let status = Command::new(&tsc)
            .arg("-p")
            .arg(&config)
            .current_dir(&app.root)
            .status()
            .map_err(|e| format!("could not run {}: {e}", tsc.display()))?;
        if !status.success() {
            failed.push(app.name.as_str());
        }
    }
    match failed[..] {
        [] => Ok(()),
        _ if apps.len() == 1 => Err("the app's scripts do not type-check (tsc, above)".into()),
        _ => Err(format!("the scripts of {} do not type-check (tsc, above)", failed.join(", "))),
    }
}

/// The workspace members whose directory is `dir` or under it and that depend on `corten`, in
/// cargo's order.
fn apps(metadata: &Value, dir: &Path) -> Vec<App> {
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let dir = canonical(dir);
    let members: Vec<&str> =
        metadata.get("workspace_members").map_or(&[][..], Value::items).iter().filter_map(Value::as_str).collect();
    let packages = metadata.get("packages").map_or(&[][..], Value::items);
    packages
        .iter()
        .filter(|p| p.get("id").and_then(Value::as_str).is_some_and(|id| members.contains(&id)))
        .filter(|p| {
            p.get("dependencies")
                .map_or(&[][..], Value::items)
                .iter()
                .any(|d| d.get("name").and_then(Value::as_str) == Some("corten"))
        })
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?.to_string();
            let root = Path::new(p.get("manifest_path")?.as_str()?).parent()?;
            canonical(root).starts_with(&dir).then(|| App { name, root: root.to_path_buf() })
        })
        .collect()
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

/// `tsc` from the `node_modules` of the first of `homes` that has one, else from the `PATH`
/// (`tsc.cmd` on Windows).
fn find_tsc(homes: &[&Path]) -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["tsc.cmd", "tsc.exe", "tsc"] } else { &["tsc"] };
    let local = homes.iter().flat_map(|home| names.iter().map(move |n| home.join("node_modules").join(".bin").join(n)));
    let candidates = local.chain(names.iter().map(PathBuf::from));
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
fn corten_js_dir(metadata: &Value) -> Option<PathBuf> {
    metadata.get("packages")?.items().iter().find_map(|package| {
        let dir = Path::new(package.get("manifest_path")?.as_str()?).parent()?;
        let name = dir.file_name()?.to_str()?;
        let named = name == "corten" || name.strip_prefix("corten-").is_some_and(|v| v.starts_with(|c: char| c.is_ascii_digit()));
        let js = dir.join("js");
        (named && js.join("corten.js").is_file()).then_some(js)
    })
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
        let find = |dirs: &[&str]| corten_js_dir(&json::parse(&metadata(dirs)).unwrap());
        let found = find(&["registry/corten-macros-0.1.0", "registry/corten-0.1.0"]).unwrap();
        assert!(found.ends_with("corten-0.1.0/js"), "{found:?}");
        let found = find(&["checkout/crates/corten"]).unwrap();
        assert!(found.ends_with("crates/corten/js"), "{found:?}");
        assert_eq!(find(&["registry/corten-macros-0.1.0"]), None);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_workspace_root_checks_each_member_that_uses_corten() {
        let base = std::env::temp_dir().join(format!("corten-check-members-{}", std::process::id()));
        for dir in ["ws/admin", "ws/market", "ws/tools", "registry/corten-0.1.0"] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
        }
        let package = |name: &str, dir: &str, deps: &str| {
            let manifest = json_string(&base.join(dir).join("Cargo.toml").to_string_lossy());
            let deps: Vec<String> = deps.split_whitespace().map(|d| format!(r#"{{"name":"{d}","kind":null}}"#)).collect();
            format!(r#"{{"name":"{name}","id":"{name}-id","manifest_path":{manifest},"dependencies":[{}]}}"#, deps.join(","))
        };
        let metadata = format!(
            r#"{{"packages":[{},{},{},{}],"workspace_members":["admin-id","market-id","tools-id"],"target_directory":"/t"}}"#,
            package("admin", "ws/admin", "corten serde"),
            package("market", "ws/market", "corten"),
            package("tools", "ws/tools", "serde"),
            package("corten", "registry/corten-0.1.0", "corten-macros"),
        );
        let metadata = json::parse(&metadata).unwrap();
        let names = |dir: &str| apps(&metadata, &base.join(dir)).into_iter().map(|app| app.name).collect::<Vec<_>>();
        assert_eq!(names("ws"), ["admin", "market"]);
        assert_eq!(names("ws/market"), ["market"]);
        assert!(names("ws/tools").is_empty());
        let admin = apps(&metadata, &base.join("ws/admin")).remove(0);
        assert_eq!(admin.root, base.join("ws/admin"), "the root is the manifest's directory, as cargo wrote it");
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
