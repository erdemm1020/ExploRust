//! Which icon (and color) an item gets. "Simple" keeps the original handful of grey
//! icons; "Advanced" tells types apart: a red PDF, a green spreadsheet, code colored by
//! language, folders with a glyph for Downloads, Pictures, Music...

use crate::config::IconStyle;

/// What to draw for an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub asset: &'static str,
    /// `0xRRGGBB`, or `None` for the theme's folder / file color.
    pub color: Option<u32>,
}

const fn look(asset: &'static str, color: u32) -> Look {
    Look {
        asset,
        color: Some(color),
    }
}

const fn plain(asset: &'static str) -> Look {
    Look { asset, color: None }
}

/// Every icon this module can ask for (so a test can check they all exist).
#[cfg(test)]
pub const ASSETS: &[&str] = &[
    "icons/folder.svg",
    "icons/file.svg",
    "icons/image.svg",
    "icons/media.svg",
    "icons/archive.svg",
    "icons/code.svg",
    "icons/doc-text.svg",
    "icons/doc-markdown.svg",
    "icons/doc-pdf.svg",
    "icons/doc-word.svg",
    "icons/doc-sheet.svg",
    "icons/doc-slides.svg",
    "icons/doc-image.svg",
    "icons/doc-audio.svg",
    "icons/doc-video.svg",
    "icons/doc-archive.svg",
    "icons/doc-disk.svg",
    "icons/doc-app.svg",
    "icons/doc-font.svg",
    "icons/doc-database.svg",
    "icons/doc-config.svg",
    "icons/doc-log.svg",
    "icons/doc-shell.svg",
    "icons/doc-secret.svg",
    "icons/doc-web.svg",
    "icons/doc-code.svg",
    "icons/doc-ebook.svg",
    "icons/doc-design.svg",
    "icons/doc-model.svg",
    "icons/dir-docs.svg",
    "icons/dir-downloads.svg",
    "icons/dir-pictures.svg",
    "icons/dir-music.svg",
    "icons/dir-movies.svg",
    "icons/dir-apps.svg",
    "icons/dir-library.svg",
    "icons/dir-code.svg",
    "icons/dir-git.svg",
    "icons/dir-packages.svg",
    "icons/dir-desktop.svg",
];

pub fn for_item(name: &str, is_dir: bool, style: IconStyle) -> Look {
    match style {
        IconStyle::Simple => simple(name, is_dir),
        IconStyle::Advanced => advanced(name, is_dir),
    }
}

fn extension(name: &str) -> String {
    match name.rsplit_once('.') {
        // ".gitignore" has no extension; "a.tar.gz" ends in "gz".
        Some((stem, ext)) if !stem.is_empty() => ext.to_lowercase(),
        _ => String::new(),
    }
}

/// The original icons: a folder, a document, and four kinds of file.
fn simple(name: &str, is_dir: bool) -> Look {
    if is_dir {
        return plain("icons/folder.svg");
    }
    plain(match extension(name).as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "svg" | "bmp" | "tiff" | "ico" => {
            "icons/image.svg"
        }
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "mp4" | "mov" | "mkv" | "avi" | "webm" => {
            "icons/media.svg"
        }
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "dmg" | "pkg" => {
            "icons/archive.svg"
        }
        "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "go" | "c" | "h" | "cpp" | "java" | "swift"
        | "sh" | "json" | "toml" | "yaml" | "yml" | "html" | "css" | "kt" | "rb" => {
            "icons/code.svg"
        }
        _ => "icons/file.svg",
    })
}

const GREY: u32 = 0x8e8e93;

