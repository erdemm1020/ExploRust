use crate::jobs::Progress;
use std::{
    cmp::Ordering,
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use gpui::SharedString;

#[derive(Clone)]
pub struct Entry {
    pub name: SharedString,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// Space a folder takes, once found out (in the background).
    pub dir_size: Option<u64>,
    /// A date already worded (items on a server come as text).
    pub date_label: Option<String>,
    /// Lowercase name, computed once for sorting.
    lower: String,
}

impl Entry {
    /// An item of a server's folder.
    pub fn remote(path: PathBuf, name: String, is_dir: bool, size: u64, date: String) -> Self {
        Self {
            lower: name.to_lowercase(),
            name: name.into(),
            path,
            is_dir,
            size,
            modified: None,
            dir_size: None,
            date_label: Some(date),
        }
    }

    fn file_size(&self) -> u64 {
        if self.is_dir {
            self.dir_size.unwrap_or(0)
        } else {
            self.size
        }
    }

    /// Case-insensitive "name contains", `needle` already lowercase.
    pub fn matches(&self, needle: &str) -> bool {
        self.lower.contains(needle)
    }

    pub fn size_label(&self) -> SharedString {
        match (self.is_dir, self.dir_size) {
            (true, Some(bytes)) => format_size(bytes).into(),
            (true, None) => "--".into(),
            (false, _) => format_size(self.size).into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortKey {
    Name,
    Modified,
    Size,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sort {
    pub key: SortKey,
    pub ascending: bool,
    /// "file2" before "file10" instead of plain letter order.
    pub natural: bool,
    pub folders_on_top: bool,
}

impl Sort {
    /// Sorting by `key`: A to Z, but newest and biggest first, like Finder.
    pub fn by(key: SortKey) -> Self {
        Self {
            key,
            ascending: key == SortKey::Name,
            natural: true,
            folders_on_top: true,
        }
    }

    pub fn with_options(self, natural: bool, folders_on_top: bool) -> Self {
        Self {
            natural,
            folders_on_top,
            ..self
        }
    }

    /// What a click on the `key` column does: flip the direction if it is
    /// already the sort column, else switch to it.
    pub fn clicked(self, key: SortKey) -> Self {
        if self.key == key {
            Self {
                ascending: !self.ascending,
                ..self
            }
        } else {
            Self {
                key,
                ascending: key == SortKey::Name,
                ..self
            }
        }
    }
}

/// Letters in order, but runs of digits compare as numbers:
/// "file2" < "file10", and "a01" is just after "a1".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    fn digits(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        let mut run = String::new();
        while let Some(&c) = chars.peek().filter(|c| c.is_ascii_digit()) {
            run.push(c);
            chars.next();
        }
        run
    }

    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (na, nb) = (digits(&mut a), digits(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                // A longer number (without leading zeros) is a bigger number.
                let ord = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| na.len().cmp(&nb.len()));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

/// Hidden items come first (they only show when asked to), then folders (if asked);
/// the sort applies inside each group.
pub fn sort_entries(entries: &mut [Entry], sort: Sort) {
    let by_name = |a: &Entry, b: &Entry| {
        if sort.natural {
            natural_cmp(&a.lower, &b.lower)
        } else {
            a.lower.cmp(&b.lower)
        }
    };
    entries.sort_by(|a, b| {
        let hidden = is_hidden(&b.name).cmp(&is_hidden(&a.name));
        let groups = if sort.folders_on_top {
            b.is_dir.cmp(&a.is_dir)
        } else {
            Ordering::Equal
        };
        hidden.then(groups).then_with(|| {
            let primary = match sort.key {
                SortKey::Name => by_name(a, b),
                SortKey::Modified => a.modified.cmp(&b.modified),
                // Folders have no size: they count as empty, so they tie.
                SortKey::Size => a.file_size().cmp(&b.file_size()),
            };
            let primary = if sort.ascending {
                primary
            } else {
                primary.reverse()
            };
            // Ties always fall back to A to Z, whatever the direction.
            primary.then_with(|| by_name(a, b))
        })
    });
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Read a directory, sorted.
pub fn read_dir(path: &Path, show_hidden: bool, sort: Sort) -> io::Result<Vec<Entry>> {
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
                lower: name.to_lowercase(),
                name: name.into(),
                path: e.path(),
                is_dir: meta.is_dir(),
                size: meta.len(),
                modified: meta.modified().ok(),
                dir_size: None,
                date_label: None,
            })
        })
        .collect();

    sort_entries(&mut entries, sort);
    Ok(entries)
}

pub fn format_size(bytes: u64) -> String {
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
#[allow(dead_code)]
pub fn transfer(src: &Path, dest_dir: &Path, copy: bool) -> io::Result<PathBuf> {
    transfer_with(src, dest_dir, copy, &Progress::default())
}

/// Like [`transfer`], counting the bytes copied and stopping if the job is cancelled.
pub fn transfer_with(
    src: &Path,
    dest_dir: &Path,
    copy: bool,
    progress: &Progress,
) -> io::Result<PathBuf> {
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
        crate::jobs::copy_tree(src, &target, progress)?;
        return Ok(target);
    }
    match fs::rename(src, &target) {
        Ok(()) => Ok(target),
        // Different volume: copy, then remove the original.
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => {
            crate::jobs::copy_tree(src, &target, progress)?;
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

/// Number of items inside `dir`, subfolders included. Stops counting once it
/// passes `limit`, so a huge folder costs no more than a small one.
pub fn count_items(dir: &Path, limit: usize) -> usize {
    fn walk(dir: &Path, limit: usize, count: &mut usize) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            *count += 1;
            if *count > limit {
                return;
            }
            // `file_type` does not follow symlinks: no loops, no escaping the folder.
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                walk(&entry.path(), limit, count);
                if *count > limit {
                    return;
                }
            }
        }
    }
    let mut count = 0;
    walk(dir, limit, &mut count);
    count
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

pub(crate) fn unique_path(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("explorust-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_ref()).collect()
    }

    #[test]
    fn hidden_items_come_first_then_folders() {
        let root = scratch("hidden-first");
        for d in ["zdir", ".hdir"] {
            fs::create_dir(root.join(d)).unwrap();
        }
        for f in ["a.txt", ".env", ".b"] {
            fs::write(root.join(f), "x").unwrap();
        }
        let sorted = |show, sort| names(&read_dir(&root, show, sort).unwrap()).join(",");
        let name = Sort::by(SortKey::Name);
        assert_eq!(sorted(true, name), ".hdir,.b,.env,zdir,a.txt");
        // Whatever the column or direction, hidden stay on top.
        let by_size = Sort {
            ascending: false,
            ..Sort::by(SortKey::Size)
        };
        assert!(sorted(true, by_size).starts_with(".hdir,"));
        assert_eq!(
            sorted(true, name.with_options(true, false)),
            ".b,.env,.hdir,a.txt,zdir"
        );
        // Hidden off: nothing changes.
        assert_eq!(sorted(false, name), "zdir,a.txt");
    }

    #[test]
    fn sorts_by_each_column_keeping_folders_on_top() {
        let root = scratch("sort");
        fs::create_dir(root.join("zdir")).unwrap();
        fs::create_dir(root.join("Adir")).unwrap();
        fs::write(root.join("b.txt"), "12345").unwrap();
        fs::write(root.join("C.txt"), "1").unwrap();
        fs::write(root.join("a.txt"), "123").unwrap();
        // Make modification times distinct and known: C oldest, then b, then a.
        let t = |secs| std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        // Both folders share a date, so the name decides their order.
        for dir in ["zdir", "Adir"] {
            fs::File::open(root.join(dir))
                .unwrap()
                .set_modified(t(500))
                .unwrap();
        }
        for (f, secs) in [("C.txt", 1_000), ("b.txt", 2_000), ("a.txt", 3_000)] {
            fs::File::options()
                .write(true)
                .open(root.join(f))
                .unwrap()
                .set_modified(t(secs))
                .unwrap();
        }

        let by = |sort| names(&read_dir(&root, false, sort).unwrap()).join(",");
        assert_eq!(by(Sort::by(SortKey::Name)), "Adir,zdir,a.txt,b.txt,C.txt");
        assert_eq!(
            by(Sort {
                ascending: false,
                ..Sort::by(SortKey::Name)
            }),
            "zdir,Adir,C.txt,b.txt,a.txt"
        );
        // Newest first by default.
        assert_eq!(
            by(Sort::by(SortKey::Modified)),
            "Adir,zdir,a.txt,b.txt,C.txt"
        );
        assert_eq!(
            by(Sort {
                ascending: true,
                ..Sort::by(SortKey::Modified)
            }),
            "Adir,zdir,C.txt,b.txt,a.txt"
        );
        // Biggest first by default; folders stay A to Z whatever the direction.
        assert_eq!(by(Sort::by(SortKey::Size)), "Adir,zdir,b.txt,a.txt,C.txt");
        assert_eq!(
            by(Sort {
                ascending: true,
                ..Sort::by(SortKey::Size)
            }),
            "Adir,zdir,C.txt,a.txt,b.txt"
        );
    }

    #[test]
    fn natural_order_compares_numbers_by_value() {
        fn sorted(mut v: Vec<&str>) -> Vec<&str> {
            v.sort_by(|a, b| natural_cmp(a, b));
            v
        }
        assert_eq!(
            sorted(vec!["file10", "file2", "file1", "file20"]),
            ["file1", "file2", "file10", "file20"]
        );
        assert_eq!(sorted(vec!["a10b", "a9z", "a9"]), ["a9", "a9z", "a10b"]);
        assert_eq!(sorted(vec!["a01", "a1", "a"]), ["a", "a1", "a01"]);
        assert_eq!(sorted(vec!["b", "a2", "a10"]), ["a2", "a10", "b"]);
    }

    #[test]
    fn natural_and_folder_options_change_the_order() {
        let root = scratch("opts");
        fs::create_dir(root.join("dir")).unwrap();
        for f in ["f10", "f2", "a"] {
            fs::write(root.join(f), "").unwrap();
        }
        let by = |sort: Sort| names(&read_dir(&root, false, sort).unwrap()).join(",");
        let name = Sort::by(SortKey::Name);
        assert_eq!(by(name), "dir,a,f2,f10");
        assert_eq!(by(name.with_options(false, true)), "dir,a,f10,f2");
        assert_eq!(by(name.with_options(true, false)), "a,dir,f2,f10");
        // The options survive a click on another column.
        assert!(
            !name
                .with_options(false, false)
                .clicked(SortKey::Size)
                .natural
        );
    }

    #[test]
    fn clicking_a_column_flips_or_switches() {
        let s = Sort::by(SortKey::Name);
        assert!(s.ascending);
        let flipped = s.clicked(SortKey::Name);
        assert!(!flipped.ascending && flipped.key == SortKey::Name);
        let switched = flipped.clicked(SortKey::Size);
        assert_eq!(switched, Sort::by(SortKey::Size));
        assert!(!switched.ascending);
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
    fn counts_items_recursively_and_stops_at_limit() {
        let root = scratch("count");
        fs::create_dir_all(root.join("a/b")).unwrap();
        for f in ["x", "a/y", "a/b/z"] {
            fs::write(root.join(f), "").unwrap();
        }
        // x, a, a/y, a/b, a/b/z
        assert_eq!(count_items(&root, 100), 5);
        assert!(count_items(&root, 2) > 2);
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
