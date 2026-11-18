use std::path::Path;

use objc2_foundation::{NSFileManager, NSString, NSURL};

/// Move a file or folder to the macOS Trash (keeps "Put Back" working).
pub fn move_to_trash(path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };
    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trashes_a_file() {
        let file = std::env::temp_dir().join(format!("explorust-trash-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        assert!(move_to_trash(&file));
        assert!(!file.exists());
    }
}
