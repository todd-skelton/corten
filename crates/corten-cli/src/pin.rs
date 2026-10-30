//! Whether this `corten` command was built from the corten the app builds against: an app pins
//! corten (a git revision or a release) in its `Cargo.lock`, and a command from another one
//! formats and checks the way that one did, so `check`, `fmt` and `dev` say so first. While a
//! pin is being bumped, the lock still names the corten the app is leaving until cargo resolves
//! it again, so a `Cargo.toml` that asks for another revision than the lock was written for is
//! what the command is compared with.

use std::{io::Write, path::Path};

/// The commit this command was built from, when known (`build.rs`).
pub const REV: &str = env!("CORTEN_CLI_REV");

/// The corten an app's `Cargo.lock` names.
#[derive(Debug, PartialEq)]
pub enum Locked {
    /// `git+URL?rev=..#sha`: the repository, the commit, and what `Cargo.toml` asked for when
    /// the lock was written (`rev=8680dd62`, `tag=v0.2`, `branch=main`; empty for the default
    /// branch).
    Git { url: String, sha: String, asked: String },
    /// A release from a registry.
    Release(String),
}

/// The `corten` packages in a `Cargo.lock`; a path dependency (a checkout of corten itself)
/// names no source, so it is left out.
pub fn locked(lock: &str) -> Vec<Locked> {
    let mut found = Vec::new();
    for block in lock.split("[[package]]").skip(1) {
        let field = |key: &str| {
            block.lines().find_map(|line| {
                let value = line.strip_prefix(key)?.trim_start().strip_prefix('=')?.trim();
                Some(value.trim_matches('"').to_string())
            })
        };
        if field("name").as_deref() != Some("corten") {
            continue;
        }
        let Some(source) = field("source") else { continue };
        if let Some(git) = source.strip_prefix("git+") {
            let Some((url, sha)) = git.split_once('#') else { continue };
            let (url, asked) = url.split_once('?').unwrap_or((url, ""));
            found.push(Locked::Git { url: url.to_string(), sha: sha.to_string(), asked: asked.to_string() });
        } else if let Some(version) = field("version") {
            found.push(Locked::Release(version));
        }
    }
    found
}

/// What a manifest asks of its `corten` git dependency: `rev=ab31954a`, `tag=..` or `branch=..`
/// as the lock records it, or empty for the default branch. `None` when it has no `corten` git
/// dependency. Reads `corten = { git = .., rev = .. }` and a `[dependencies.corten]` table (or
/// `[workspace.dependencies.corten]`).
pub fn asked(manifest: &str) -> Option<String> {
    let mut table = false;
    let mut keys: Vec<(String, String)> = Vec::new();
    let pair = |pair: &str| {
        let (key, value) = pair.split_once('=')?;
        Some((key.trim().to_string(), value.trim().trim_matches('"').to_string()))
    };
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            table = matches!(
                line.trim_matches(['[', ']']).trim(),
                "dependencies.corten" | "workspace.dependencies.corten" | "dev-dependencies.corten"
            );
        } else if table {
            keys.extend(pair(line));
        } else if let Some(inner) = line.strip_prefix("corten").and_then(|rest| rest.trim_start().strip_prefix('=')) {
            let inner = inner.trim().strip_prefix('{').and_then(|t| t.strip_suffix('}')).unwrap_or("");
            keys.extend(inner.split(',').filter_map(pair));
        }
    }
    keys.iter().any(|(k, _)| k == "git").then(|| {
        ["rev", "tag", "branch"]
            .iter()
            .find_map(|kind| keys.iter().find(|(k, _)| k == kind).map(|(k, v)| format!("{k}={v}")))
            .unwrap_or_default()
    })
}

/// What to tell the user when this command (`version`, built from `rev`) is not the one for
/// `locked`: what differs and the command that installs the matching one. `None` when they
/// match, or when this command's commit is not known.
pub fn mismatch(locked: &Locked, version: &str, rev: &str) -> Option<String> {
    let short = |sha: &str| sha.get(..8).unwrap_or(sha).to_string();
    match locked {
        Locked::Git { url, sha, .. } if !rev.is_empty() && !sha.starts_with(rev) && !rev.starts_with(sha.as_str()) => {
            Some(format!(
                // The whole commit: cargo finds a short one only among the branches and tags it fetches.
                "this corten command was built from corten {}, and the app builds against {}, so fmt and check may not do what that one does. Install the matching command:\n  cargo install --git {url} --rev {sha} corten-cli --locked",
                short(rev),
                short(sha)
            ))
        }
        Locked::Release(release) if release != version => Some(format!(
            "this corten command is {version}, and the app builds against corten {release}, so fmt and check may not do what that one does. Install the matching command:\n  cargo install corten-cli --version {release} --locked"
        )),
        _ => None,
    }
}

/// The lock's git pin as `Cargo.toml` asks for it now: unchanged when they agree; while a pin is
/// being bumped (the manifest asks for another revision than the lock was written for), that
/// revision, and `None` for another tag or branch, whose commit is not known until cargo
/// resolves it.
pub fn as_asked(locked: Locked, asked: Option<&str>) -> Option<Locked> {
    match (locked, asked) {
        (Locked::Git { url, asked: was, .. }, Some(now)) if was != now => {
            let sha = now.strip_prefix("rev=")?.to_string();
            Some(Locked::Git { url, sha, asked: now.to_string() })
        }
        (locked, _) => Some(locked),
    }
}

