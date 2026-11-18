use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2_foundation::{NSFileManager, NSString, NSURL};

/// Move a file or folder to the macOS Trash (keeps "Put Back" working).
/// Returns where it ended up in the Trash, so the move can be undone.
pub fn move_to_trash(path: &Path) -> Option<PathBuf> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
    let mut trashed: Option<Retained<NSURL>> = None;
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, Some(&mut trashed))
        .ok()?;
    Some(PathBuf::from(trashed?.path()?.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trashes_a_file() {
        let file = std::env::temp_dir().join(format!("explorust-trash-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        let trashed = move_to_trash(&file).expect("moved to the Trash");
        assert!(!file.exists());
        assert!(trashed.exists(), "{trashed:?}");
        let _ = std::fs::remove_file(trashed);
    }
}
