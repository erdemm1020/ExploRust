//! Git, through the `git` command: what a repository says about the folder being
//! browsed (branch, changed files, ignored files) and a few safe actions.
//! Every read uses `--no-optional-locks`, so it never gets in the way of a
//! `git` command the user is running at the same time.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// What happened to a file, as a badge letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflict,
}

impl Change {
    pub fn letter(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "?",
            Self::Conflict => "!",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repo {
    pub root: PathBuf,
    /// The folder that was scanned, as git sees it (links resolved): the paths
    /// of its entries are `dir.join(name)`.
    pub dir: PathBuf,
    /// Branch name, or a short commit when HEAD is detached.
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    /// Changed files, by absolute path.
    pub changes: HashMap<PathBuf, Change>,
    /// Entries of the browsed folder that git ignores.
    pub ignored: HashSet<PathBuf>,
}

impl Repo {
    /// The badge of an item: its own change, or for a folder "something inside changed".
    pub fn badge(&self, path: &Path, is_dir: bool) -> Option<&'static str> {
        if let Some(change) = self.changes.get(path) {
            return Some(change.letter());
        }
        (is_dir && self.changes.keys().any(|p| p.starts_with(path))).then_some("•")
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        self.ignored.contains(path)
    }

    pub fn change_count(&self) -> usize {
        self.changes.len()
    }
}

/// The `git` to run. On a Mac without developer tools, `/usr/bin/git` is a stub that
/// pops up an installer: it is only used when the tools are there.
fn git_bin() -> Option<&'static Path> {
    static BIN: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    BIN.get_or_init(|| {
        ["/opt/homebrew/bin/git", "/usr/local/bin/git"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.exists())
            .or_else(|| {
                let tools = Command::new("/usr/bin/xcode-select")
                    .arg("-p")
                    .output()
                    .ok()?;
                tools
                    .status
                    .success()
                    .then(|| PathBuf::from("/usr/bin/git"))
            })
    })
    .as_deref()
}

/// Is there a usable `git` on this Mac?
pub fn available() -> bool {
    git_bin().is_some()
}

fn command() -> Command {
    Command::new(git_bin().unwrap_or(Path::new("/usr/bin/false")))
}

fn git(dir: &Path) -> Command {
    let mut c = command();
    c.arg("--no-optional-locks").arg("-C").arg(dir);
    c.stdin(Stdio::null()).stderr(Stdio::null());
    // Never wait for a password or an editor.
    c.env("GIT_TERMINAL_PROMPT", "0").env("GIT_EDITOR", "true");
    c
}

fn run(mut cmd: Command) -> Option<String> {
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What git knows about `dir` (a folder inside a repository), or `None`.
pub fn scan(dir: &Path) -> Option<Repo> {
    let root = PathBuf::from(
        run({
            let mut c = git(dir);
            c.args(["rev-parse", "--show-toplevel"]);
            c
        })?
        .trim(),
    );
    // canonicalize: `git` reports real paths (/private/var...), the browser may not.
    let root = std::fs::canonicalize(&root).unwrap_or(root);

    let status = run({
        let mut c = git(dir);
        c.args([
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=normal",
        ]);
        c
    })?;
    let mut repo = parse_status(&status, &root);
    repo.dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());

    // Ignored entries of this folder only: one call for the names we list.
    repo.ignored = ignored_in(&repo.dir);
    Some(repo)
}

