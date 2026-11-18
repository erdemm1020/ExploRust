use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        &[$(
            (concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")).as_slice()),
        )*]
    };
}

/// Icons embedded in the binary: no disk access at runtime.
const ICONS: &[(&str, &[u8])] = icons![
    "folder",
    "file",
    "image",
    "media",
    "archive",
    "code",
    "home",
    "desktop",
    "downloads",
    "applications",
    "sort-asc",
    "sort-desc",
    "search",
    "settings",
    "server",
    "dir-apps",
    "dir-code",
    "dir-desktop",
    "dir-docs",
    "dir-downloads",
    "dir-git",
    "dir-library",
    "dir-movies",
    "dir-music",
    "dir-packages",
    "dir-pictures",
    "doc-app",
    "doc-archive",
    "doc-audio",
    "doc-code",
    "doc-config",
    "doc-database",
    "doc-design",
    "doc-disk",
    "doc-ebook",
    "doc-font",
    "doc-image",
    "doc-log",
    "doc-markdown",
    "doc-model",
    "doc-pdf",
    "doc-secret",
    "doc-sheet",
    "doc-shell",
    "doc-slides",
    "doc-text",
    "doc-video",
    "doc-web",
    "doc-word",
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}
