//! Self-update from GitHub Releases: find a newer release, download its DMG,
//! check it, then swap the installed app for the new one.
//!
//! Only macOS tools are used (`curl`, `shasum`, `hdiutil`, `ditto`, `codesign`):
//! no HTTP library, nothing extra to ship.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::Value;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "erdemm1020/ExploRust";
const APP_NAME: &str = "ExploRust.app";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// "0.2.0" (no leading "v").
    pub version: String,
    pub notes: String,
    /// The release page, for when an automatic install is not possible.
    pub page_url: String,
    pub dmg_url: String,
    pub checksum_url: String,
}

/// Where to ask for the newest release. Debug builds can point elsewhere
/// (`EXPLORUST_UPDATE_URL`) to test the whole flow against a local server.
fn latest_release_url() -> String {
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("EXPLORUST_UPDATE_URL") {
        return url;
    }
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

/// `Ok(None)`: this is already the latest version.
pub fn check() -> Result<Option<Release>, String> {
    let json = http_get(&latest_release_url())?;
    let release = parse_release(&json)?;
    Ok(is_newer(&release.version, VERSION).then_some(release))
}

/// Download, verify and install `release` over the running app.
/// Returns the path of the new app, ready to be relaunched.
pub fn install(release: &Release) -> Result<PathBuf, String> {
    let app = bundle_of(&std::env::current_exe().map_err(|e| e.to_string())?)
        .ok_or("ExploRust is not running from an app bundle, so it cannot replace itself")?;
    let parent = app.parent().ok_or("the app has no parent folder")?;

    let work = std::env::temp_dir().join(format!("explorust-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = install_in(release, &app, parent, &work);
    let _ = fs::remove_dir_all(&work);
    result
}

fn install_in(
    release: &Release,
    app: &Path,
    parent: &Path,
    work: &Path,
) -> Result<PathBuf, String> {
    // 1. Download the DMG and check it against the published checksum.
    let dmg = work.join("update.dmg");
    download(&release.dmg_url, &dmg)?;
    let expected = parse_checksum(&http_get(&release.checksum_url)?)
        .ok_or("the release checksum file is not valid")?;
    let actual = sha256_of(&dmg)?;
    if actual != expected {
        return Err("the download is corrupted (checksum mismatch)".into());
    }

    // 2. Open it, and copy the new app next to the old one (same volume: the swap is a rename).
    let mount = Mount::attach(&dmg, &work.join("mnt"))?;
    let new_app = mount.path().join(APP_NAME);
    if !new_app.is_dir() {
        return Err(format!("{APP_NAME} is missing from the DMG"));
    }
    let staged = parent.join(".ExploRust-update.app");
    let _ = fs::remove_dir_all(&staged);
    run(
        Command::new("/usr/bin/ditto").arg(&new_app).arg(&staged),
        "copying the new version (is the app folder writable?)",
    )?;
    drop(mount);

    // 3. Check what we are about to run.
    let staged_version = run(
        Command::new("/usr/libexec/PlistBuddy")
            .arg("-c")
            .arg("Print :CFBundleShortVersionString")
            .arg(staged.join("Contents/Info.plist")),
        "reading the new version",
    )?;
    if staged_version.trim() != release.version {
        let _ = fs::remove_dir_all(&staged);
        return Err(format!(
            "the DMG holds version {} instead of {}",
            staged_version.trim(),
            release.version
        ));
    }
    if let Err(e) = run(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&staged),
        "checking the signature",
    ) {
        let _ = fs::remove_dir_all(&staged);
        return Err(e);
    }

    // 4. Swap. The old app goes aside first, and comes back if anything fails.
    let backup = parent.join(".ExploRust-old.app");
    let _ = fs::remove_dir_all(&backup);
    fs::rename(app, &backup).map_err(|e| format!("could not move the old app: {e}"))?;
    if let Err(e) = fs::rename(&staged, app) {
        let _ = fs::rename(&backup, app);
        let _ = fs::remove_dir_all(&staged);
        return Err(format!("could not install the new app: {e}"));
    }
    // The running process keeps working: its files stay until it exits.
    let _ = fs::remove_dir_all(&backup);
    Ok(app.to_path_buf())
}

/// Start `app` again once this process is gone.
pub fn relaunch(app: &Path) {
    let _ = Command::new("/bin/sh")
        .arg("-c")
        .arg("sleep 1; exec /usr/bin/open \"$0\"")
        .arg(app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// A mounted DMG, detached when dropped.
struct Mount(PathBuf);

impl Mount {
    fn attach(dmg: &Path, at: &Path) -> Result<Self, String> {
        fs::create_dir_all(at).map_err(|e| e.to_string())?;
        run(
            Command::new("/usr/bin/hdiutil")
                .args([
                    "attach",
                    "-nobrowse",
                    "-readonly",
                    "-noverify",
                    "-mountpoint",
                ])
                .arg(at)
                .arg(dmg),
            "opening the DMG",
        )?;
        Ok(Self(at.to_path_buf()))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/hdiutil")
            .args(["detach", "-force"])
            .arg(&self.0)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// The `.app` that contains the executable `exe`, if it lives in one.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let in_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app";
    in_bundle.then(|| app.to_path_buf())
}

fn parse_release(json: &str) -> Result<Release, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| "unexpected answer from GitHub")?;
    let tag = v["tag_name"].as_str().ok_or("the release has no version")?;
    let asset = |suffix: &str| {
        v["assets"]
            .as_array()?
            .iter()
            .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(suffix)))
            .and_then(|a| a["browser_download_url"].as_str())
            .map(str::to_string)
    };
    let dmg_url = asset(".dmg").ok_or("the release has no DMG")?;
    let checksum_url = asset(".dmg.sha256").ok_or("the release has no checksum")?;
    for url in [&dmg_url, &checksum_url] {
        if !trusted_url(url) {
            return Err("the release points outside of the project".into());
        }
    }
    Ok(Release {
        version: tag.trim_start_matches('v').to_string(),
        notes: v["body"].as_str().unwrap_or_default().trim().to_string(),
        page_url: v["html_url"].as_str().map_or_else(
            || format!("https://github.com/{REPO}/releases"),
            str::to_string,
        ),
        dmg_url,
        checksum_url,
    })
}

/// Downloads must come from this project's releases (or a local test server in debug builds).
fn trusted_url(url: &str) -> bool {
    url.starts_with(&format!("https://github.com/{REPO}/"))
        || (cfg!(debug_assertions) && url.starts_with("http://127.0.0.1:"))
}

/// Is version `a` newer than `b`? Compares "1.10.0" style numbers part by part.
pub fn is_newer(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(a), parts(b));
    (0..a.len().max(b.len())).find_map(|i| {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0),
            b.get(i).copied().unwrap_or(0),
        );
        (x != y).then_some(x > y)
    }) == Some(true)
}

