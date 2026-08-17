use std::process::Command;

fn main() {
    // The isolated build container this crate is compiled in receives an
    // explicit include-list `tar` of source files (Cargo.toml/.lock,
    // crates, migrations, testdata) that deliberately excludes `.git` for
    // safe transfer, so `git rev-parse` inside it always fails. The
    // caller must instead pass the real commit hash (read from the
    // actual repository before packaging) via `FIRESIFT_GIT_COMMIT`.
    // Falls back to a local git lookup for ordinary local builds where
    // `.git` is present, and to "unknown" only if neither is available.
    if let Ok(overridden) = std::env::var("FIRESIFT_GIT_COMMIT") {
        println!("cargo:rustc-env=FIRESIFT_GIT_COMMIT={overridden}");
        println!("cargo:rerun-if-env-changed=FIRESIFT_GIT_COMMIT");
        return;
    }

    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |s| s.trim().to_owned());

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| !o.stdout.is_empty());

    let commit = if dirty {
        format!("{commit}-dirty")
    } else {
        commit
    };

    println!("cargo:rustc-env=FIRESIFT_GIT_COMMIT={commit}");
    println!("cargo:rerun-if-env-changed=FIRESIFT_GIT_COMMIT");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/index");
}
