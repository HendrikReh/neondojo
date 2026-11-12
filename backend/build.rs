use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=migrations");

    let commit = git_commit().unwrap_or_default();
    println!("cargo:rustc-env=BGTUTOR_GIT_COMMIT={commit}");
}

fn git_commit() -> Option<String> {
    // Source archives may be unpacked inside a different Git checkout.
    if !Path::new("../.git").exists() {
        return None;
    }

    for path in ["HEAD", "packed-refs"].into_iter().filter_map(git_path) {
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git_path(&reference)
        && let Some(existing) = path.ancestors().find(|path| path.exists())
    {
        // A packed branch gains a loose ref when a new commit advances it.
        println!("cargo:rerun-if-changed={}", existing.display());
    }

    git(&["rev-parse", "--verify", "HEAD"])
}

fn git_path(path: &str) -> Option<PathBuf> {
    git(&["rev-parse", "--path-format=absolute", "--git-path", path]).map(PathBuf::from)
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim_end().to_owned())
}
