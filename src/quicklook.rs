//! Quick Look: Space previews the selection in the system's preview window,
//! Space again closes it.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// Show the preview of `paths`, or close it if it is already open.
pub fn toggle(open: &mut Option<Child>, paths: &[PathBuf]) {
    if let Some(mut child) = open.take() {
        // Still running: the key closes it. Already closed by the user: open a new one.
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
    }
    if paths.is_empty() {
        return;
    }
    *open = Command::new("/usr/bin/qlmanage")
        .arg("-p")
        .args(paths)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_selected_opens_nothing() {
        let mut open = None;
        toggle(&mut open, &[]);
        assert!(open.is_none());
    }
}
