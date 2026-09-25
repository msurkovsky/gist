//! `include_dir!` in `src/init.rs` embeds `skills/` and `experimental/`, but a proc macro
//! cannot tell cargo which files it read. Editing a file it embedded is caught, but adding
//! or removing one is not. Without this hint the previous binary is considered fresh:
//! `gk init` installs a stale set of skills and the init tests run against it.
fn main() {
    println!("cargo:rerun-if-changed=../../../skills");
    println!("cargo:rerun-if-changed=../../../experimental");
}
