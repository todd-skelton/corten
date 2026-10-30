//! Whether this `corten` command was built from the corten the app builds against: an app pins
//! corten (a git revision or a release) in its `Cargo.lock`, and a command from another one
//! formats and checks the way that one did, so `check`, `fmt` and `dev` say so first.

use std::path::Path;

/// The commit this command was built from, when known (`build.rs`).
pub const REV: &str = env!("CORTEN_CLI_REV");

/// The corten an app's `Cargo.lock` names.
#[derive(Debug, PartialEq)]
pub enum Locked {
    /// `git+URL?rev=..#sha`: the repository and the commit.
    Git { url: String, sha: String },
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
            let url = url.split_once('?').map_or(url, |(url, _)| url);
            found.push(Locked::Git { url: url.to_string(), sha: sha.to_string() });
        } else if let Some(version) = field("version") {
            found.push(Locked::Release(version));
        }
    }
    found
}

/// What to tell the user when this command (`version`, built from `rev`) is not the one for
/// `locked`: what differs and the command that installs the matching one. `None` when they
/// match, or when this command's commit is not known.
pub fn mismatch(locked: &Locked, version: &str, rev: &str) -> Option<String> {
    let short = |sha: &str| sha.get(..8).unwrap_or(sha).to_string();
    match locked {
        Locked::Git { url, sha } if !rev.is_empty() && !sha.starts_with(rev) && !rev.starts_with(sha.as_str()) => Some(format!(
            "this corten command was built from corten {}, and the app builds against {}, so fmt and check may not do what that one does. Install the matching command:\n  cargo install --git {url} --rev {} corten-cli --locked",
            short(rev),
            short(sha),
            short(sha)
        )),
        Locked::Release(release) if release != version => Some(format!(
            "this corten command is {version}, and the app builds against corten {release}, so fmt and check may not do what that one does. Install the matching command:\n  cargo install corten-cli --version {release} --locked"
        )),
        _ => None,
    }
}

/// Prints a note when the app in `root` (or the workspace above it) pins another corten.
pub fn warn(root: &Path) {
    let Some(lock) = root.ancestors().map(|dir| dir.join("Cargo.lock")).find(|path| path.is_file()) else { return };
    let Ok(text) = std::fs::read_to_string(lock) else { return };
    let locked = locked(&text);
    if locked.iter().any(|l| mismatch(l, env!("CARGO_PKG_VERSION"), REV).is_none()) {
        return;
    }
    if let Some(note) = locked.first().and_then(|l| mismatch(l, env!("CARGO_PKG_VERSION"), REV)) {
        eprintln!("corten: note: {note}\n");
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
        assert_eq!(locked(LOCK), [Locked::Git { url: "https://github.com/todd-skelton/corten".into(), sha: sha.into() }]);
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
            note.ends_with("cargo install --git https://github.com/todd-skelton/corten --rev 8680dd62 corten-cli --locked"),
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
}
