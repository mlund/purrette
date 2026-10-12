//! Embeds `git describe` (nearest tag, commits since, SHA, `-dirty`) so a running app,
//! and anything it exports, can name the exact code that built it.
//!
//! Plain `git` instead of a crate: no API churn. Outside a checkout (source archive,
//! shallow CI clone) the revision is "unknown" rather than a build failure.

use std::process::Command;

fn main() {
    let revision = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    if revision.is_none() {
        println!("cargo:warning=git revision unavailable; app will report \"unknown\"");
    }
    println!(
        "cargo:rustc-env=GIT_REVISION={}",
        revision.as_deref().unwrap_or("unknown")
    );
    // HEAD moves on commit/checkout, refs on tag, index on staging (dirty state).
    for path in [".git/HEAD", ".git/refs", ".git/index"] {
        println!("cargo:rerun-if-changed={path}");
    }
}