fn advanced(name: &str, is_dir: bool) -> Look {
    let lower = name.to_lowercase();
    if is_dir {
        return folder(&lower);
    }
    // Names that say more than their extension.
    if lower.starts_with(".env")
        || matches!(
            lower.as_str(),
            "id_rsa" | "id_ed25519" | "id_ecdsa" | ".netrc" | ".npmrc"
        )
    {
        return look("icons/doc-secret.svg", 0xd9951a);
    }
    match lower.as_str() {
        "dockerfile" | "containerfile" => return look("icons/doc-code.svg", 0x2496ed),
        "makefile" | "cmakelists.txt" | "justfile" => return look("icons/doc-config.svg", GREY),
        "readme" | "license" | "licence" | "changelog" | "authors" | "notice" => {
            return look("icons/doc-text.svg", GREY);
        }
        _ => {}
    }
    let ext = extension(name);
    let doc = |asset: &'static str, color: u32| look(asset, color);
    match ext.as_str() {
        "txt" | "rtf" | "text" | "nfo" => doc("icons/doc-text.svg", GREY),
        "md" | "markdown" | "mdx" | "rst" => doc("icons/doc-markdown.svg", 0x7c83fd),
        "pdf" => doc("icons/doc-pdf.svg", 0xe5484d),
        "doc" | "docx" | "odt" | "pages" => doc("icons/doc-word.svg", 0x2b6cd4),
        "xls" | "xlsx" | "csv" | "tsv" | "numbers" | "ods" | "parquet" => {
            doc("icons/doc-sheet.svg", 0x2fa84f)
        }
        "ppt" | "pptx" | "key" | "odp" => doc("icons/doc-slides.svg", 0xe87d2f),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tiff" | "tif"
        | "ico" | "svg" | "avif" | "raw" | "cr2" | "nef" | "arw" => {
            doc("icons/doc-image.svg", 0x1fb5a3)
        }
        "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "aiff" | "aif" => {
            doc("icons/doc-audio.svg", 0xff5c8a)
        }
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" | "flv" | "wmv" => {
            doc("icons/doc-video.svg", 0x8e5cf0)
        }
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "7z" | "rar" | "zst" => {
            doc("icons/doc-archive.svg", 0xb7791f)
        }
        "dmg" | "iso" | "img" | "pkg" => doc("icons/doc-disk.svg", 0x6b7a90),
        "app" | "exe" | "msi" | "apk" | "ipa" => doc("icons/doc-app.svg", 0x0a84ff),
        "ttf" | "otf" | "woff" | "woff2" | "ttc" => doc("icons/doc-font.svg", 0x7a7a85),
        "db" | "sqlite" | "sqlite3" | "sql" | "mdb" => doc("icons/doc-database.svg", 0x5b8def),
        "toml" | "yaml" | "yml" | "ini" | "conf" | "cfg" | "plist" | "properties" | "lock" => {
            doc("icons/doc-config.svg", GREY)
        }
        "log" => doc("icons/doc-log.svg", 0xa0a0a8),
        "sh" | "zsh" | "bash" | "fish" | "command" | "ps1" | "bat" => {
            doc("icons/doc-shell.svg", 0x34c759)
        }
        "pem" | "p12" | "pfx" | "cer" | "crt" | "kdbx" | "keychain" | "gpg" | "asc" => {
            doc("icons/doc-secret.svg", 0xd9951a)
        }
        "webloc" | "url" => doc("icons/doc-web.svg", 0x3b82f6),
        "epub" | "mobi" | "azw3" => doc("icons/doc-ebook.svg", 0xc2410c),
        "psd" | "ai" | "sketch" | "fig" | "xd" | "afdesign" => {
            doc("icons/doc-design.svg", 0xe879f9)
        }
        "obj" | "stl" | "fbx" | "blend" | "3mf" | "usdz" | "gltf" | "glb" => {
            doc("icons/doc-model.svg", 0x14b8a6)
        }
        _ => match language_color(&ext) {
            Some(color) => doc("icons/doc-code.svg", color),
            None => plain("icons/file.svg"),
        },
    }
}

/// The usual color of a language, for its source files.
fn language_color(ext: &str) -> Option<u32> {
    Some(match ext {
        "rs" => 0xe8743b,
        "js" | "mjs" | "cjs" | "jsx" => 0xe0b020,
        "ts" | "tsx" => 0x3178c6,
        "py" | "pyw" | "ipynb" => 0x3b7bbf,
        "go" => 0x00add8,
        "swift" => 0xf05138,
        "java" => 0xe76f00,
        "kt" | "kts" => 0x7f52ff,
        "c" | "h" => 0x5c6bc0,
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "m" | "mm" => 0x2f7fc1,
        "cs" => 0x8a4fb5,
        "rb" => 0xcc342d,
        "php" => 0x777bb4,
        "html" | "htm" => 0xe44d26,
        "css" | "scss" | "sass" | "less" => 0x2965f1,
        "json" | "jsonc" | "json5" => 0xc9a227,
        "xml" | "xsl" | "xib" | "storyboard" => 0x5a8f5a,
        "lua" => 0x4b6bff,
        "r" => 0x276dc3,
        "dart" => 0x0175c2,
        "zig" => 0xf7a41d,
        "vue" => 0x41b883,
        "svelte" => 0xff3e00,
        "ex" | "exs" => 0x8a5fa0,
        "hs" => 0x7a6bb0,
        "scala" => 0xdc322f,
        "pl" | "pm" => 0x6a7bd6,
        "graphql" | "gql" => 0xe535ab,
        _ => return None,
    })
}

