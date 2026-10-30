//! Records the commit the `corten` command is built from (`CORTEN_CLI_REV`, empty when it is not
//! known), so it can tell an app that pins another corten. Standard library only, like the CLI: it
//! reads `.git` itself, or the `.cargo_vcs_info.json` a crates.io package carries.

use std::{
    fs,
    path::{Path, PathBuf},
};

fn main() {
    let dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let rev = vcs_info(&dir).or_else(|| git_head(&dir)).unwrap_or_default();
    println!("cargo:rustc-env=CORTEN_CLI_REV={rev}");
}

/// The `git.sha1` of a published package's `.cargo_vcs_info.json`.
fn vcs_info(dir: &Path) -> Option<String> {
    // Only a file that exists is watched: cargo reruns a script whose watched file is missing.
    let path = dir.join(".cargo_vcs_info.json");
    let text = fs::read_to_string(&path).ok()?;
    println!("cargo:rerun-if-changed={}", path.display());
    let at = text.find("\"sha1\"")?;
    let rest = &text[at + 6..];
    let start = rest.find('"')? + 1;
    let sha = &rest[start..start + rest[start..].find('"')?];
    is_sha(sha).then(|| sha.to_string())
}

/// The commit `HEAD` names in the repository around `dir`: a cargo git checkout or a clone.
fn git_head(dir: &Path) -> Option<String> {
    let dot_git = dir.ancestors().map(|d| d.join(".git")).find(|p| p.exists())?;
    // A worktree's `.git` is a file naming its own directory, which names the shared one.
    let git = if dot_git.is_file() {
        let text = fs::read_to_string(&dot_git).ok()?;
        // A relative one (a submodule, `git worktree add --relative-paths`) is from the file's directory.
        dot_git.parent()?.join(text.strip_prefix("gitdir:")?.trim())
    } else {
        dot_git
    };
    let common = match fs::read_to_string(git.join("commondir")) {
        Ok(rel) => git.join(rel.trim()),
        Err(_) => git.clone(),
    };
    let head = git.join("HEAD");
    println!("cargo:rerun-if-changed={}", head.display());
    let head = fs::read_to_string(head).ok()?;
    let head = head.trim();
    let Some(name) = head.strip_prefix("ref:").map(str::trim) else {
        return is_sha(head).then(|| head.to_string());
    };
    // The directory the branch's loose ref is in, which is watched whether the ref is loose or
    // packed: a commit on a packed branch writes a new loose ref there, and a `git pack-refs`
    // removes one, and neither changes `HEAD` or `packed-refs`.
    for base in [&git, &common] {
        // Its nearest directory that exists: `pack-refs` removes one left empty.
        if let Some(dir) = base.join(name).ancestors().skip(1).find(|dir| dir.is_dir()) {
            println!("cargo:rerun-if-changed={}", dir.display());
        }
    }
    for base in [&git, &common] {
        if let Ok(sha) = fs::read_to_string(base.join(name)) {
            let sha = sha.trim();
            return is_sha(sha).then(|| sha.to_string());
        }
    }
    let packed = common.join("packed-refs");
    let text = fs::read_to_string(&packed).ok()?;
    println!("cargo:rerun-if-changed={}", packed.display());
    text.lines().find_map(|line| {
        let (sha, at) = line.split_once(' ')?;
        (at == name && is_sha(sha)).then(|| sha.to_string())
    })
}

fn is_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit())
}
