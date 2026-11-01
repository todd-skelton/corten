//! Two crates' namesakes fail to link once a program uses both, on every platform: a library's
//! `Dialog` with a stylesheet and the app's, and a library's `PanelId` and the app's. A Windows
//! GNU link loads an object file from a library only for a symbol it needs, so before each item's
//! constant pointed at a static beside its symbols, it left the library's out and linked, while
//! Linux refused the same program (#570).
//!
//! It builds two small crates against this one with `cargo`, offline, from the workspace's lock.

use std::{path::Path, process::Command};

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The build's output, after writing the app's `main.rs`; `release` builds the profile an app ships.
fn build(root: &Path, main: &str, release: bool) -> (bool, String) {
    write(&root.join("app/src/main.rs"), main);
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("link-clash-target");
    let mut args = vec!["build", "--offline", "-q", "-p", "app"];
    if release {
        args.push("--release");
    }
    // What matters of the release profile is that debug assertions are off; unoptimized, it builds
    // in about the time a debug build does.
    let out = Command::new(env!("CARGO"))
        .args(args)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", target)
        .env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "0")
        .output()
        .unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn namesakes_in_two_crates_fail_to_link_once_both_are_used() {
    let corten = Path::new(env!("CARGO_MANIFEST_DIR")).to_string_lossy().replace('\\', "/");
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("link-clash");
    let manifest = |name: &str, deps: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[dependencies]\ncorten = {{ path = \"{corten}\" }}\n{deps}")
    };
    write(&root.join("Cargo.toml"), "[workspace]\nmembers = [\"kit\", \"app\"]\nresolver = \"2\"\n");
    // The workspace's lock, so the build reads no index.
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"), root.join("Cargo.lock")).unwrap();
    let component = |extra: &str| {
        format!(
            "use corten::prelude::*;\n#[derive(Id)]\npub struct PanelId;\n#[component(css)]\npub fn Dialog() -> impl Render {{\n    html! {{ <div class={{Dialog}} id={{PanelId}}>\"{extra}\"</div> }}\n}}\n"
        )
    };
    write(&root.join("kit/Cargo.toml"), &manifest("kit", ""));
    write(&root.join("kit/src/lib.rs"), &component("kit"));
    write(&root.join("kit/src/dialog.css"), ".dialog { gap: 0; }\n");
    write(&root.join("app/Cargo.toml"), &manifest("app", "kit = { path = \"../kit\" }\n"));
    write(&root.join("app/src/dialog.css"), ".dialog { gap: 1rem; }\n");
    let own = component("app");
    // The setup builds: the app uses its own only and never names the kit, so the kit is not linked
    // at all (this says nothing about a namesake in a linked crate).
    let (ok, stderr) = build(&root, &format!("{own}fn main() {{ let _ = App::new(Routes::new()).components(&[Dialog]); println!(\"{{}}\", Dialog {{}}.to_html()); }}\n"), false);
    assert!(ok, "{stderr}");
    // The issue's program: the app lists both, renders neither and writes no id, in the profile an
    // app ships and in a debug build.
    let listed = format!("{own}fn main() {{ let _ = App::new(Routes::new()).components(&[Dialog, kit::Dialog]); }}\n");
    for release in [true, false] {
        let (ok, stderr) = build(&root, &listed, release);
        assert!(!ok, "both listed Dialogs linked (release: {release})");
        assert!(stderr.contains("corten__two_components_style__dialog"), "{stderr}");
    }
    // A page renders the kit's as well, and writes both ids (a debug build, as every test is).
    let (ok, stderr) = build(
        &root,
        &format!("{own}fn main() {{ let _ = App::new(Routes::new()).components(&[Dialog]); println!(\"{{}}{{}}\", Dialog {{}}.to_html(), kit::Dialog {{}}.to_html()); }}\n"),
        false,
    );
    assert!(!ok, "both Dialogs linked");
    assert!(stderr.contains("corten__two_components_style__dialog"), "{stderr}");
    assert!(stderr.contains("corten__two_ids_are__panel"), "{stderr}");
}
