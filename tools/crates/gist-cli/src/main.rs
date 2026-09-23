//! `gk` — the Gist toolkit.
//!
//! Tools here are called by an agent far more often than by a person, so the
//! defaults lean that way: no prompts, no spinners, bounded output.

use clap::{Parser, Subcommand};
use gist_core::{emit, exit, fail, Human};
use ignore::WalkBuilder;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "gk",
    version,
    about = "Gist toolkit — understand enough to steer"
)]
struct Cli {
    /// Emit a JSON envelope instead of human-readable text
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Summarize the shape of a tree: how many files, which languages, how big
    Outline {
        /// Directory to summarize
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Maximum number of file types to report
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// Include files that .gitignore excludes
        #[arg(long)]
        all: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Outline { path, limit, all } => match outline(&path, limit, all) {
            Ok(result) => emit(result, cli.json),
            Err(message) => fail(message, exit::FAILURE, cli.json),
        },
    }
}

#[derive(Debug, Serialize)]
struct Outline {
    root: String,
    files: usize,
    bytes: u64,
    file_types: Vec<FileType>,
    /// True when `file_types` was cut short by `--limit`.
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct FileType {
    extension: String,
    files: usize,
    bytes: u64,
}

fn outline(path: &PathBuf, limit: usize, all: bool) -> Result<Outline, String> {
    if !path.exists() {
        return Err(format!("no such path: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("not a directory: {}", path.display()));
    }

    let mut counts: HashMap<String, (usize, u64)> = HashMap::new();
    let mut files = 0usize;
    let mut bytes = 0u64;

    let walker = WalkBuilder::new(path)
        .hidden(!all)
        .git_ignore(!all)
        .git_exclude(!all)
        .build();

    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        let extension = entry
            .path()
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("(none)")
            .to_string();

        let slot = counts.entry(extension).or_insert((0, 0));
        slot.0 += 1;
        slot.1 += size;
        files += 1;
        bytes += size;
    }

    let mut file_types: Vec<FileType> = counts
        .into_iter()
        .map(|(extension, (files, bytes))| FileType {
            extension,
            files,
            bytes,
        })
        .collect();

    // Biggest footprint first — that is the part worth asking about.
    file_types.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.extension.cmp(&b.extension)));
    let truncated = file_types.len() > limit;
    file_types.truncate(limit);

    Ok(Outline {
        root: path.display().to_string(),
        files,
        bytes,
        file_types,
        truncated,
    })
}

impl Human for Outline {
    fn human(&self) -> String {
        let mut out = format!(
            "{}: {} files, {}\n",
            self.root,
            self.files,
            human_bytes(self.bytes)
        );
        for entry in &self.file_types {
            out.push_str(&format!(
                "  {:<12} {:>5} files  {:>10}\n",
                entry.extension,
                entry.files,
                human_bytes(entry.bytes)
            ));
        }
        if self.truncated {
            out.push_str("  … more file types hidden by --limit\n");
        }
        out.trim_end().to_string()
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_paths_are_an_expected_failure_not_a_panic() {
        let err = outline(&PathBuf::from("/nope/does/not/exist"), 10, false).unwrap_err();
        assert!(err.contains("no such path"));
    }

    #[test]
    fn bytes_render_at_a_readable_scale() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
    }

    #[test]
    fn outline_counts_files_in_its_own_source_tree() {
        let result = outline(&PathBuf::from(env!("CARGO_MANIFEST_DIR")), 20, false).unwrap();
        assert!(result.files > 0);
        assert!(result.file_types.iter().any(|t| t.extension == "rs"));
    }
}
