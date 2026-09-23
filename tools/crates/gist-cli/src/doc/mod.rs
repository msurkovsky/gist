//! `gk doc` — how much of a change is documentation.
//!
//! The number that matters is the ratio of *documentation* to code, where
//! documentation means a comment run of at least `--min-run` lines. A lone
//! `// bump the retry` is an aside, not documentation, and counting it hides
//! the thing you actually want to see: blocks of prose growing faster than
//! the code they describe.

mod git;
mod lang;

use clap::Args as ClapArgs;
use gist_core::Human;
use git::Source;
use ignore::WalkBuilder;
use lang::{LineKind, Scanner};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Command line for `gk doc`.
#[derive(ClapArgs, Debug)]
pub struct Args {
    /// Count whole files or directories instead of a diff
    #[arg(long, num_args = 1.., value_name = "PATH")]
    files: Vec<PathBuf>,

    /// Measure the index instead of the working tree
    #[arg(long, conflicts_with_all = ["files", "range", "base"])]
    staged: bool,

    /// Measure a revision range, e.g. main..HEAD
    #[arg(long, value_name = "SPEC", conflicts_with_all = ["files", "base"])]
    range: Option<String>,

    /// Measure against the merge base with a branch
    #[arg(long, value_name = "REV", conflicts_with = "files")]
    base: Option<String>,

    /// Shortest comment run that counts as documentation
    #[arg(long, default_value_t = 2, value_name = "N")]
    min_run: usize,

    /// Maximum files and runs listed
    #[arg(long, default_value_t = 20, value_name = "N")]
    limit: usize,
}

/// One measurement: totals, the files behind them, and the runs to go read.
#[derive(Debug, Serialize)]
pub struct Report {
    source: String,
    min_run: usize,
    totals: Totals,
    files: Vec<FileReport>,
    /// Comment runs of at least `min_run` lines, longest first.
    runs: Vec<Run>,
    /// Files whose post-image could not be read, so block state was guessed.
    approximate_files: usize,
    /// Files with no comment syntax worth counting: data, prose, lockfiles.
    skipped_files: usize,
    truncated: bool,
}

#[derive(Debug, Default, Serialize)]
struct Totals {
    /// Comment lines belonging to a run of at least `min_run` lines.
    doc: usize,
    /// Every comment line, one-line asides included.
    comment: usize,
    code: usize,
    blank: usize,
    /// `doc / (doc + code)`.
    ratio: f64,
}

#[derive(Debug, Serialize)]
struct FileReport {
    path: String,
    doc: usize,
    comment: usize,
    code: usize,
    ratio: f64,
    /// True when block-comment state was guessed from the diff alone.
    approximate: bool,
}

#[derive(Debug, Serialize)]
struct Run {
    path: String,
    line: usize,
    lines: usize,
}

/// Measure a diff, or whole files when `--files` is given.
pub fn run(args: Args) -> Result<Report, String> {
    if args.files.is_empty() {
        measure_diff(&args)
    } else {
        measure_files(&args)
    }
}

fn source_from(args: &Args) -> Source {
    if args.staged {
        Source::Staged
    } else if let Some(spec) = &args.range {
        Source::Range(spec.clone())
    } else if let Some(rev) = &args.base {
        Source::Base(rev.clone())
    } else {
        Source::Worktree
    }
}

fn measure_diff(args: &Args) -> Result<Report, String> {
    let source = source_from(args);
    let repo = git::Repo::discover()?;
    let diffs = repo.diff(&source)?;

    let mut builder = ReportBuilder::new(source.label(), args.min_run);

    for file in diffs {
        if file.added.is_empty() {
            continue;
        }
        let Some(syntax) = lang::syntax_for(Path::new(&file.path)) else {
            builder.skipped_files += 1;
            continue;
        };

        let selected: Vec<usize> = file.added.iter().map(|(number, _)| *number).collect();
        let mut approximate = false;
        let kinds: Vec<LineKind> = match repo.post_image(&source, &file.path) {
            // Classifying the whole file is the only way to know whether an
            // added line sits inside a block comment opened further up.
            Some(content) => {
                let mut scanner = Scanner::new(syntax);
                let all: Vec<LineKind> = content.lines().map(|l| scanner.classify(l)).collect();
                selected
                    .iter()
                    .map(|number| all.get(number - 1).copied().unwrap_or(LineKind::Code))
                    .collect()
            }
            None => {
                approximate = true;
                builder.approximate_files += 1;
                let mut scanner = Scanner::new(syntax);
                file.added
                    .iter()
                    .map(|(_, text)| scanner.classify(text))
                    .collect()
            }
        };

        builder.add_file(&file.path, &selected, &kinds, approximate);
    }

    Ok(builder.finish(args.limit))
}