/// The hash out of a `shasum` line: "<64 hex digits>  <file name>".
fn parse_checksum(text: &str) -> Option<String> {
    let hash = text.split_whitespace().next()?.to_lowercase();
    (hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())).then_some(hash)
}

fn sha256_of(file: &Path) -> Result<String, String> {
    let out = run(
        Command::new("/usr/bin/shasum")
            .args(["-a", "256"])
            .arg(file),
        "checking the download",
    )?;
    parse_checksum(&out).ok_or_else(|| "could not compute the checksum".to_string())
}

fn http_get(url: &str) -> Result<String, String> {
    run(
        Command::new("/usr/bin/curl").args([
            "-fsSL",
            "--max-time",
            "20",
            "-H",
            "Accept: application/vnd.github+json",
            // GitHub wants a user agent; nothing about the user is sent.
            "-H",
            "User-Agent: ExploRust-updater",
            url,
        ]),
        "contacting GitHub",
    )
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    run(
        Command::new("/usr/bin/curl")
            .args(["-fL", "--retry", "2", "--max-time", "600", "-o"])
            .arg(to)
            .arg(url),
        "downloading the update",
    )
    .map(|_| ())
}

/// Run a command; its output on success, a readable error otherwise.
fn run(cmd: &mut Command, what: &str) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = err.trim();
        Err(if err.is_empty() {
            format!("{what} failed")
        } else {
            format!("{what} failed: {err}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "tag_name": "v0.2.0",
        "html_url": "https://github.com/erdemm1020/ExploRust/releases/tag/v0.2.0",
        "body": "  Faster search.\n",
        "assets": [
            {"name": "notes.txt", "browser_download_url": "https://github.com/erdemm1020/ExploRust/releases/download/v0.2.0/notes.txt"},
            {"name": "ExploRust-0.2.0.dmg", "browser_download_url": "https://github.com/erdemm1020/ExploRust/releases/download/v0.2.0/ExploRust-0.2.0.dmg"},
            {"name": "ExploRust-0.2.0.dmg.sha256", "browser_download_url": "https://github.com/erdemm1020/ExploRust/releases/download/v0.2.0/ExploRust-0.2.0.dmg.sha256"}
        ]
    }"#;

    #[test]
    fn reads_a_release() {
        let r = parse_release(SAMPLE).unwrap();
        assert_eq!(r.version, "0.2.0");
        assert_eq!(r.notes, "Faster search.");
        assert!(r.dmg_url.ends_with("ExploRust-0.2.0.dmg"));
        assert!(r.checksum_url.ends_with("ExploRust-0.2.0.dmg.sha256"));
        assert!(r.page_url.ends_with("/tag/v0.2.0"));
    }

    #[test]
    fn refuses_incomplete_or_foreign_releases() {
        assert!(parse_release("not json").is_err());
        assert!(parse_release(r#"{"tag_name": "v1", "assets": []}"#).is_err());
        // A DMG without its checksum is not installed.
        let no_sum = SAMPLE.replace("ExploRust-0.2.0.dmg.sha256", "other.txt");
        assert!(parse_release(&no_sum).is_err());
        // Downloads from somewhere else are not trusted.
        let foreign = SAMPLE.replace("github.com/erdemm1020/ExploRust", "evil.example.com/x");
        assert!(parse_release(&foreign).is_err());
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("v1.0.1", "1.0.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("1.0.0", "1.0"));
        assert!(is_newer("1.0.1-beta", "1.0.0"));
    }

    #[test]
    fn reads_shasum_lines() {
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(
            parse_checksum(&format!("{hash}  file.dmg\n")),
            Some(hash.into())
        );
        assert_eq!(parse_checksum(&hash.to_uppercase()), Some(hash.into()));
        assert_eq!(parse_checksum("tooshort  file"), None);
        assert_eq!(parse_checksum(""), None);
    }

    #[test]
    fn hashes_a_file() {
        let file = std::env::temp_dir().join(format!("explorust-sha-{}", std::process::id()));
        fs::write(&file, "abc").unwrap();
        assert_eq!(
            sha256_of(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_file(file);
    }

    #[test]
    fn finds_the_app_bundle_of_an_executable() {
        let exe = Path::new("/Applications/ExploRust.app/Contents/MacOS/ExploRust");
        assert_eq!(bundle_of(exe), Some("/Applications/ExploRust.app".into()));
        // A plain `cargo run` binary is not in a bundle.
        assert_eq!(
            bundle_of(Path::new("/Users/me/proj/target/debug/ExploRust")),
            None
        );
        assert_eq!(
            bundle_of(Path::new("/x/Foo.app/Contents/Other/ExploRust")),
            None
        );
    }

    // --- The whole install, against a local server and a real DMG (no UI involved). ---

    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("explorust-upd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal, validly signed `ExploRust.app` of the given version, inside `dir`.
    fn make_app(dir: &Path, version: &str) -> PathBuf {
        let app = dir.join("ExploRust.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(
            app.join("Contents/Info.plist"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
                 <key>CFBundleExecutable</key><string>ExploRust</string>\
                 <key>CFBundleIdentifier</key><string>app.explorust.test</string>\
                 <key>CFBundlePackageType</key><string>APPL</string>\
                 <key>CFBundleShortVersionString</key><string>{version}</string>\
                 </dict></plist>"
            ),
        )
        .unwrap();
        // Any real executable will do for the test.
        fs::copy("/usr/bin/true", app.join("Contents/MacOS/ExploRust")).unwrap();
        run(
            Command::new("/usr/bin/codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&app),
            "signing the test app",
        )
        .unwrap();
        app
    }

    fn make_dmg(from: &Path, to: &Path) {
        run(
            Command::new("/usr/bin/hdiutil")
                .args(["create", "-volname", "ExploRust", "-srcfolder"])
                .arg(from)
                .args(["-ov", "-format", "UDZO"])
                .arg(to),
            "making the test DMG",
        )
        .unwrap();
    }

    /// Serves `files` ("/name" -> bytes) on 127.0.0.1 and returns the port.
    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or("/");
                let body = files
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map(|(_, b)| b.clone());
                let (status, body) = match body {
                    Some(b) => ("200 OK", b),
                    None => ("404 Not Found", Vec::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        port
    }

    fn version_of(app: &Path) -> String {
        run(
            Command::new("/usr/libexec/PlistBuddy")
                .arg("-c")
                .arg("Print :CFBundleShortVersionString")
                .arg(app.join("Contents/Info.plist")),
            "reading a version",
        )
        .unwrap()
        .trim()
        .to_string()
    }

    /// An installed old app, plus a release (`version`) served locally whose
    /// checksum file says `checksum` (None: the right one).
    fn setup(name: &str, version: &str, checksum: Option<&str>) -> (PathBuf, PathBuf, Release) {
        let root = scratch(name);
        let installed = root.join("Applications");
        fs::create_dir_all(&installed).unwrap();
        let old = make_app(&installed, "0.0.1");

        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        make_app(&source, version);
        let dmg = root.join("ExploRust.dmg");
        make_dmg(&source, &dmg);
        let hash = checksum.map_or_else(|| sha256_of(&dmg).unwrap(), str::to_string);

        let port = serve(vec![
            ("/update.dmg", fs::read(&dmg).unwrap()),
            (
                "/update.dmg.sha256",
                format!("{hash}  update.dmg\n").into_bytes(),
            ),
        ]);
        let release = Release {
            version: version.to_string(),
            notes: String::new(),
            page_url: String::new(),
            dmg_url: format!("http://127.0.0.1:{port}/update.dmg"),
            checksum_url: format!("http://127.0.0.1:{port}/update.dmg.sha256"),
        };
        (root, old, release)
    }

    /// Is nothing under `root` still mounted?
    fn no_mount_left(root: &Path) -> bool {
        let mounts = run(&mut Command::new("/sbin/mount"), "listing mounts").unwrap();
        // `/var` is a symlink to `/private/var`: mount lists the real path.
        let real = fs::canonicalize(root).unwrap();
        !mounts.contains(real.to_str().unwrap())
    }

    #[test]
    fn installs_a_new_version_from_a_dmg() {
        let (root, old, release) = setup("ok", "9.9.9", None);
        assert_eq!(version_of(&old), "0.0.1");

        let work = root.join("work");
        fs::create_dir_all(&work).unwrap();
        let installed = install_in(&release, &old, old.parent().unwrap(), &work).unwrap();

        assert_eq!(installed, old);
        assert_eq!(version_of(&old), "9.9.9");
        // Nothing is left beside the app, and the DMG is no longer mounted.
        let leftovers: Vec<_> = fs::read_dir(old.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, ["ExploRust.app"]);
        assert!(no_mount_left(&root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_corrupted_download_leaves_the_app_alone() {
        let (root, old, release) = setup("bad-sum", "9.9.9", Some(&"0".repeat(64)));
        let work = root.join("work");
        fs::create_dir_all(&work).unwrap();
        let err = install_in(&release, &old, old.parent().unwrap(), &work).unwrap_err();
        assert!(err.contains("corrupted"), "{err}");
        assert_eq!(version_of(&old), "0.0.1");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_dmg_with_the_wrong_version_is_refused() {
        // The release says 9.9.9 but the DMG holds 1.2.3.
        let (root, old, mut release) = setup("wrong-version", "1.2.3", None);
        release.version = "9.9.9".into();
        let work = root.join("work");
        fs::create_dir_all(&work).unwrap();
        let err = install_in(&release, &old, old.parent().unwrap(), &work).unwrap_err();
        assert!(err.contains("1.2.3"), "{err}");
        assert_eq!(version_of(&old), "0.0.1");
        // The staged copy is cleaned up too.
        assert!(!old.parent().unwrap().join(".ExploRust-update.app").exists());
        assert!(no_mount_left(&root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_download_is_an_error() {
        let (root, old, mut release) = setup("missing", "9.9.9", None);
        release.dmg_url = release.dmg_url.replace("update.dmg", "nope.dmg");
        let work = root.join("work");
        fs::create_dir_all(&work).unwrap();
        assert!(install_in(&release, &old, old.parent().unwrap(), &work).is_err());
        assert_eq!(version_of(&old), "0.0.1");
        let _ = fs::remove_dir_all(root);
    }
}
