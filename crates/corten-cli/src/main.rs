//! `corten`: create, run and check Corten apps. Standard library only.
//!
//! - `corten new <dir>` writes a working app (page, form, handler script, test, Dockerfile).
//! - `corten dev` builds and runs the app, rebuilds and restarts it when a file changes (a failed
//!   build keeps the running server), and open pages reload by themselves.
//! - `corten build` builds the release binary. `corten check` runs format, lint, a type check of
//!   the app's scripts (with TypeScript's `tsc`, when it is installed) and tests.
//! - `corten fmt` runs `cargo fmt` and lays out the markup in `html!`, which rustfmt leaves alone.

mod dev;
mod fmt;
mod json;
mod new;
mod scripts;

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
  corten check     corten fmt --check, cargo clippy -D warnings, a type check
                   of the scripts in src/ and static/ (TypeScript's tsc, from node_modules or
                   the PATH; without it, a note says they were not checked), cargo test. At a
                   workspace root it checks the scripts of each member that uses corten.
  corten help      This text.
  corten --version
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            print!("{HELP}");
            Ok(())
        }
        Some("-V" | "--version" | "version") => {
            println!("corten {}", env!("CARGO_PKG_VERSION"));
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
        Some("check") => fmt::run(&["--check".to_string()])
            .and_then(|()| cargo(&["clippy", "--all-targets", "--", "-D", "warnings"]))
            .and_then(|()| std::env::current_dir().map_err(|e| e.to_string()).and_then(|root| scripts::check(&root)))
            .and_then(|()| cargo(&["test"])),
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