/// The entries of `dir` that `.gitignore` & co. ignore.
fn ignored_in(dir: &Path) -> HashSet<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    let names: Vec<String> = read
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != ".git")
        .collect();
    if names.is_empty() {
        return HashSet::new();
    }
    let mut child = match git(dir)
        .args(["check-ignore", "-z", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return HashSet::new(),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let input = names.join("\0");
        // On its own thread: a long list must not block while git answers.
        std::thread::spawn(move || {
            let _ = std::io::Write::write_all(&mut stdin, input.as_bytes());
        });
    }
    let Ok(out) = child.wait_with_output() else {
        return HashSet::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(|name| dir.join(name))
        .collect()
}

/// Read `git status --porcelain=v2 --branch -z`.
fn parse_status(text: &str, root: &Path) -> Repo {
    let mut repo = Repo {
        root: root.to_path_buf(),
        ..Repo::default()
    };
    let mut records = text.split('\0').filter(|r| !r.is_empty());
    while let Some(rec) = records.next() {
        if let Some(rest) = rec.strip_prefix("# branch.head ") {
            repo.branch = rest.to_string();
        } else if let Some(rest) = rec.strip_prefix("# branch.oid ") {
            // Only used when detached (head is "(detached)").
            if repo.branch.is_empty() {
                repo.branch = rest.chars().take(7).collect();
            }
        } else if let Some(rest) = rec.strip_prefix("# branch.ab ") {
            // "+<ahead> -<behind>"
            let mut parts = rest.split_whitespace();
            repo.ahead = parts
                .next()
                .and_then(|p| p.trim_start_matches('+').parse().ok())
                .unwrap_or(0);
            repo.behind = parts
                .next()
                .and_then(|p| p.trim_start_matches('-').parse().ok())
                .unwrap_or(0);
        } else if let Some(path) = rec.strip_prefix("? ") {
            repo.changes.insert(root.join(path), Change::Untracked);
        } else if rec.starts_with("1 ") {
            // 1 XY sub mH mI mW hH hI <path>
            let mut f = rec.splitn(9, ' ');
            let xy = f.nth(1).unwrap_or("..");
            if let Some(path) = f.nth(6) {
                repo.changes.insert(root.join(path), classify(xy));
            }
        } else if rec.starts_with("2 ") {
            // 2 XY sub mH mI mW hH hI Xscore <path>\0<origPath>
            let mut f = rec.splitn(10, ' ');
            let xy = f.nth(1).unwrap_or("..");
            if let Some(path) = f.nth(7) {
                repo.changes
                    .insert(root.join(path), classify(xy).max_renamed());
            }
            records.next(); // the original path
        } else if rec.starts_with("u ") {
            // u XY sub m1 m2 m3 mW h1 h2 h3 <path>
            if let Some(path) = rec.splitn(11, ' ').nth(10) {
                repo.changes.insert(root.join(path), Change::Conflict);
            }
        }
    }
    if repo.branch == "(detached)" {
        repo.branch = "detached HEAD".into();
    }
    repo
}

impl Change {
    fn max_renamed(self) -> Self {
        // A rename shows as renamed, unless it is a conflict.
        if self == Change::Conflict {
            self
        } else {
            Change::Renamed
        }
    }
}

/// From the two-letter status ("M.", ".M", "A.", "UU"...).
fn classify(xy: &str) -> Change {
    let b: Vec<char> = xy.chars().collect();
    let (x, y) = (
        b.first().copied().unwrap_or('.'),
        b.get(1).copied().unwrap_or('.'),
    );
    if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
        Change::Conflict
    } else if x == 'D' || y == 'D' {
        Change::Deleted
    } else if x == 'A' {
        Change::Added
    } else if x == 'R' || x == 'C' {
        Change::Renamed
    } else {
        Change::Modified
    }
}

/// Stage `paths` (`git add`).
pub fn stage(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    run_paths(root, &["add", "--"], paths)
}

/// Unstage `paths`, keeping the changes in the files.
pub fn unstage(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    run_paths(root, &["restore", "--staged", "--"], paths)
}

/// Throw away the changes of tracked files (they go back to the last commit).
pub fn discard(root: &Path, paths: &[PathBuf]) -> Result<(), String> {
    run_paths(root, &["restore", "--"], paths)
}

