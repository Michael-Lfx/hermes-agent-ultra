use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=HERMES_GIT_COMMIT");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");

    let commit = std::env::var("HERMES_GIT_COMMIT")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(resolve_git_commit)
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=HERMES_GIT_COMMIT={commit}");
}

fn resolve_git_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?;
    let commit = commit.trim();
    if commit.is_empty() {
        None
    } else {
        Some(commit.to_string())
    }
}