fn measure_files(args: &Args) -> Result<Report, String> {
    let mut builder = ReportBuilder::new("files".to_string(), args.min_run);

    for root in &args.files {
        if !root.exists() {
            return Err(format!("no such path: {}", root.display()));
        }
        for entry in WalkBuilder::new(root).build().flatten() {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.path();
            let Some(syntax) = lang::syntax_for(path) else {
                builder.skipped_files += 1;
                continue;
            };
            let Ok(content) = std::fs::read_to_string(path) else {
                builder.skipped_files += 1;
                continue;
            };

            let mut scanner = Scanner::new(syntax);
            let kinds: Vec<LineKind> = content.lines().map(|l| scanner.classify(l)).collect();
            let numbers: Vec<usize> = (1..=kinds.len()).collect();
            builder.add_file(&path.display().to_string(), &numbers, &kinds, false);
        }
    }

    Ok(builder.finish(args.limit))
}

struct ReportBuilder {
    source: String,
    min_run: usize,
    totals: Totals,
    files: Vec<FileReport>,
    runs: Vec<Run>,
    approximate_files: usize,
    skipped_files: usize,
}

impl ReportBuilder {
    fn new(source: String, min_run: usize) -> Self {
        Self {
            source,
            min_run: min_run.max(1),
            totals: Totals::default(),
            files: Vec::new(),
            runs: Vec::new(),
            approximate_files: 0,
            skipped_files: 0,
        }
    }

    fn add_file(&mut self, path: &str, numbers: &[usize], kinds: &[LineKind], approximate: bool) {
        let tally = tally(numbers, kinds, self.min_run);
        if tally.comment == 0 && tally.code == 0 {
            return;
        }

        self.totals.doc += tally.doc;
        self.totals.comment += tally.comment;
        self.totals.code += tally.code;
        self.totals.blank += tally.blank;

        for (line, lines) in tally.runs {
            self.runs.push(Run {
                path: path.to_string(),
                line,
                lines,
            });
        }

        self.files.push(FileReport {
            path: path.to_string(),
            doc: tally.doc,
            comment: tally.comment,
            code: tally.code,
            ratio: ratio(tally.doc, tally.code),
            approximate,
        });
    }

    fn finish(mut self, limit: usize) -> Report {
        self.totals.ratio = ratio(self.totals.doc, self.totals.code);

        self.files
            .sort_by(|a, b| b.doc.cmp(&a.doc).then(a.path.cmp(&b.path)));
        self.runs.sort_by(|a, b| {
            b.lines
                .cmp(&a.lines)
                .then(a.path.cmp(&b.path))
                .then(a.line.cmp(&b.line))
        });

        let truncated = self.files.len() > limit || self.runs.len() > limit;
        self.files.truncate(limit);
        self.runs.truncate(limit);

        Report {
            source: self.source,
            min_run: self.min_run,
            totals: self.totals,
            files: self.files,
            runs: self.runs,
            approximate_files: self.approximate_files,
            skipped_files: self.skipped_files,
            truncated,
        }
    }
}

#[derive(Default)]
struct Tally {
    doc: usize,
    comment: usize,
    code: usize,
    blank: usize,
    /// (first line, length) of every comment run of at least `min_run` lines.
    runs: Vec<(usize, usize)>,
}

/// Count a file's selected lines, grouping comments into runs.
///
/// A run only continues across lines that are adjacent in the file, so two
/// comment lines added forty lines apart are two runs of one, not a run of two.
fn tally(numbers: &[usize], kinds: &[LineKind], min_run: usize) -> Tally {
    let mut tally = Tally::default();
    let mut run_start = 0usize;
    let mut run_len = 0usize;
    let mut previous = None;

    let close = |tally: &mut Tally, start: usize, len: usize| {
        if len >= min_run {
            tally.doc += len;
            tally.runs.push((start, len));
        }
    };

    for (index, kind) in kinds.iter().enumerate() {
        let number = numbers[index];
        let adjacent = previous.is_some_and(|last| last + 1 == number);

        match kind {
            LineKind::Comment => {
                tally.comment += 1;
                if run_len > 0 && adjacent {
                    run_len += 1;
                } else {
                    close(&mut tally, run_start, run_len);
                    run_start = number;
                    run_len = 1;
                }
            }
            LineKind::Code => {
                tally.code += 1;
                close(&mut tally, run_start, run_len);
                run_len = 0;
            }
            LineKind::Blank => {
                tally.blank += 1;
                close(&mut tally, run_start, run_len);
                run_len = 0;
            }
        }
        previous = Some(number);
    }
    close(&mut tally, run_start, run_len);

    tally
}

