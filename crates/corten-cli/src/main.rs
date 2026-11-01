//! `corten`: create, run and check Corten apps. Standard library only.
//!
//! - `corten new <dir>` writes a working app (page, form, handler script, test, Dockerfile).
//! - `corten dev` builds and runs the app, rebuilds and restarts it when a file changes (a failed
//!   build keeps the running server), and open pages reload by themselves.
//! - `corten build` builds the release binary. `corten check` runs format, lint and
//!   tests, then a type check of the app's scripts (with TypeScript's `tsc`, when it is installed).
//! - `corten fmt` runs `cargo fmt` and lays out the markup in `html!`, which rustfmt leaves alone.
//! - `corten ui` sets up the UI kit and copies its components into the app.

mod dev;
mod fmt;
mod json;
mod new;
mod pin;
mod scripts;
mod ui;

use std::process::{Command, ExitCode};

const HELP: &str = "corten: create, run and check Corten apps

Usage:
  corten new <dir> [--name <crate>] [--corten-path <path>]
                   Create an app in <dir>. --corten-path uses a local checkout of the
                   corten crate (crates/corten) instead of the crates.io release.
  corten dev [--port <port>]
                   Build and run the app in this directory; rebuild and restart on every
                   change, and reload open pages. Default port 3000.
  corten build     Build the release binary.
  corten fmt [--check] [path ..]
                   cargo fmt, then lay out the markup inside html! (rustfmt leaves macro
                   bodies alone), so it fixes what corten check reports. With paths, only
                   their markup. --check reports what needs it instead of writing it.
  corten check     corten fmt --check, cargo clippy -D warnings, cargo test, then a type
                   check of the scripts in src/ and static/ (TypeScript's tsc, from
                   node_modules or the PATH; without it, a note says they were not checked)
                   against the types/app.d.ts the tests wrote. At a workspace root it checks
                   the scripts of each member that uses corten.
  corten ui init [--kit-theme]
                   Set up the UI kit: src/ui.rs listing its components, declared as mod ui
                   and listed on the App. An app with a color theme of its own keeps it, and
                   is told which tokens the components read that it lacks; otherwise (or
                   with --kit-theme) it writes the kit's theme tokens (tokens/ui.*.json,
                   light and dark) and the page's colors in static/style.css.
  corten ui add <name>.. [--force] | corten ui add --all [--force]
                   Copy kit components into src/ui/, where they are yours to change, and
                   list them in src/ui.rs. Refuses to replace a file without --force.
  corten ui list   The kit's components.
  corten ui diff [name]..
                   How each copied component differs from this command's kit: whether you
                   edited it, whether the kit changed it since, and the lines. Writes nothing.
  corten help      This text.
  corten --version The version, and the commit the command was built from. dev, fmt and
                   check say when the app's Cargo.lock pins another corten, and how to
                   install the matching command.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("dev" | "fmt" | "check"))
        && let Ok(root) = std::env::current_dir()
    {
        pin::warn(&root);
    }
    let result = match args.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("-V" | "--version" | "version") => {
            match pin::REV.get(..8) {
                Some(rev) => println!("corten {} ({rev})", env!("CARGO_PKG_VERSION")),
                None => println!("corten {}", env!("CARGO_PKG_VERSION")),
            }
            Ok(())
        }
        Some("new") => new::run(&args[1..]),
        Some("dev") => dev::run(&args[1..]),
        Some("build") => cargo(&["build", "--release"]).map(|()| {
            println!(
                "\ncorten: built the release binary in target/release. It is the whole deploy: the static files are compiled into it (see Dockerfile)."
            );
        }),
        Some("fmt") => fmt::run(&args[1..]),
        Some("ui") => ui::run(&args[1..]),
        Some("check") => fmt::run(&["--check".to_string()])
            .and_then(|()| cargo(&["clippy", "--all-targets", "--", "-D", "warnings"]))
            // The tests first: the one that writes `types/app.d.ts` brings it up to date (and fails,
            // saying so, when it was stale) before `tsc` reads it, so a script that uses a new
            // view or event is not refused for a declaration the check had not written yet. The
            // scripts are checked whatever the tests did, so a failing test hides no script error.
            .and_then(|()| {
                let tests = cargo(&["test"]);
                let scripts = std::env::current_dir().map_err(|e| e.to_string()).and_then(|root| scripts::check(&root));
                match (tests, scripts) {
                    (Ok(()), scripts) => scripts,
                    (Err(tests), Ok(())) => Err(tests),
                    (Err(tests), Err(scripts)) => Err(format!("{tests}, and {scripts}")),
                }
            }),
        Some(other) => Err(format!("unknown command {other:?}; run corten help")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("corten: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Runs `cargo <args>` in the current directory and fails when it does.
fn cargo(args: &[&str]) -> Result<(), String> {
    let status = Command::new("cargo").args(args).status().map_err(|e| format!("could not run cargo: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("cargo {} failed", args.join(" "))) }
}

/// The value after `flag` in `args`, if present.
fn option<'a>(args: &'a [String], flag: &str) -> Result<Option<&'a str>, String> {
    match args.iter().position(|a| a == flag) {
        Some(i) => args.get(i + 1).map(|v| Some(v.as_str())).ok_or_else(|| format!("{flag} needs a value")),
        None => Ok(None),
    }
}
