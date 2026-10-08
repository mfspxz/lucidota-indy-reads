//! Build-time provenance capture (C1: source/build identity).
//!
//! Bakes into the binary: crate version, git commit (or "no-vcs"),
//! rustc version, and UTC build timestamp. The /health endpoint serves
//! these, so runtime truth can be compared against source truth without
//! trusting anyone's prose.
use std::process::Command;

fn cmd(prog: &str, args: &[&str]) -> Option<String> {
    Command::new(prog)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn main() {
    // Re-run when the commit changes, not on every build.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");

    let commit = cmd("git", &["rev-parse", "HEAD"]).unwrap_or_else(|| "no-vcs".to_string());
    let branch = cmd("git", &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "no-vcs".to_string());
    let dirty = cmd("git", &["status", "--porcelain"])
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let rustc = cmd("rustc", &["--version"]).unwrap_or_else(|| "unknown".to_string());
    let ts = cmd("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=INDY_BUILD_COMMIT={}", commit);
    println!("cargo:rustc-env=INDY_BUILD_BRANCH={}", branch);
    println!("cargo:rustc-env=INDY_BUILD_DIRTY={}", dirty);
    println!("cargo:rustc-env=INDY_BUILD_RUSTC={}", rustc);
    println!("cargo:rustc-env=INDY_BUILD_TS={}", ts);
}
