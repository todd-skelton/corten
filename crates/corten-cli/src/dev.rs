//! `corten dev`: build, run, and rebuild and restart on every change.
//!
//! Changes are found by polling modification times (no dependencies, works in containers and
//! on network drives), in the app's directory and in every local package its build reads (path
//! dependencies, a workspace's shared crates), taken from `cargo metadata` after each build. A failed build leaves the running server alone. The binary runs from a
//! copy, so the next build can replace the original while it runs (Windows locks running
//! executables). The app gets `CORTEN_DEV=1`: Corten then serves `/corten/dev`, and every open
//! page reloads when it reconnects to a server that started later.
//!
//! The app's static files are compiled in (`embed!`), but under `corten dev` the app reads them
//! from disk and writes where (`CORTEN_DEV_ASSETS` names the file), so a change to those files
//! alone restarts the last build instead of compiling again.

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, SystemTime},
};

use crate::option;

/// Directories that never trigger a rebuild.
const IGNORED: &[&str] = &["target", ".git", "node_modules", ".idea", ".vscode"];

pub fn run(args: &[String]) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    if !root.join("Cargo.toml").is_file() {
        return Err("run corten dev in the app's directory (where Cargo.toml is)".into());
    }
    let port =
        option(args, "--port")?.map(str::to_string).or_else(|| std::env::var("PORT").ok()).unwrap_or_else(|| "3000".into());
    let mut server: Option<Child> = None;
    let mut roots = vec![root.clone()];
    let mut seen = snapshot(&roots);
    let dev_dir = root.join("target").join("corten-dev");
    let assets_list = dev_dir.join("static-dirs");
    // The copy last started, and whether the change since needs no build.
    let mut running: Option<PathBuf> = None;
    let mut restart_only = false;
    // A failed build is tried again at the next change, whatever it is.
    let mut failed = false;
    loop {
        if let (true, false, Some(copy)) = (restart_only, failed, &running) {
            stop(&mut server);
            server = Some(start(copy, &port, &root, &assets_list)?);
        } else {
            let built = build(&root);
            // A dependency added or removed since the last build changes what is watched. Files of a
            // newly watched package count from now, so the change that triggered this build is not
            // seen twice.
            if let Some(packages) = local_packages(&root) {
                let next = watch_roots(&root, packages);
                for added in next.iter().filter(|r| !roots.contains(r)) {
                    walk(added, &mut seen);
                }
                roots = next;
            }
            match built {
                Ok(binary) => {
                    stop(&mut server);
                    let copy = dev_dir.join(binary.file_name().unwrap_or_default());
                    std::fs::create_dir_all(&dev_dir).map_err(|e| e.to_string())?;
                    std::fs::copy(&binary, &copy).map_err(|e| format!("copy {}: {e}", binary.display()))?;
                    server = Some(start(&copy, &port, &root, &assets_list)?);
                    running = Some(copy);
                    failed = false;
                }
                Err(message) => {
                    eprintln!("corten: {message}; the last good build keeps running");
                    failed = true;
                }
            }
        }
        // Wait for a change, settling for a moment so a save of several files is one rebuild.
        loop {
            std::thread::sleep(Duration::from_millis(300));
            if let Some(child) = server.as_mut()
                && let Ok(Some(status)) = child.try_wait()
            {
                eprintln!("corten: the app exited ({status}); waiting for a change");
                server = None;
            }
            let now = snapshot(&roots);
            if now != seen {
                std::thread::sleep(Duration::from_millis(150));
                let now = snapshot(&roots);
                let dirs = std::fs::read_to_string(&assets_list).unwrap_or_default();
                restart_only = only_under(&seen, &now, &dirs.lines().map(PathBuf::from).collect::<Vec<_>>());
                seen = now;
                eprintln!(
                    "corten: change detected, {}",
                    if restart_only && !failed { "restarting (static files only)" } else { "rebuilding" }
                );
                break;
            }
        }
    }
}

/// Starts the app's copy in dev mode; it writes its static directory to `assets_list`.
fn start(copy: &Path, port: &str, root: &Path, assets_list: &Path) -> Result<Child, String> {
    let _ = std::fs::remove_file(assets_list);
    Command::new(copy)
        .env("CORTEN_DEV", "1")
        .env("CORTEN_DEV_ASSETS", assets_list)
        .env("PORT", port)
        .current_dir(root)
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", copy.display()))
}