/// Prints a note when the app in `root` (or the workspace above it) pins another corten.
pub fn warn(root: &Path) {
    let Some(lock) = root.ancestors().map(|dir| dir.join("Cargo.lock")).find(|path| path.is_file()) else { return };
    let Ok(text) = std::fs::read_to_string(&lock) else { return };
    // What the manifests between here and the lock ask for (the nearest that names corten).
    let top = lock.parent().unwrap_or(root);
    let asked = root
        .ancestors()
        .take_while(|dir| dir.starts_with(top))
        .filter_map(|dir| std::fs::read_to_string(dir.join("Cargo.toml")).ok())
        .find_map(|manifest| asked(&manifest));
    let locked: Vec<Locked> = locked(&text).into_iter().filter_map(|l| as_asked(l, asked.as_deref())).collect();
    if locked.iter().any(|l| mismatch(l, env!("CARGO_PKG_VERSION"), REV).is_none()) {
        return;
    }
    if let Some(note) = locked.first().and_then(|l| mismatch(l, env!("CARGO_PKG_VERSION"), REV)) {
        // A note: a closed stderr fails nothing.
        let _ = writeln!(std::io::stderr(), "corten: note: {note}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCK: &str = r#"
[[package]]
name = "axum"
version = "0.8.4"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "corten"
version = "0.1.0"
source = "git+https://github.com/todd-skelton/corten?rev=8680dd62#8680dd620abceb65da75017a21456aa28977b832"
dependencies = [
 "axum",
]

[[package]]
name = "corten-macros"
version = "0.1.0"
source = "git+https://github.com/todd-skelton/corten?rev=8680dd62#8680dd620abceb65da75017a21456aa28977b832"
"#;

    #[test]
    fn a_pinned_revision_is_read_from_the_lock() {
        let sha = "8680dd620abceb65da75017a21456aa28977b832";
        assert_eq!(
            locked(LOCK),
            [Locked::Git { url: "https://github.com/todd-skelton/corten".into(), sha: sha.into(), asked: "rev=8680dd62".into() }]
        );
        let release = "[[package]]\nname = \"corten\"\nversion = \"0.2.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
        assert_eq!(locked(release), [Locked::Release("0.2.0".into())]);
        // A checkout of corten itself (a path dependency) names no source.
        assert_eq!(locked("[[package]]\nname = \"corten\"\nversion = \"0.1.0\"\n"), Vec::<Locked>::new());
    }

    #[test]
    fn another_corten_says_which_and_how_to_install_the_matching_command() {
        let pinned = &locked(LOCK)[0];
        assert_eq!(mismatch(pinned, "0.1.0", "8680dd620abceb65da75017a21456aa28977b832"), None);
        let note = mismatch(pinned, "0.1.0", "dd028b94ad0f63d687ef65b157117fa53caab41c").expect("another commit");
        assert!(note.contains("built from corten dd028b94, and the app builds against 8680dd62"), "{note}");
        assert!(
            note.ends_with("cargo install --git https://github.com/todd-skelton/corten --rev 8680dd620abceb65da75017a21456aa28977b832 corten-cli --locked"),
            "{note}"
        );
        // Built where the commit is not known: nothing to compare.
        assert_eq!(mismatch(pinned, "0.1.0", ""), None);
        let release = Locked::Release("0.2.0".into());
        assert!(
            mismatch(&release, "0.1.0", "")
                .expect("another release")
                .ends_with("cargo install corten-cli --version 0.2.0 --locked")
        );
        assert_eq!(mismatch(&release, "0.2.0", ""), None);
    }

    #[test]
    fn a_manifest_says_what_it_asks_of_corten() {
        let inline = "[dependencies]\ncorten = { git = \"https://github.com/todd-skelton/corten\", rev = \"ab31954a\", features = [\"json\"] }\ncorten-ui = { git = \"https://github.com/todd-skelton/corten\", rev = \"ab31954a\" }\n";
        assert_eq!(asked(inline).as_deref(), Some("rev=ab31954a"));
        let table = "[workspace.dependencies.corten]\ngit = \"https://github.com/todd-skelton/corten\"\ntag = \"v0.2.0\"\n\n[dependencies]\nserde = \"1\"\n";
        assert_eq!(asked(table).as_deref(), Some("tag=v0.2.0"));
        assert_eq!(asked("[dependencies]\ncorten = { git = \"https://github.com/todd-skelton/corten\" }\n").as_deref(), Some(""));
        // A release, a member that takes the workspace's, or no corten: nothing asked here.
        assert_eq!(asked("[dependencies]\ncorten = \"0.2\"\n"), None);
        assert_eq!(asked("[dependencies]\ncorten = { workspace = true }\n"), None);
        assert_eq!(asked("[dependencies]\nserde = { version = \"1\", rev = \"x\" }\n"), None);
    }

    #[test]
    fn during_a_pin_bump_the_command_is_compared_with_what_the_manifest_asks_for() {
        let pinned = || locked(LOCK).remove(0);
        // The manifest asks what the lock was written for: the lock's commit.
        assert_eq!(as_asked(pinned(), Some("rev=8680dd62")), Some(pinned()));
        assert_eq!(as_asked(pinned(), None), Some(pinned()));
        // Bumped to ab31954a, the lock not resolved yet: a command built from ab31954a says nothing,
        // and another one is told to install ab31954a, not the commit the app is leaving.
        let bumped = as_asked(pinned(), Some("rev=ab31954a")).expect("a revision");
        assert_eq!(mismatch(&bumped, "0.1.0", "ab31954a1b2c3d4e5f60718293a4b5c6d7e8f901"), None);
        let note = mismatch(&bumped, "0.1.0", "dd028b94ad0f63d687ef65b157117fa53caab41c").expect("another commit");
        assert!(note.ends_with("--rev ab31954a corten-cli --locked"), "{note}");
        // Bumped to a tag or a branch: its commit is not known until cargo resolves it.
        assert_eq!(as_asked(pinned(), Some("tag=v0.2.0")), None);
        assert_eq!(as_asked(pinned(), Some("")), None);
    }
}