fn run_paths(root: &Path, args: &[&str], paths: &[PathBuf]) -> Result<(), String> {
    let out = command()
        .arg("-C")
        .arg(root)
        .args(args)
        .args(paths)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// The web page of `path` (or of the repository when `path` is `None`) on the branch,
/// for GitHub and GitLab remotes.
pub fn web_url(root: &Path, branch: &str, path: Option<&Path>) -> Option<String> {
    let remote = run({
        let mut c = git(root);
        c.args(["remote", "get-url", "origin"]);
        c
    })?;
    let base = web_base(remote.trim())?;
    let Some(path) = path else { return Some(base) };
    let rel = path
        .strip_prefix(root)
        .ok()?
        .to_string_lossy()
        .replace(' ', "%20");
    if rel.is_empty() {
        return Some(base);
    }
    let kind = if path.is_dir() { "tree" } else { "blob" };
    let infix = if base.contains("gitlab") { "/-" } else { "" };
    let branch = if branch.starts_with("detached") {
        "HEAD"
    } else {
        branch
    };
    Some(format!("{base}{infix}/{kind}/{branch}/{rel}"))
}

/// "https://github.com/user/repo" from the usual remote spellings.
fn web_base(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let remote = remote.strip_suffix(".git").unwrap_or(remote);
    let (host, path) = if let Some(rest) = remote.strip_prefix("git@") {
        rest.split_once(':')?
    } else if let Some(rest) = remote.strip_prefix("ssh://git@") {
        rest.split_once('/')?
    } else if let Some(rest) = remote.strip_prefix("https://") {
        // Drop credentials: "https://user:token@host/..." must never reach the browser.
        rest.rsplit_once('@')
            .map_or(rest, |(_, h)| h)
            .split_once('/')?
    } else {
        return None;
    };
    let host = host.split(':').next()?;
    (!host.is_empty() && !path.is_empty()).then(|| format!("https://{host}/{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("explorust-git-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(&dir).unwrap()
    }

    fn git_ok(dir: &Path, args: &[&str]) {
        let status = command()
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn have_git() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[test]
    fn a_plain_folder_is_not_a_repository() {
        if !have_git() {
            return;
        }
        let dir = scratch("plain");
        assert!(scan(&dir).is_none());
    }

    #[test]
    fn reads_branch_changes_and_ignored_files() {
        if !have_git() {
            return;
        }
        let dir = scratch("repo");
        git_ok(&dir, &["init", "-q", "-b", "main"]);
        fs::write(dir.join(".gitignore"), "target/\n*.log\n").unwrap();
        fs::write(dir.join("kept.txt"), "1").unwrap();
        fs::write(dir.join("gone.txt"), "1").unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/lib.rs"), "1").unwrap();
        git_ok(&dir, &["add", "."]);
        git_ok(&dir, &["commit", "-q", "-m", "first"]);

        fs::write(dir.join("kept.txt"), "changed").unwrap(); // modified
        fs::remove_file(dir.join("gone.txt")).unwrap(); // deleted
        fs::write(dir.join("new.txt"), "n").unwrap(); // untracked
        fs::write(dir.join("src/lib.rs"), "changed").unwrap(); // inside a folder
        fs::create_dir_all(dir.join("target")).unwrap();
        fs::write(dir.join("target/x"), "x").unwrap();
        fs::write(dir.join("debug.log"), "x").unwrap();

        let repo = scan(&dir).unwrap();
        assert_eq!(repo.branch, "main");
        assert_eq!(repo.root, dir);
        assert_eq!(repo.badge(&dir.join("kept.txt"), false), Some("M"));
        assert_eq!(repo.badge(&dir.join("gone.txt"), false), Some("D"));
        assert_eq!(repo.badge(&dir.join("new.txt"), false), Some("?"));
        // A folder shows a dot when something inside changed.
        assert_eq!(repo.badge(&dir.join("src"), true), Some("•"));
        assert_eq!(repo.badge(&dir.join("target"), true), None);
        assert!(repo.is_ignored(&dir.join("target")));
        assert!(repo.is_ignored(&dir.join("debug.log")));
        assert!(!repo.is_ignored(&dir.join("kept.txt")));
        assert!(!repo.is_ignored(&dir.join(".git")));
    }

    #[test]
    fn stage_unstage_and_discard() {
        if !have_git() {
            return;
        }
        let dir = scratch("actions");
        git_ok(&dir, &["init", "-q", "-b", "main"]);
        fs::write(dir.join("a.txt"), "1").unwrap();
        git_ok(&dir, &["add", "."]);
        git_ok(&dir, &["commit", "-q", "-m", "first"]);
        fs::write(dir.join("a.txt"), "2").unwrap();

        stage(&dir, &[dir.join("a.txt")]).unwrap();
        let status = run({
            let mut c = git(&dir);
            c.args(["status", "--porcelain"]);
            c
        })
        .unwrap();
        assert!(status.starts_with("M "), "{status}"); // staged

        unstage(&dir, &[dir.join("a.txt")]).unwrap();
        let status = run({
            let mut c = git(&dir);
            c.args(["status", "--porcelain"]);
            c
        })
        .unwrap();
        assert!(status.starts_with(" M"), "{status}"); // modified, not staged
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "2");

        discard(&dir, &[dir.join("a.txt")]).unwrap();
        assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "1");
        assert!(discard(&dir, &[dir.join("nope")]).is_err());
    }

    #[test]
    fn reads_ahead_behind_conflicts_and_renames() {
        let root = Path::new("/r");
        let text = "# branch.oid abc\0# branch.head dev\0# branch.upstream origin/dev\0# branch.ab +2 -1\0\
                    1 .M N... 100644 100644 100644 h h src/a b.rs\0\
                    2 R. N... 100644 100644 100644 h h R100 new.rs\0old.rs\0\
                    u UU N... 100644 100644 100644 100644 h h h clash.txt\0\
                    ? untracked file.txt\0";
        let repo = parse_status(text, root);
        assert_eq!(
            (repo.branch.as_str(), repo.ahead, repo.behind),
            ("dev", 2, 1)
        );
        assert_eq!(repo.changes[&root.join("src/a b.rs")], Change::Modified);
        assert_eq!(repo.changes[&root.join("new.rs")], Change::Renamed);
        assert!(!repo.changes.contains_key(&root.join("old.rs")));
        assert_eq!(repo.changes[&root.join("clash.txt")], Change::Conflict);
        assert_eq!(
            repo.changes[&root.join("untracked file.txt")],
            Change::Untracked
        );
        let detached = parse_status("# branch.oid 1234567890\0# branch.head (detached)\0", root);
        assert_eq!(detached.branch, "detached HEAD");
    }

    #[test]
    fn remotes_become_web_addresses() {
        assert_eq!(
            web_base("git@github.com:me/proj.git").as_deref(),
            Some("https://github.com/me/proj")
        );
        assert_eq!(
            web_base("https://github.com/me/proj.git").as_deref(),
            Some("https://github.com/me/proj")
        );
        assert_eq!(
            web_base("ssh://git@gitlab.com/g/p.git").as_deref(),
            Some("https://gitlab.com/g/p")
        );
        // Credentials in a URL never reach the browser.
        assert_eq!(
            web_base("https://user:secret-token@github.com/me/proj.git").as_deref(),
            Some("https://github.com/me/proj")
        );
        assert_eq!(web_base("/local/path/repo"), None);
        assert_eq!(web_base(""), None);
    }

    #[test]
    fn builds_page_addresses_for_files_and_folders() {
        if !have_git() {
            return;
        }
        let dir = scratch("url");
        git_ok(&dir, &["init", "-q", "-b", "main"]);
        git_ok(
            &dir,
            &["remote", "add", "origin", "git@github.com:me/proj.git"],
        );
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/a b.rs"), "").unwrap();
        assert_eq!(
            web_url(&dir, "main", None).as_deref(),
            Some("https://github.com/me/proj")
        );
        assert_eq!(
            web_url(&dir, "main", Some(&dir.join("src/a b.rs"))).as_deref(),
            Some("https://github.com/me/proj/blob/main/src/a%20b.rs")
        );
        assert_eq!(
            web_url(&dir, "dev", Some(&dir.join("src"))).as_deref(),
            Some("https://github.com/me/proj/tree/dev/src")
        );
    }
}