fn stop(server: &mut Option<Child>) {
    if let Some(mut old) = server.take() {
        let _ = old.kill();
        let _ = old.wait();
    }
}

/// True when something changed between `before` and `after`, every changed file is under one of
/// `dirs`, and none was removed: the app reads those itself in dev, so a restart serves them. A
/// removed file builds, so a page that still names it does not compile.
fn only_under(before: &BTreeMap<PathBuf, SystemTime>, after: &BTreeMap<PathBuf, SystemTime>, dirs: &[PathBuf]) -> bool {
    let changed = after.iter().filter(|(path, time)| before.get(*path) != Some(*time)).map(|(path, _)| path);
    if before.keys().any(|path| !after.contains_key(path)) {
        return false;
    }
    let mut any = false;
    for path in changed {
        if !dirs.iter().any(|dir| path.starts_with(dir)) {
            return false;
        }
        any = true;
    }
    any
}

/// `cargo build`, with diagnostics shown as usual; the path of the package's binary.
fn build(root: &Path) -> Result<PathBuf, String> {
    let mut child = Command::new("cargo")
        .args(["build", "--message-format=json-render-diagnostics"])
        .current_dir(root)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    let mut binary = None;
    for line in BufReader::new(child.stdout.take().expect("piped")).lines().map_while(Result::ok) {
        // A bin target's artifact line: {"reason":"compiler-artifact",...,"executable":"/path"}.
        if line.contains(r#""reason":"compiler-artifact""#)
            && line.contains(r#""kind":["bin"]"#)
            && let Some(path) = json_string_field(&line, "executable")
        {
            binary = Some(PathBuf::from(path));
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("the build failed".into());
    }
    binary.ok_or_else(|| "the package has no binary to run".into())
}

/// The string value of `"field":"..."` in a line of cargo's JSON output.
pub(crate) fn json_string_field(line: &str, field: &str) -> Option<String> {
    let start = line.find(&format!(r#""{field}":""#))? + field.len() + 4;
    let mut out = String::new();
    let mut chars = line[start..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// The directory of every local package in the app's build (`cargo metadata` names them by a
/// `path+file://` package id), or `None` when cargo cannot say (a broken `Cargo.toml`), so the
/// previous list is kept.
fn local_packages(root: &Path) -> Option<Vec<PathBuf>> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(root)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(package_dirs(&String::from_utf8_lossy(&output.stdout)))
}

/// The directories named by the `path+file://` package ids in `text`: `path+file:///a/b#0.1.0`,
/// `path+file:///a/b#name@0.1.0`, or an older cargo's `name 0.1.0 (path+file:///a/b)`.
fn package_dirs(text: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (at, _) in text.match_indices("path+file://") {
        let rest = &text[at + "path+file://".len()..];
        let end = rest.find(['#', ')', '"', '?']).unwrap_or(rest.len());
        if let Some(dir) = file_url_path(&rest[..end])
            && !dirs.contains(&dir)
        {
            dirs.push(dir);
        }
    }
    dirs
}

/// The path of a `file://` URL's path part (`/D:/a%20b` on Windows, `/home/a` elsewhere).
fn file_url_path(url: &str) -> Option<PathBuf> {
    let bytes = url.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let path = String::from_utf8(decoded).ok()?;
    // `/C:/x` is `C:/x` on Windows.
    let path = match path.as_bytes() {
        [b'/', drive, b':', ..] if cfg!(windows) && drive.is_ascii_alphabetic() => &path[1..],
        _ => path.as_str(),
    };
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// The directories to watch: the app's and each local package's, leaving out one inside another
/// already watched, so no file is walked twice.
fn watch_roots(root: &Path, packages: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = std::iter::once(root.to_path_buf()).chain(packages).collect();
    all.sort_by_key(|p| p.components().count());
    let mut roots: Vec<PathBuf> = Vec::new();
    for dir in all {
        if !roots.iter().any(|r| dir.starts_with(r)) {
            roots.push(dir);
        }
    }
    roots
}

fn snapshot(roots: &[PathBuf]) -> BTreeMap<PathBuf, SystemTime> {
    let mut files = BTreeMap::new();
    for root in roots {
        walk(root, &mut files);
    }
    files
}

fn walk(dir: &Path, files: &mut BTreeMap<PathBuf, SystemTime>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if !IGNORED.contains(&name.as_ref()) && !name.starts_with('.') {
                walk(&path, files);
            }
        } else if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
            files.insert(path, modified);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_executable_from_cargo_json() {
        let line = r#"{"reason":"compiler-artifact","target":{"kind":["bin"],"name":"app"},"executable":"C:\\a\\target\\debug\\app.exe","fresh":false}"#;
        assert_eq!(json_string_field(line, "executable").as_deref(), Some(r"C:\a\target\debug\app.exe"));
        assert_eq!(json_string_field(r#"{"executable":null}"#, "executable"), None);
    }

    #[test]
    fn finds_each_local_package_in_cargo_metadata() {
        let metadata = concat!(
            r#"{"packages":[{"name":"axum","id":"registry+https://github.com/rust-lang/crates.io-index#axum@0.8.4"},"#,
            r#"{"name":"app","id":"path+file:///work/shop/app#0.1.0"},"#,
            r#"{"name":"platform","id":"path+file:///work/shop/my%20platform#platform@0.1.0"},"#,
            r#"{"name":"old","id":"old 0.1.0 (path+file:///work/old)"},"#,
            r#"{"name":"g","id":"git+https://github.com/a/g#abc"}],"#,
            r#""workspace_members":["path+file:///work/shop/app#0.1.0"]}"#
        );
        let dirs = package_dirs(metadata);
        assert_eq!(dirs, [PathBuf::from("/work/shop/app"), PathBuf::from("/work/shop/my platform"), PathBuf::from("/work/old")]);
    }

    #[test]
    fn a_change_to_static_files_alone_needs_no_build() {
        let t = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let file = |p: &str| PathBuf::from(p);
        let before = BTreeMap::from([(file("/app/static/style.css"), t(1)), (file("/app/src/main.rs"), t(1))]);
        let dirs = [PathBuf::from("/app/static")];
        let mut styled = before.clone();
        styled.insert(file("/app/static/style.css"), t(2));
        styled.insert(file("/app/static/new.svg"), t(2));
        assert!(only_under(&before, &styled, &dirs));
        let mut coded = styled.clone();
        coded.insert(file("/app/src/main.rs"), t(2));
        assert!(!only_under(&before, &coded, &dirs), "a source change builds");
        assert!(!only_under(&before, &styled, &[]), "an app that did not say where its files are builds");
        assert!(!only_under(&before, &before, &dirs), "nothing changed");
        let mut removed = before.clone();
        removed.remove(&file("/app/static/style.css"));
        assert!(!only_under(&before, &removed, &dirs), "a removed file builds: a page may still name it");
    }

    #[test]
    fn watches_each_directory_once() {
        let roots = watch_roots(
            Path::new("/work/shop/app"),
            vec![PathBuf::from("/work/shop/app"), PathBuf::from("/work/shop/platform"), PathBuf::from("/work/shop/app/macros")],
        );
        assert_eq!(roots, [PathBuf::from("/work/shop/app"), PathBuf::from("/work/shop/platform")]);
    }

    #[test]
    fn a_change_in_a_path_dependency_is_seen() {
        let base = std::env::temp_dir().join(format!("corten-dev-watch-{}", std::process::id()));
        let (app, lib) = (base.join("app"), base.join("lib"));
        std::fs::create_dir_all(app.join("src")).unwrap();
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::write(
            app.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nlib = { path = \"../lib\" }\n",
        )
        .unwrap();
        std::fs::write(app.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(lib.join("Cargo.toml"), "[package]\nname = \"lib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::write(lib.join("src/lib.rs"), "").unwrap();
        let packages = local_packages(&app).expect("cargo metadata runs");
        let roots = watch_roots(&app, packages);
        let lib_file = lib.join("src").join("lib.rs");
        let before = snapshot(&roots);
        assert!(before.keys().any(|p| p.canonicalize().ok() == lib_file.canonicalize().ok()), "{roots:?}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