/// Folders that deserve a glyph.
fn folder(lower: &str) -> Look {
    let named = |asset: &'static str| plain(asset);
    if lower.ends_with(".app") {
        return look("icons/doc-app.svg", 0x0a84ff);
    }
    match lower {
        "desktop" => named("icons/dir-desktop.svg"),
        "documents" | "docs" | "doc" | "notes" => named("icons/dir-docs.svg"),
        "downloads" => named("icons/dir-downloads.svg"),
        "pictures" | "photos" | "images" | "img" | "screenshots" => named("icons/dir-pictures.svg"),
        "music" | "audio" | "sounds" => named("icons/dir-music.svg"),
        "movies" | "videos" | "video" => named("icons/dir-movies.svg"),
        "applications" | "apps" => named("icons/dir-apps.svg"),
        "library" | "lib" | "libs" => named("icons/dir-library.svg"),
        "developer" | "code" | "src" | "source" | "sources" | "projects" | "repos" | "dev"
        | "git" => named("icons/dir-code.svg"),
        ".git" | ".github" | ".gitlab" => look("icons/dir-git.svg", 0xe8743b),
        // Folders full of things nobody reads: faded.
        "node_modules" | "target" | "dist" | "build" | "vendor" | "venv" | ".venv"
        | "__pycache__" | "pods" | ".build" | "out" | ".next" | ".cache" => {
            look("icons/dir-packages.svg", GREY)
        }
        _ => plain("icons/folder.svg"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Assets;
    use gpui::AssetSource;

    fn adv(name: &str, dir: bool) -> Look {
        for_item(name, dir, IconStyle::Advanced)
    }

    #[test]
    fn every_icon_that_can_be_asked_for_is_embedded_and_valid() {
        for path in ASSETS {
            let bytes = Assets
                .load(path)
                .unwrap()
                .unwrap_or_else(|| panic!("{path} is not embedded"));
            let text = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(
                text.starts_with("<svg") && text.contains("</svg>"),
                "{path}"
            );
            assert!(text.contains("currentColor"), "{path} cannot be tinted");
        }
    }

    #[test]
    fn advanced_icons_tell_types_apart() {
        assert_eq!(adv("report.PDF", false).asset, "icons/doc-pdf.svg");
        assert_eq!(adv("budget.xlsx", false).asset, "icons/doc-sheet.svg");
        assert_eq!(adv("notes.md", false).asset, "icons/doc-markdown.svg");
        assert_eq!(adv("photo.HEIC", false).asset, "icons/doc-image.svg");
        assert_eq!(adv("deploy.sh", false).asset, "icons/doc-shell.svg");
        assert_eq!(adv("archive.tar.gz", false).asset, "icons/doc-archive.svg");
        // Code keeps one glyph and takes the color of its language.
        let (rs, py) = (adv("main.rs", false), adv("app.py", false));
        assert_eq!(
            (rs.asset, py.asset),
            ("icons/doc-code.svg", "icons/doc-code.svg")
        );
        assert_ne!(rs.color, py.color);
        assert_eq!(adv("a.xyz", false), plain("icons/file.svg"));
        assert_eq!(adv("noextension", false), plain("icons/file.svg"));
    }

    #[test]
    fn secrets_and_special_names_are_recognized() {
        assert_eq!(adv(".env", false).asset, "icons/doc-secret.svg");
        assert_eq!(adv(".env.local", false).asset, "icons/doc-secret.svg");
        assert_eq!(adv("id_ed25519", false).asset, "icons/doc-secret.svg");
        assert_eq!(adv("Dockerfile", false).asset, "icons/doc-code.svg");
        assert_eq!(adv("Makefile", false).asset, "icons/doc-config.svg");
        // A dot file with no extension is not "gitignore" the extension.
        assert_eq!(adv(".gitignore", false), plain("icons/file.svg"));
    }

    #[test]
    fn well_known_folders_get_their_glyph() {
        assert_eq!(adv("Downloads", true).asset, "icons/dir-downloads.svg");
        assert_eq!(adv("Pictures", true).asset, "icons/dir-pictures.svg");
        assert_eq!(adv("src", true).asset, "icons/dir-code.svg");
        assert_eq!(adv(".git", true).asset, "icons/dir-git.svg");
        assert_eq!(adv("node_modules", true).color, Some(GREY));
        assert_eq!(adv("Safari.app", true).asset, "icons/doc-app.svg");
        assert_eq!(adv("holiday", true), plain("icons/folder.svg"));
    }

    #[test]
    fn simple_icons_are_the_original_ones_whatever_the_file() {
        let simple = |n: &str, d: bool| for_item(n, d, IconStyle::Simple);
        assert_eq!(simple("Downloads", true), plain("icons/folder.svg"));
        assert_eq!(simple("a.png", false), plain("icons/image.svg"));
        assert_eq!(simple("a.mp4", false), plain("icons/media.svg"));
        assert_eq!(simple("a.zip", false), plain("icons/archive.svg"));
        assert_eq!(simple("a.rs", false), plain("icons/code.svg"));
        assert_eq!(simple("a.pdf", false), plain("icons/file.svg"));
        assert!(simple("a.pdf", false).color.is_none());
    }
}
