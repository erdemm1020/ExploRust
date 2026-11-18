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
    let Some(url) = file_url(file) else { return Vec::new() };
    let default = default_app(file).map(|a| a.path);
    let mut apps: Vec<AppInfo> = NSWorkspace::sharedWorkspace()
        .URLsForApplicationsToOpenURL(&url)
        .iter()
        .filter_map(|u| AppInfo::from_url(&u))
        .collect();
    apps.sort_by_cached_key(|a| (Some(&a.path) != default.as_ref(), a.name.to_lowercase()));
    apps.dedup_by(|a, b| a.path == b.path);
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
    let (Some(app_url), Some(url)) = (file_url(app), file_url(file)) else { return };
    NSWorkspace::sharedWorkspace()
        .setDefaultApplicationAtURL_toOpenContentTypeOfFileAtURL_completionHandler(
            &app_url, &url, None,
        );
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
        assert!(t.path.extension().is_some_and(|e| e == "app"), "{:?}", t.path);
        println!("terminal = {} ({})", t.name, t.path.display());
    }

    #[test]
    fn lists_apps_for_a_text_file() {
        let file = std::env::temp_dir().join(format!("explorust-apps-{}.txt", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        let apps = apps_for(&file);
        assert!(!apps.is_empty());
        println!("default = {:?}", default_app(&file).map(|a| a.name));
        println!("apps = {:?}", apps.iter().map(|a| &a.name).collect::<Vec<_>>());
        let _ = std::fs::remove_file(file);
    }
}
