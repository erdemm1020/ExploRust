//! How much space a folder takes, asked to `du` (fast, native, and stoppable).

use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Disk space used by everything under `dir`, in bytes. `None` if it takes longer than
/// `limit` (a huge folder) or `du` fails. Stays on one volume (`-x`).
pub fn folder_size(dir: &Path, limit: Duration) -> Option<u64> {
    let mut child = Command::new("/usr/bin/du")
        .args(["-skx"])
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(_) => break,
            None if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    let out = child.wait_with_output().ok()?;
    // "<kilobytes>\t<path>" (a folder we cannot read still prints a size, with errors on stderr).
    let kb: u64 = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kb * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn measures_a_folder() {
        let dir = std::env::temp_dir().join(format!("explorust-du-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a"), vec![0u8; 1_000_000]).unwrap();
        fs::write(dir.join("sub/b"), vec![1u8; 2_000_000]).unwrap();
        let size = folder_size(&dir, Duration::from_secs(5)).unwrap();
        assert!((3_000_000..3_400_000).contains(&size), "{size}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_folder_has_no_size() {
        assert_eq!(
            folder_size(Path::new("/no/such/folder/here"), Duration::from_secs(2)),
            None
        );
    }

    #[test]
    fn a_slow_scan_is_given_up() {
        // A zero time limit always gives up (unless du is instant): never hangs.
        let _ = folder_size(Path::new("/usr"), Duration::from_millis(1));
    }
}
