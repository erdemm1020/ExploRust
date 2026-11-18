//! Zip and unzip with the tools macOS already has (`ditto`, `zip`, `tar`).

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    fs::unique_path,
    jobs::{Progress, run_cancellable},
};

/// Suffixes we know how to open, longest first.
const SUFFIXES: &[&str] = &[".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".zip", ".tar"];

pub fn is_archive(path: &Path) -> bool {
    suffix(path).is_some()
}

fn suffix(path: &Path) -> Option<&'static str> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    SUFFIXES.iter().copied().find(|s| name.ends_with(s))
}

/// The name without its archive suffix: "photos.tar.gz" -> "photos".
fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    match suffix(path) {
        Some(s) => name[..name.len() - s.len()].to_string(),
        None => name,
    }
}

/// Zip `items` (all in `dir`) into `dir`: "<name>.zip" for one item, else "Archive.zip".
pub fn compress(items: &[PathBuf], dir: &Path, progress: &Progress) -> Result<PathBuf, String> {
    let first = items.first().ok_or("nothing to compress")?;
    let name = match items {
        [one] => format!(
            "{}.zip",
            one.file_name().unwrap_or_default().to_string_lossy()
        ),
        _ => "Archive.zip".to_string(),
    };
    let target = unique_path(dir, OsStr::new(&name));

    let mut cmd = if let [one] = items {
        // What the Finder does: keeps the folder name, Finder metadata and symlinks.
        let mut c = Command::new("/usr/bin/ditto");
        c.args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(one)
            .arg(&target);
        c
    } else {
        let mut c = Command::new("/usr/bin/zip");
        c.current_dir(dir).args(["-r", "-q", "-y"]).arg(&target);
        for item in items {
            c.arg(item.file_name().ok_or("bad name")?);
        }
        c
    };
    let _ = first;
    match run_cancellable(&mut cmd, progress) {
        Ok(_) => Ok(target),
        Err(e) => {
            let _ = fs::remove_file(&target);
            Err(e)
        }
    }
}

