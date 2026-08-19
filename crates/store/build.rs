fn main() {
    // The SQLx migrator embeds this directory at compile time. Explicitly
    // track it so Docker's persistent Cargo target cache cannot reuse a store
    // crate that predates a newly added migration file.
    println!("cargo:rerun-if-changed=../../migrations");
}
