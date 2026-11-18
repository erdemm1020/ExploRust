//! What "Get Info" (⌘I) shows about an item.

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

#[derive(Clone, Debug)]
pub struct Info {
    pub name: String,
    pub path: PathBuf,
    /// "Folder", "Plain Text Document"... in the user's language.
    pub kind: String,
    pub is_dir: bool,
    /// File size; for a folder the space it takes (if it was quick to find out).
    pub size: Option<u64>,
    /// Direct children of a folder.
    pub items: Option<usize>,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub mode: u32,
    pub owner: String,
    pub link_target: Option<PathBuf>,
}

/// Look at `path`. Blocking (it runs a few small tools): call it from a background thread.
pub fn gather(path: &Path) -> Option<Info> {
    let link = fs::symlink_metadata(path).ok()?;
    let link_target = link
        .file_type()
        .is_symlink()
        .then(|| fs::read_link(path).ok())
        .flatten();
    // What a link points to is what the user cares about.
    let meta = fs::metadata(path).unwrap_or(link);
    let is_dir = meta.is_dir();

    let name = path.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    Some(Info {
        kind: kind_of(path, is_dir),
        size: if is_dir {
            crate::sizes::folder_size(path, Duration::from_secs(4))
        } else {
            Some(meta.len())
        },
        items: is_dir
            .then(|| fs::read_dir(path).map(|r| r.count()).ok())
            .flatten(),
        created: meta.created().ok(),
        modified: meta.modified().ok(),
        accessed: meta.accessed().ok(),
        mode: meta.mode() & 0o7777,
        owner: owner_of(path),
        link_target,
        is_dir,
        name,
        path: path.to_path_buf(),
    })
}

/// The kind as Spotlight words it ("PDF document"), else a plain guess.
fn kind_of(path: &Path, is_dir: bool) -> String {
    Command::new("/usr/bin/mdls")
        .args(["-name", "kMDItemKind", "-raw"])
        .arg(path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|k| !k.is_empty() && k != "(null)")
        .unwrap_or_else(|| if is_dir { "Folder" } else { "Document" }.to_string())
}

/// "user:group", names rather than numbers.
fn owner_of(path: &Path) -> String {
    Command::new("/usr/bin/stat")
        .args(["-f", "%Su:%Sg"])
        .arg(path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// "rwxr-xr--" for a Unix mode.
pub fn rwx(mode: u32) -> String {
    let bit = |b: u32, c: char| if mode & b != 0 { c } else { '-' };
    [
        bit(0o400, 'r'),
        bit(0o200, 'w'),
        bit(0o100, 'x'),
        bit(0o040, 'r'),
        bit(0o020, 'w'),
        bit(0o010, 'x'),
        bit(0o004, 'r'),
        bit(0o002, 'w'),
        bit(0o001, 'x'),
    ]
    .iter()
    .collect()
}

/// Make a file executable (where it is readable), or take the execute bits away.
pub fn toggle_executable(path: &Path) -> std::io::Result<u32> {
    let mode = fs::metadata(path)?.mode() & 0o7777;
    let new = if mode & 0o111 != 0 {
        mode & !0o111
    } else {
        // Execute where there is read: 644 becomes 755.
        mode | ((mode & 0o444) >> 2)
    };
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(new))?;
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("explorust-info-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn describes_a_file() {
        let dir = scratch("file");
        let file = dir.join("notes.txt");
        fs::write(&file, "hello").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        let info = gather(&file).unwrap();
        assert_eq!(info.name, "notes.txt");
        assert_eq!(info.size, Some(5));
        assert_eq!(info.mode, 0o640);
        assert_eq!(rwx(info.mode), "rw-r-----");
        assert!(!info.is_dir && info.items.is_none());
        assert!(info.owner.contains(':'), "{}", info.owner);
        assert!(!info.kind.is_empty());
        assert!(info.modified.is_some());
    }

    #[test]
    fn describes_a_folder_and_a_link() {
        let dir = scratch("folder");
        fs::create_dir(dir.join("d")).unwrap();
        fs::write(dir.join("d/one"), vec![0u8; 5000]).unwrap();
        fs::write(dir.join("d/two"), "x").unwrap();
        let info = gather(&dir.join("d")).unwrap();
        assert!(info.is_dir);
        assert_eq!(info.items, Some(2));
        assert!(info.size.unwrap() >= 5000);

        std::os::unix::fs::symlink("d", dir.join("link")).unwrap();
        let link = gather(&dir.join("link")).unwrap();
        assert_eq!(link.link_target, Some(PathBuf::from("d")));
        assert!(link.is_dir);
        assert!(gather(&dir.join("missing")).is_none());
    }

    #[test]
    fn permissions_read_like_ls() {
        assert_eq!(rwx(0o755), "rwxr-xr-x");
        assert_eq!(rwx(0o000), "---------");
        assert_eq!(rwx(0o600), "rw-------");
    }

    #[test]
    fn the_execute_bits_toggle() {
        let dir = scratch("exec");
        let file = dir.join("run.sh");
        fs::write(&file, "#!/bin/sh").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(toggle_executable(&file).unwrap(), 0o755);
        assert_eq!(toggle_executable(&file).unwrap(), 0o644);
        // Owner-only files stay owner-only.
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(toggle_executable(&file).unwrap(), 0o700);
    }
}