/// Unpack `archive` next to itself. A single item inside is put there as is;
/// several go into a folder named after the archive. Returns what was created.
pub fn extract(archive: &Path, progress: &Progress) -> Result<PathBuf, String> {
    let dir = archive.parent().ok_or("no folder")?;
    let scratch = unique_path(dir, OsStr::new(".explorust-extract"));
    fs::create_dir(&scratch).map_err(|e| e.to_string())?;

    let result =
        unpack_into(archive, &scratch, progress).and_then(|()| place(archive, dir, &scratch));
    // Whatever happened, the scratch folder goes away (it is empty after a success).
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn unpack_into(archive: &Path, scratch: &Path, progress: &Progress) -> Result<(), String> {
    let mut cmd = if suffix(archive) == Some(".zip") {
        let mut c = Command::new("/usr/bin/ditto");
        c.args(["-x", "-k"]).arg(archive).arg(scratch);
        c
    } else {
        // bsdtar refuses absolute paths and `..` by default.
        let mut c = Command::new("/usr/bin/tar");
        c.arg("-xf").arg(archive).arg("-C").arg(scratch);
        c
    };
    run_cancellable(&mut cmd, progress).map(|_| ())
}

fn place(archive: &Path, dir: &Path, scratch: &Path) -> Result<PathBuf, String> {
    let _ = fs::remove_dir_all(scratch.join("__MACOSX"));
    let items: Vec<PathBuf> = fs::read_dir(scratch)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .collect();
    match items.as_slice() {
        [] => Err("the archive is empty".into()),
        [one] => {
            let target = unique_path(dir, one.file_name().unwrap_or_default());
            fs::rename(one, &target).map_err(|e| e.to_string())?;
            Ok(target)
        }
        _ => {
            let target = unique_path(dir, OsStr::new(&stem(archive)));
            fs::create_dir(&target).map_err(|e| e.to_string())?;
            for item in items {
                fs::rename(&item, target.join(item.file_name().unwrap_or_default()))
                    .map_err(|e| e.to_string())?;
            }
            Ok(target)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("explorust-zip-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn knows_archive_names() {
        for yes in [
            "a.zip",
            "A.ZIP",
            "b.tar.gz",
            "c.tgz",
            "d.tar",
            "e.tar.bz2",
            "f.tar.xz",
        ] {
            assert!(is_archive(Path::new(yes)), "{yes}");
        }
        for no in ["a.txt", "zip", "b.gz", "c.tar.gz.bak"] {
            assert!(!is_archive(Path::new(no)), "{no}");
        }
        assert_eq!(stem(Path::new("/x/photos.tar.gz")), "photos");
        assert_eq!(stem(Path::new("/x/Docs.ZIP")), "Docs");
    }

    #[test]
    fn a_folder_round_trips_through_a_zip() {
        let root = scratch("folder");
        fs::create_dir_all(root.join("proj/src")).unwrap();
        fs::write(root.join("proj/src/main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("proj/readme"), "hi").unwrap();

        let zip = compress(&[root.join("proj")], &root, &Progress::default()).unwrap();
        assert_eq!(zip, root.join("proj.zip"));

        fs::rename(root.join("proj"), root.join("proj-original")).unwrap();
        // "proj" holds one top-level item (the folder itself): it comes back as "proj".
        let out = extract(&zip, &Progress::default()).unwrap();
        assert_eq!(out, root.join("proj"));
        assert_eq!(
            fs::read_to_string(root.join("proj/src/main.rs")).unwrap(),
            "fn main() {}"
        );
        // No scratch folder is left behind.
        let hidden: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(".explorust"))
            .collect();
        assert!(hidden.is_empty());
    }

    #[test]
    fn several_items_zip_together_and_unpack_into_a_named_folder() {
        let root = scratch("many");
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("b.txt"), "b").unwrap();
        let zip = compress(
            &[root.join("a.txt"), root.join("b.txt")],
            &root,
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(zip, root.join("Archive.zip"));
        // A second archive does not overwrite the first.
        let zip2 = compress(
            &[root.join("a.txt"), root.join("b.txt")],
            &root,
            &Progress::default(),
        )
        .unwrap();
        assert_eq!(zip2, root.join("Archive 2.zip"));

        let out = extract(&zip, &Progress::default()).unwrap();
        assert_eq!(out, root.join("Archive"));
        assert_eq!(fs::read_to_string(root.join("Archive/b.txt")).unwrap(), "b");
    }

    #[test]
    fn a_tar_gz_is_unpacked_too() {
        let root = scratch("tar");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/one"), "1").unwrap();
        let status = Command::new("/usr/bin/tar")
            .current_dir(&root)
            .args(["-czf", "src.tar.gz", "src"])
            .status()
            .unwrap();
        assert!(status.success());
        fs::remove_dir_all(root.join("src")).unwrap();
        let out = extract(&root.join("src.tar.gz"), &Progress::default()).unwrap();
        assert_eq!(out, root.join("src"));
        assert_eq!(fs::read_to_string(root.join("src/one")).unwrap(), "1");
    }

    #[test]
    fn an_archive_cannot_write_outside_its_folder() {
        let root = scratch("slip");
        let inner = root.join("in");
        fs::create_dir_all(&inner).unwrap();
        // A zip whose entry climbs out of the extraction folder.
        let script = "import zipfile,sys; z=zipfile.ZipFile(sys.argv[1],'w'); z.writestr('../../evil.txt','x'); z.writestr('ok.txt','y'); z.close()";
        let made = Command::new("/usr/bin/python3")
            .args(["-c", script])
            .arg(inner.join("bad.zip"))
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return; // no python3 here: nothing to check
        }
        let _ = extract(&inner.join("bad.zip"), &Progress::default());
        assert!(!root.join("evil.txt").exists());
        assert!(!inner.join("evil.txt").exists());
        assert!(!root.parent().unwrap().join("evil.txt").exists());
    }

    #[test]
    fn a_cancelled_zip_leaves_no_file() {
        let root = scratch("cancel");
        fs::write(root.join("a"), "a").unwrap();
        let p = Progress::default();
        p.cancel();
        assert!(compress(&[root.join("a")], &root, &p).is_err());
        assert!(!root.join("a.zip").exists());
    }
}
