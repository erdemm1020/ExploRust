//! Long operations (big copies, archives) that run in the background and report
//! how far they are, so the window never freezes and the user can cancel.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

/// Shared between the worker and the window.
#[derive(Default)]
pub struct Progress {
    done: AtomicU64,
    /// 0 when the size of the job is not known (archives): the bar just moves.
    total: AtomicU64,
    cancelled: AtomicBool,
    finished: AtomicBool,
}

impl Progress {
    pub fn set_total(&self, bytes: u64) {
        self.total.store(bytes, Ordering::Relaxed);
    }

    pub fn add(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn finish(&self) {
        self.finished.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    /// 0.0 to 1.0, or `None` when the total is unknown.
    pub fn fraction(&self) -> Option<f32> {
        let total = self.total.load(Ordering::Relaxed);
        (total > 0).then(|| (self.done.load(Ordering::Relaxed) as f32 / total as f32).min(1.0))
    }
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

/// Total size of the files under `paths` (symlinks count for nothing).
pub fn tree_size(paths: &[PathBuf]) -> u64 {
    fn walk(path: &Path) -> u64 {
        let Ok(meta) = fs::symlink_metadata(path) else {
            return 0;
        };
        if meta.is_dir() {
            fs::read_dir(path)
                .map(|rd| rd.flatten().map(|e| walk(&e.path())).sum())
                .unwrap_or(0)
        } else if meta.is_file() {
            meta.len()
        } else {
            0
        }
    }
    paths.iter().map(|p| walk(p)).sum()
}

/// Copy a file, a folder (recursively) or a symlink, counting bytes as they go.
/// A cancelled copy removes what it had written.
pub fn copy_tree(src: &Path, dst: &Path, progress: &Progress) -> io::Result<()> {
    let result = copy_node(src, dst, progress);
    if result.is_err() {
        let _ = fs::remove_dir_all(dst).or_else(|_| fs::remove_file(dst));
    }
    result
}

fn copy_node(src: &Path, dst: &Path, progress: &Progress) -> io::Result<()> {
    if progress.is_cancelled() {
        return Err(cancelled());
    }
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(src)?, dst)
    } else if meta.is_dir() {
        fs::create_dir(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_node(&entry.path(), &dst.join(entry.file_name()), progress)?;
        }
        fs::set_permissions(dst, meta.permissions())
    } else {
        copy_file(src, dst, &meta, progress)
    }
}

fn copy_file(src: &Path, dst: &Path, meta: &fs::Metadata, progress: &Progress) -> io::Result<()> {
    const BIG: u64 = 8 * 1024 * 1024;
    if meta.len() < BIG {
        // The system copy is the fastest, and keeps what the Finder keeps.
        fs::copy(src, dst)?;
        progress.add(meta.len());
        return Ok(());
    }
    // Big files go in chunks, so the bar moves and Cancel is quick.
    let mut from = fs::File::open(src)?;
    let mut to = fs::File::create(dst)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if progress.is_cancelled() {
            return Err(cancelled());
        }
        let n = io::Read::read(&mut from, &mut buf)?;
        if n == 0 {
            break;
        }
        io::Write::write_all(&mut to, &buf[..n])?;
        progress.add(n as u64);
    }
    fs::set_permissions(dst, meta.permissions())
}

/// Run `cmd` to its end, but stop it as soon as the job is cancelled.
/// Returns what it printed on success.
pub fn run_cancellable(cmd: &mut Command, progress: &Progress) -> Result<String, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Read the pipes on their own threads: a full pipe would block the child.
    let out = child.stdout.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = io::Read::read_to_string(&mut s, &mut text);
            text
        })
    });
    let err = child.stderr.take().map(|mut s| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = io::Read::read_to_string(&mut s, &mut text);
            text
        })
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if progress.is_cancelled() => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("cancelled".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = out.and_then(|h| h.join().ok()).unwrap_or_default();
    let err = err.and_then(|h| h.join().ok()).unwrap_or_default();
    if status.success() {
        Ok(out)
    } else {
        Err(err.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("explorust-jobs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn copies_a_tree_and_counts_the_bytes() {
        let root = scratch("copy");
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/x"), vec![1u8; 1000]).unwrap();
        fs::write(root.join("a/b/y"), vec![2u8; 500]).unwrap();
        std::os::unix::fs::symlink("x", root.join("a/link")).unwrap();

        let p = Progress::default();
        p.set_total(tree_size(&[root.join("a")]));
        assert_eq!(p.fraction(), Some(0.0));
        copy_tree(&root.join("a"), &root.join("copy"), &p).unwrap();
        assert_eq!(fs::read(root.join("copy/b/y")).unwrap().len(), 500);
        assert_eq!(
            fs::read_link(root.join("copy/link")).unwrap(),
            Path::new("x")
        );
        assert_eq!(p.fraction(), Some(1.0));
    }

    #[test]
    fn big_files_are_copied_in_chunks() {
        let root = scratch("big");
        let data = vec![7u8; 9 * 1024 * 1024 + 5];
        fs::write(root.join("big"), &data).unwrap();
        let p = Progress::default();
        copy_tree(&root.join("big"), &root.join("big2"), &p).unwrap();
        assert_eq!(fs::read(root.join("big2")).unwrap(), data);
        assert_eq!(p.done.load(Ordering::Relaxed), data.len() as u64);
    }

    #[test]
    fn a_cancelled_copy_leaves_nothing_behind() {
        let root = scratch("cancel");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("a/x"), "x").unwrap();
        let p = Progress::default();
        p.cancel();
        let err = copy_tree(&root.join("a"), &root.join("copy"), &p).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(!root.join("copy").exists());
    }

    #[test]
    fn a_command_can_be_cancelled() {
        let p = Progress::default();
        p.cancel();
        let err = run_cancellable(Command::new("/bin/sleep").arg("30"), &p).unwrap_err();
        assert_eq!(err, "cancelled");
        let ok = run_cancellable(Command::new("/bin/echo").arg("hi"), &Progress::default());
        assert_eq!(ok.unwrap().trim(), "hi");
        let bad = run_cancellable(
            Command::new("/bin/ls").arg("/no/such/dir"),
            &Progress::default(),
        );
        assert!(bad.is_err());
    }
}
