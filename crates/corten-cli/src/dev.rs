//! `corten dev`: build, run, and rebuild and restart on every change.
//!
//! Changes are found by polling modification times (no dependencies, works in containers and
//! on network drives). A failed build leaves the running server alone. The binary runs from a
//! copy, so the next build can replace the original while it runs (Windows locks running
//! executables). The app gets `CORTEN_DEV=1`: Corten then serves `/corten/dev`, and every open
//! page reloads when it reconnects to a server that started later.

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
    let mut seen = snapshot(&root);
    loop {
        match build(&root) {
            Ok(binary) => {
                if let Some(mut old) = server.take() {
                    let _ = old.kill();
                    let _ = old.wait();
                }
                let copy = root.join("target").join("corten-dev").join(binary.file_name().unwrap_or_default());
                std::fs::create_dir_all(copy.parent().expect("has a parent")).map_err(|e| e.to_string())?;
                std::fs::copy(&binary, &copy).map_err(|e| format!("copy {}: {e}", binary.display()))?;
                let child = Command::new(&copy)
                    .env("CORTEN_DEV", "1")
                    .env("PORT", &port)
                    .current_dir(&root)
                    .spawn()
                    .map_err(|e| format!("could not start {}: {e}", copy.display()))?;
                server = Some(child);
            }
            Err(message) => eprintln!("corten: {message}; the last good build keeps running"),
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
            let now = snapshot(&root);
            if now != seen {
                std::thread::sleep(Duration::from_millis(150));
                seen = snapshot(&root);
                eprintln!("corten: change detected, rebuilding");
                break;
            }
        }
    }
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
fn json_string_field(line: &str, field: &str) -> Option<String> {
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

fn snapshot(root: &Path) -> BTreeMap<PathBuf, SystemTime> {
    let mut files = BTreeMap::new();
    walk(root, &mut files);
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
}