fn ratio(doc: usize, code: usize) -> f64 {
    let total = doc + code;
    if total == 0 {
        0.0
    } else {
        doc as f64 / total as f64
    }
}

impl Human for Report {
    fn human(&self) -> String {
        let mut out = format!(
            "{} · runs of {}+ lines count as documentation\n\n",
            self.source, self.min_run
        );
        out.push_str(&format!(
            "  doc {}   code {}   ({} comment incl. one-liners, {} blank)\n",
            self.totals.doc, self.totals.code, self.totals.comment, self.totals.blank
        ));
        out.push_str(&format!(
            "  {:.1}% documentation\n",
            self.totals.ratio * 100.0
        ));

        if !self.files.is_empty() {
            out.push_str(&format!(
                "\n  {:>5} {:>5} {:>7}  file\n",
                "doc", "code", "ratio"
            ));
            for file in &self.files {
                out.push_str(&format!(
                    "  {:>5} {:>5} {:>6.1}%  {}{}\n",
                    file.doc,
                    file.code,
                    file.ratio * 100.0,
                    file.path,
                    if file.approximate { " ~" } else { "" }
                ));
            }
        }

        if !self.runs.is_empty() {
            out.push_str("\n  longest runs\n");
            for run in self.runs.iter().take(10) {
                out.push_str(&format!("  {:>5}  {}:{}\n", run.lines, run.path, run.line));
            }
        }

        if self.approximate_files > 0 {
            out.push_str(&format!(
                "\n  ~ {} file(s) classified from the diff alone; block state is a guess\n",
                self.approximate_files
            ));
        }
        if self.truncated {
            out.push_str("  … list cut short by --limit\n");
        }

        out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(spec: &str) -> Vec<LineKind> {
        spec.chars()
            .map(|c| match c {
                'c' => LineKind::Comment,
                'b' => LineKind::Blank,
                _ => LineKind::Code,
            })
            .collect()
    }

    #[test]
    fn a_lone_comment_line_is_not_documentation() {
        let kinds = kinds("cxxx");
        let result = tally(&[1, 2, 3, 4], &kinds, 2);
        assert_eq!(result.comment, 1);
        assert_eq!(result.doc, 0, "one-liners are asides, not docs");
        assert!(result.runs.is_empty());
    }

    #[test]
    fn adjacent_comment_lines_form_a_run() {
        let result = tally(&[1, 2, 3, 4], &kinds("cccx"), 2);
        assert_eq!(result.doc, 3);
        assert_eq!(result.runs, vec![(1, 3)]);
    }

    #[test]
    fn comments_added_far_apart_are_separate_runs() {
        // Two added comment lines, forty lines apart in the file.
        let result = tally(&[10, 50], &kinds("cc"), 2);
        assert_eq!(result.comment, 2);
        assert_eq!(result.doc, 0, "non-adjacent lines are not one block");
    }

    #[test]
    fn min_run_of_one_counts_every_comment() {
        let result = tally(&[1, 5, 9], &kinds("ccc"), 1);
        assert_eq!(result.doc, 3);
        assert_eq!(result.runs.len(), 3);
    }

    #[test]
    fn blank_lines_break_a_run_and_stay_out_of_the_ratio() {
        let result = tally(&[1, 2, 3, 4], &kinds("cbcc"), 2);
        assert_eq!(result.blank, 1);
        assert_eq!(result.doc, 2, "only the trailing pair is a run");
        assert_eq!(ratio(result.doc, result.code), 1.0);
    }

    #[test]
    fn ratio_is_doc_over_doc_plus_code() {
        assert_eq!(ratio(0, 0), 0.0);
        assert_eq!(ratio(1, 3), 0.25);
    }
}
