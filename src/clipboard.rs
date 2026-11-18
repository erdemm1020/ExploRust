//! Files on the system pasteboard, so copy/paste works with the real Finder
//! and any other app, both ways.

use std::path::{Path, PathBuf};

use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSString, NSURL};

/// Put `paths` on the general pasteboard (replacing its content).
pub fn copy_files(paths: &[PathBuf]) {
    write_to(&NSPasteboard::generalPasteboard(), paths);
}

/// Files currently on the general pasteboard (copied here or in another app).
pub fn files() -> Vec<PathBuf> {
    read_from(&NSPasteboard::generalPasteboard())
}

fn write_to(pasteboard: &NSPasteboard, paths: &[PathBuf]) {
    let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = paths
        .iter()
        .filter_map(|p| Some(NSURL::fileURLWithPath(&NSString::from_str(p.to_str()?))))
        .map(ProtocolObject::from_retained)
        .collect();
    pasteboard.clearContents();
    pasteboard.writeObjects(&NSArray::from_retained_slice(&urls));
}

fn read_from(pasteboard: &NSPasteboard) -> Vec<PathBuf> {
    let Some(items) = pasteboard.pasteboardItems() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            // Each file is a "file:///..." URL string.
            let text = item.stringForType(unsafe { NSPasteboardTypeFileURL })?;
            let url = NSURL::URLWithString(&text)?;
            Some(Path::new(&url.path()?.to_string()).to_path_buf())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_round_trip_through_a_private_pasteboard() {
        // A private pasteboard: the test must not touch the user's clipboard.
        let pb = NSPasteboard::pasteboardWithUniqueName();
        let paths: Vec<PathBuf> = vec!["/tmp/a b.txt".into(), "/tmp/dir (1)/c".into()];
        write_to(&pb, &paths);
        assert_eq!(read_from(&pb), paths);
    }

    #[test]
    fn empty_pasteboard_has_no_files() {
        assert!(read_from(&NSPasteboard::pasteboardWithUniqueName()).is_empty());
    }
}
