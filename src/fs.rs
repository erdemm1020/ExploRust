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
