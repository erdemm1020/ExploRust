//! Talks to macOS Launch Services (through `NSWorkspace`) to find, choose and
//! open files with applications.

use std::path::{Path, PathBuf};

use objc2::rc::Retained;
use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSString, NSURL};

#[derive(Clone)]
pub struct AppInfo {
    pub name: String,
    pub path: PathBuf,
}

impl AppInfo {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_stem()?.to_string_lossy().into_owned();
        Some(Self {
            name,
            path: path.to_path_buf(),
        })
    }

    fn from_url(url: &NSURL) -> Option<Self> {
        let path = PathBuf::from(url.path()?.to_string());
        let name = path.file_stem()?.to_string_lossy().into_owned();
        Some(Self { name, path })
    }
}

fn file_url(path: &Path) -> Option<Retained<NSURL>> {
    Some(NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?)))
}

/// The app macOS would open `file` with.
pub fn default_app(file: &Path) -> Option<AppInfo> {
    let url = file_url(file)?;
    let app = NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&url)?;
    AppInfo::from_url(&app)
}

/// Every app able to open `file`, default first, then by name.
pub fn apps_for(file: &Path) -> Vec<AppInfo> {
    let Some(url) = file_url(file) else {
        return Vec::new();
    };
    let default = default_app(file).map(|a| a.path);
    let mut apps: Vec<AppInfo> = NSWorkspace::sharedWorkspace()
        .URLsForApplicationsToOpenURL(&url)
        .iter()
        .filter_map(|u| AppInfo::from_url(&u))
        .collect();
    apps.sort_by_cached_key(|a| (Some(&a.path) != default.as_ref(), a.name.to_lowercase()));
    // Same-named copies (e.g. several Chrome builds) would just clutter the list.
    apps.dedup_by(|a, b| a.name == b.name);
    apps
}

/// Open `paths` with a specific app.
pub fn open_with(paths: &[PathBuf], app: &Path) {
    let urls: Vec<Retained<NSURL>> = paths.iter().filter_map(|p| file_url(p)).collect();
    let Some(app_url) = file_url(app) else { return };
    let urls = NSArray::from_retained_slice(&urls);
    NSWorkspace::sharedWorkspace().openURLs_withApplicationAtURL_configuration_completionHandler(
        &urls,
        &app_url,
        &NSWorkspaceOpenConfiguration::configuration(),
        None,
    );
}

/// Make `app` the default for files of the same type as `file`.
/// macOS may ask the user to confirm.
pub fn set_default(app: &Path, file: &Path) {
    let (Some(app_url), Some(url)) = (file_url(app), file_url(file)) else {
        return;
    };
    NSWorkspace::sharedWorkspace()
        .setDefaultApplicationAtURL_toOpenContentTypeOfFileAtURL_completionHandler(
            &app_url, &url, None,
        );
}

/// Terminal apps we look for, by bundle identifier.
const KNOWN_TERMINALS: &[&str] = &[
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.Warp-Stable",
    "org.alacritty",
    "net.kovidgoyal.kitty",
    "com.github.wez.wezterm",
    "com.raphaelamorim.rio",
    "co.zeit.hyper",
    "org.tabby",
    "dev.commandline.waveterm",
];

/// Code editors we look for, by bundle identifier, in order of preference.
const KNOWN_EDITORS: &[&str] = &[
    "com.microsoft.VSCode",
    "dev.zed.Zed",
    "com.todesktop.230313mzl4w4u92", // Cursor
    "com.exafunction.windsurf",
    "com.vscodium",
    "com.microsoft.VSCodeInsiders",
    "dev.zed.Zed-Preview",
    "com.sublimetext.4",
    "com.panic.Nova",
];

/// The apps among `ids` that are installed on this Mac, in the same order.
fn installed(ids: &[&str]) -> Vec<AppInfo> {
    let workspace = NSWorkspace::sharedWorkspace();
    ids.iter()
        .filter_map(|id| workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(id)))
        .filter_map(|url| AppInfo::from_url(&url))
        .collect()
}

/// Well-known terminal apps that are installed on this Mac.
pub fn installed_terminals() -> Vec<AppInfo> {
    installed(KNOWN_TERMINALS)
}

/// Well-known code editors (VS Code, Zed, Cursor...) that are installed on this Mac.
pub fn installed_editors() -> Vec<AppInfo> {
    installed(KNOWN_EDITORS)
}

/// The Mac's terminal: whatever opens `.command` files (Terminal.app unless
/// the user picked another one, e.g. Ghostty or iTerm).
pub fn terminal() -> AppInfo {
    default_app(Path::new("/tmp/explorust.command")).unwrap_or_else(|| AppInfo {
        name: "Terminal".into(),
        path: "/System/Applications/Utilities/Terminal.app".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_is_an_app_bundle() {
        let t = terminal();
        assert!(
            t.path.extension().is_some_and(|e| e == "app"),
            "{:?}",
            t.path
        );
        println!("terminal = {} ({})", t.name, t.path.display());
    }

    #[test]
    fn finds_terminal_app_installed() {
        // Terminal.app ships with macOS.
        let found = installed_terminals();
        assert!(
            found.iter().any(|a| a.name == "Terminal"),
            "{:?}",
            found.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
        println!(
            "terminals = {:?}",
            found.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn editors_are_listed_without_duplicates() {
        let found = installed_editors();
        println!(
            "editors = {:?}",
            found.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
        let mut paths: Vec<_> = found.iter().map(|a| &a.path).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), found.len());
        // Ids are unique and not shared with the terminals.
        for id in KNOWN_EDITORS {
            assert!(!KNOWN_TERMINALS.contains(id));
        }
        assert_eq!(
            KNOWN_EDITORS
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            KNOWN_EDITORS.len()
        );
    }

    #[test]
    fn lists_apps_for_a_text_file() {
        let file = std::env::temp_dir().join(format!("explorust-apps-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        let apps = apps_for(&file);
        assert!(!apps.is_empty());
        println!("default = {:?}", default_app(&file).map(|a| a.name));
        println!(
            "apps = {:?}",
            apps.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
        let _ = std::fs::remove_file(file);
    }
}
