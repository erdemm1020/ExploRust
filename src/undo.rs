//! ⌘Z for file operations. Every operation is recorded as the thing that
//! reverses it; undoing returns the operation that redoes it.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::trash;

const KEEP: usize = 50;

#[derive(Clone, Debug, PartialEq)]
pub enum UndoOp {
    /// Rename each `(from, to)`: puts moved or renamed items back.
    MoveBack(Vec<(PathBuf, PathBuf)>),
    /// Put these in the Trash: undoes a creation (new folder, paste, extract...).
    Trash(Vec<PathBuf>),
}

impl UndoOp {
    /// Do it. Returns the operation that undoes what was just done.
    /// Nothing stays half done: a failure puts back what had been moved.
    pub fn apply(&self) -> Result<UndoOp, String> {
        match self {
            UndoOp::MoveBack(pairs) => {
                let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
                for (from, to) in pairs {
                    if let Err(e) = rename_checked(from, to) {
                        for (f, t) in done.iter().rev() {
                            let _ = fs::rename(t, f);
                        }
                        return Err(e);
                    }
                    done.push((from.clone(), to.clone()));
                }
                Ok(UndoOp::MoveBack(
                    done.into_iter().map(|(f, t)| (t, f)).collect(),
                ))
            }
            UndoOp::Trash(paths) => {
                let moved: Vec<(PathBuf, PathBuf)> = paths
                    .iter()
                    .filter_map(|p| trash::move_to_trash(p).map(|t| (t, p.clone())))
                    .collect();
                if moved.is_empty() && !paths.is_empty() {
                    return Err("could not move the items to the Trash".into());
                }
                Ok(UndoOp::MoveBack(moved))
            }
        }
    }
}

fn rename_checked(from: &Path, to: &Path) -> Result<(), String> {
    if fs::symlink_metadata(from).is_err() {
        return Err(format!("“{}” is not there any more", name(from)));
    }
    if fs::symlink_metadata(to).is_ok() {
        return Err(format!("“{}” already exists", name(to)));
    }
    if let Some(parent) = to.parent()
        && !parent.is_dir()
    {
        return Err(format!("the folder “{}” is gone", name(parent)));
    }
    fs::rename(from, to).map_err(|e| e.to_string())
}

fn name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

#[derive(Default)]
pub struct UndoStack {
    undo: Vec<(String, UndoOp)>,
    redo: Vec<(String, UndoOp)>,
}

impl UndoStack {
    /// Remember something that was just done (`label`: "Rename", "Move 3 items"...).
    pub fn record(&mut self, label: impl Into<String>, undoing: UndoOp) {
        self.redo.clear();
        self.undo.push((label.into(), undoing));
        if self.undo.len() > KEEP {
            self.undo.remove(0);
        }
    }

    /// Undo the last operation; returns its label.
    pub fn undo(&mut self) -> Option<Result<String, String>> {
        let (label, op) = self.undo.pop()?;
        Some(match op.apply() {
            Ok(inverse) => {
                self.redo.push((label.clone(), inverse));
                Ok(label)
            }
            Err(e) => Err(e),
        })
    }

    pub fn redo(&mut self) -> Option<Result<String, String>> {
        let (label, op) = self.redo.pop()?;
        Some(match op.apply() {
            Ok(inverse) => {
                self.undo.push((label.clone(), inverse));
                Ok(label)
            }
            Err(e) => Err(e),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("explorust-undo-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_rename_can_be_undone_and_redone() {
        let root = scratch("rename");
        fs::write(root.join("new"), "x").unwrap();
        let mut stack = UndoStack::default();
        // "old" was renamed to "new": undoing renames "new" back to "old".
        stack.record(
            "Rename",
            UndoOp::MoveBack(vec![(root.join("new"), root.join("old"))]),
        );

        assert_eq!(stack.undo().unwrap().unwrap(), "Rename");
        assert!(root.join("old").exists() && !root.join("new").exists());
        assert_eq!(stack.redo().unwrap().unwrap(), "Rename");
        assert!(root.join("new").exists() && !root.join("old").exists());
        assert!(stack.redo().is_none());
    }

    #[test]
    fn a_creation_is_undone_through_the_trash_and_comes_back() {
        let root = scratch("create");
        fs::write(root.join("made"), "x").unwrap();
        let mut stack = UndoStack::default();
        stack.record("New File", UndoOp::Trash(vec![root.join("made")]));

        stack.undo().unwrap().unwrap();
        assert!(!root.join("made").exists());
        // Redo takes it back out of the Trash.
        stack.redo().unwrap().unwrap();
        assert!(root.join("made").exists());
    }

    #[test]
    fn a_failed_undo_changes_nothing_and_says_why() {
        let root = scratch("fail");
        fs::write(root.join("a"), "a").unwrap();
        fs::write(root.join("b"), "b").unwrap();
        fs::write(root.join("taken"), "t").unwrap();
        let op = UndoOp::MoveBack(vec![
            (root.join("a"), root.join("a2")),
            // This one cannot work: the target exists.
            (root.join("b"), root.join("taken")),
        ]);
        let err = op.apply().unwrap_err();
        assert!(err.contains("taken"), "{err}");
        // The first move was put back.
        assert!(root.join("a").exists() && !root.join("a2").exists());
    }

    #[test]
    fn recording_something_new_forgets_the_redo_list() {
        let root = scratch("redo");
        fs::write(root.join("n"), "").unwrap();
        let mut stack = UndoStack::default();
        stack.record(
            "One",
            UndoOp::MoveBack(vec![(root.join("n"), root.join("o"))]),
        );
        stack.undo().unwrap().unwrap();
        stack.record("Two", UndoOp::MoveBack(vec![]));
        assert!(stack.redo().is_none());
    }

    #[test]
    fn the_history_is_bounded() {
        let mut stack = UndoStack::default();
        for i in 0..(KEEP + 10) {
            stack.record(format!("op {i}"), UndoOp::MoveBack(vec![]));
        }
        assert_eq!(stack.undo.len(), KEEP);
        assert_eq!(stack.undo.first().unwrap().0, "op 10");
    }
}
