use std::{
    fs, io,
    path::{Path, PathBuf},
};

use gpui::SharedString;

pub struct Entry {
    pub name: SharedString,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

impl Entry {
    pub fn size_label(&self) -> SharedString {
        if self.is_dir {
            "--".into()
        } else {
            format_size(self.size).into()
        }
    }
}

/// Read a directory: folders first, then files, case-insensitive order.
pub fn read_dir(path: &Path, show_hidden: bool) -> io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            // `metadata` follows symlinks, so linked folders open like folders.
            let meta = e.path().metadata().or_else(|_| e.metadata()).ok()?;
            Some(Entry {
                name: name.into(),
                path: e.path(),
                is_dir: meta.is_dir(),
                size: meta.len(),
            })
        })
        .collect();

    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));
    Ok(entries)
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Move or copy `src` into the folder `dest_dir`, returning the new path. Never overwrites: a name
/// clash gets a numbered suffix ("name 2.ext").
pub fn transfer(src: &Path, dest_dir: &Path, copy: bool) -> io::Result<PathBuf> {
    let name = src
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?;

    if dest_dir.starts_with(src) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot move a folder into itself",
        ));
    }
    // Moving something where it already is: nothing to do.
    if !copy && src.parent() == Some(dest_dir) {
        return Ok(src.to_path_buf());
    }

    let target = unique_path(dest_dir, name);
    if copy {
        copy_recursive(src, &target)?;
        return Ok(target);
    }
    match fs::rename(src, &target) {
        Ok(()) => Ok(target),
        // Different volume: copy, then remove the original.
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            copy_recursive(src, &target)?;
            if src.is_dir() {
                fs::remove_dir_all(src)?;
            } else {
                fs::remove_file(src)?;
            }
            Ok(target)
        }
        Err(e) => Err(e),
    }
}

/// Rename `path` to `new_name` (same folder). Refuses empty names, path
/// separators and names already taken by another item.
pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let invalid = |msg| io::Error::new(io::ErrorKind::InvalidInput, msg);
    if new_name.is_empty() || new_name.contains('/') {
        return Err(invalid("invalid name"));
    }
    let old_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dest = path.with_file_name(new_name);
    // The volume is case-insensitive: "a.txt" -> "A.txt" is a legit rename of the same item.
    let same_item = old_name.to_lowercase() == new_name.to_lowercase();
    if dest.exists() && !same_item {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "name taken"));
    }
    fs::rename(path, &dest)?;
    Ok(dest)
}

/// Create an empty folder named "untitled folder" (numbered if taken).
pub fn create_folder(dir: &Path) -> io::Result<PathBuf> {
    let path = unique_path(dir, std::ffi::OsStr::new("untitled folder"));
    fs::create_dir(&path)?;
    Ok(path)
}

/// Create an empty file named "untitled file" (numbered if taken).
pub fn create_file(dir: &Path) -> io::Result<PathBuf> {
    let path = unique_path(dir, std::ffi::OsStr::new("untitled file"));
    // `create_new` fails instead of truncating if something appeared meanwhile.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    Ok(path)
}

fn unique_path(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let first = dir.join(name);
    if fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let path = Path::new(name);
    let stem = path.file_stem().unwrap_or(name).to_string_lossy();
    let ext = path.extension().map(|e| e.to_string_lossy());
    (2..)
        .map(|n| match &ext {
            Some(ext) => dir.join(format!("{stem} {n}.{ext}")),
            None => dir.join(format!("{stem} {n}")),
        })
        .find(|p| fs::symlink_metadata(p).is_err())
        .expect("infinite range")
}

fn copy_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(src)?, dst)
    } else if meta.is_dir() {
        fs::create_dir(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("explorust-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn moves_file_into_folder() {
        let root = scratch("move");
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::create_dir(root.join("dir")).unwrap();
        transfer(&root.join("a.txt"), &root.join("dir"), false).unwrap();
        assert!(!root.join("a.txt").exists());
        assert!(root.join("dir/a.txt").exists());
    }

    #[test]
    fn name_clash_gets_suffix() {
        let root = scratch("clash");
        fs::create_dir(root.join("dir")).unwrap();
        fs::write(root.join("dir/a.txt"), "old").unwrap();
        fs::write(root.join("a.txt"), "new").unwrap();
        transfer(&root.join("a.txt"), &root.join("dir"), false).unwrap();
        assert_eq!(fs::read_to_string(root.join("dir/a.txt")).unwrap(), "old");
        assert_eq!(fs::read_to_string(root.join("dir/a 2.txt")).unwrap(), "new");
    }

    #[test]
    fn copies_folder_recursively() {
        let root = scratch("copy");
        fs::create_dir_all(root.join("src/sub")).unwrap();
        fs::write(root.join("src/sub/f"), "x").unwrap();
        fs::create_dir(root.join("dest")).unwrap();
        transfer(&root.join("src"), &root.join("dest"), true).unwrap();
        assert!(root.join("src/sub/f").exists());
        assert!(root.join("dest/src/sub/f").exists());
    }

    #[test]
    fn renames_and_refuses_taken_names() {
        let root = scratch("rename");
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("b.txt"), "b").unwrap();
        assert!(rename(&root.join("a.txt"), "b.txt").is_err());
        assert!(rename(&root.join("a.txt"), "x/y").is_err());
        assert!(rename(&root.join("a.txt"), "").is_err());
        assert_eq!(
            rename(&root.join("a.txt"), "c.txt").unwrap(),
            root.join("c.txt")
        );
        assert!(root.join("c.txt").exists() && !root.join("a.txt").exists());
    }

    #[test]
    fn creates_numbered_folders() {
        let root = scratch("mkdir");
        assert_eq!(create_folder(&root).unwrap(), root.join("untitled folder"));
        assert_eq!(
            create_folder(&root).unwrap(),
            root.join("untitled folder 2")
        );
    }

    #[test]
    fn creates_numbered_files() {
        let root = scratch("mkfile");
        let first = create_file(&root).unwrap();
        assert_eq!(first, root.join("untitled file"));
        assert_eq!(fs::metadata(&first).unwrap().len(), 0);
        assert_eq!(create_file(&root).unwrap(), root.join("untitled file 2"));
    }

    #[test]
    fn refuses_folder_into_itself() {
        let root = scratch("self");
        fs::create_dir_all(root.join("a/b")).unwrap();
        assert!(transfer(&root.join("a"), &root.join("a/b"), false).is_err());
    }
}
