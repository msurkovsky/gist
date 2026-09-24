//! `include_dir!` in `src/init.rs` embeds `skills/`, but a proc macro cannot tell cargo
//! which files it read. Without this hint, editing or adding a skill leaves the previous
//! binary considered fresh: `gk init --claude` installs stale skills and the init tests
//! run against them.
fn main() {
    println!("cargo:rerun-if-changed=../../../skills");
}
