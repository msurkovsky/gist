//! Block diff between two versions, for "Show changes" and the review
//! record. Blocks compare by their source, so a changed link target counts
//! even when the text reads the same. docs/md-review-page.md#changes-between-rounds.

use serde::Serialize;
use similar::{capture_diff_slices, Algorithm, DiffOp};

use super::render::Block;

/// What happened to one block of the new version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum Change {
    Unchanged {
        old: usize,
    },
    /// Took the place of the old block at `old`.
    Changed {
        old: usize,
    },
    Added,
}

/// An old block with no counterpart in the new version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Deleted {
    /// Index of the old block.
    pub old: usize,
    /// Index of the new block it stood before; the new block count when it
    /// stood at the end.
    pub before: usize,
}

/// The new version's blocks, each with its change, and the deleted ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockDiff {
    pub blocks: Vec<Change>,
    pub deleted: Vec<Deleted>,
}

impl BlockDiff {
    /// Blocks changed, added or deleted.
    pub fn changed(&self) -> usize {
        self.deleted.len()
            + self
                .blocks
                .iter()
                .filter(|change| !matches!(change, Change::Unchanged { .. }))
                .count()
    }
}

/// Diff two versions given as their rendered blocks.
pub fn diff_blocks(old: &[Block], new: &[Block]) -> BlockDiff {
    diff(&sources(old), &sources(new))
}

fn sources(blocks: &[Block]) -> Vec<&str> {
    blocks.iter().map(|b| b.source.as_str()).collect()
}

/// Diff two versions given as their blocks' sources. A run of old blocks
/// replaced by new ones pairs up in order; old blocks left over are
/// deleted, new ones left over are added.
pub fn diff(old: &[&str], new: &[&str]) -> BlockDiff {
    let mut blocks = vec![Change::Added; new.len()];
    let mut deleted = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, old, new) {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for offset in 0..len {
                    blocks[new_index + offset] = Change::Unchanged {
                        old: old_index + offset,
                    };
                }
            }
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => {
                deleted.extend((old_index..old_index + old_len).map(|old| Deleted {
                    old,
                    before: new_index,
                }));
            }
            DiffOp::Insert { .. } => {}
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for offset in 0..old_len.min(new_len) {
                    blocks[new_index + offset] = Change::Changed {
                        old: old_index + offset,
                    };
                }
                deleted.extend(
                    (old_index + new_len..old_index + old_len).map(|old| Deleted {
                        old,
                        before: new_index + new_len,
                    }),
                );
            }
        }
    }
    BlockDiff { blocks, deleted }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_version_has_no_changes() {
        let result = diff(&["a", "b"], &["a", "b"]);
        assert_eq!(result.changed(), 0);
        assert_eq!(
            result.blocks,
            [Change::Unchanged { old: 0 }, Change::Unchanged { old: 1 }]
        );
    }

    #[test]
    fn an_edited_block_is_changed_and_paired_with_its_old_self() {
        let result = diff(&["a", "b", "c"], &["a", "B", "c"]);
        assert_eq!(result.blocks[1], Change::Changed { old: 1 });
        assert!(result.deleted.is_empty());
    }

    #[test]
    fn inserted_and_deleted_blocks_are_marked_where_they_were() {
        let result = diff(&["a", "gone", "b"], &["a", "b", "new"]);
        assert_eq!(
            result.blocks,
            [
                Change::Unchanged { old: 0 },
                Change::Unchanged { old: 2 },
                Change::Added
            ]
        );
        assert_eq!(result.deleted, [Deleted { old: 1, before: 1 }]);
        assert_eq!(result.changed(), 2);
    }

    #[test]
    fn a_deletion_alone_is_a_change() {
        let result = diff(&["a", "b"], &["a"]);
        assert_eq!(result.deleted, [Deleted { old: 1, before: 1 }]);
        assert_eq!(result.changed(), 1);
    }

    #[test]
    fn a_replacement_by_fewer_blocks_deletes_the_rest() {
        let result = diff(&["a", "x", "y", "z", "b"], &["a", "X", "b"]);
        assert_eq!(result.blocks[1], Change::Changed { old: 1 });
        assert_eq!(
            result.deleted,
            [Deleted { old: 2, before: 2 }, Deleted { old: 3, before: 2 }]
        );
    }

    #[test]
    fn a_replacement_by_more_blocks_adds_the_rest() {
        let result = diff(&["a", "x", "b"], &["a", "X", "Y", "b"]);
        assert_eq!(
            result.blocks,
            [
                Change::Unchanged { old: 0 },
                Change::Changed { old: 1 },
                Change::Added,
                Change::Unchanged { old: 2 }
            ]
        );
    }
}
