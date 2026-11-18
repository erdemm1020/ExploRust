//! Browsing and moving files on a server, through the `ssh` and `scp` commands
//! that macOS has. Keys and the ssh agent only (no password prompt can be shown),
//! and unknown servers are accepted the first time (`accept-new`) like most tools do.

use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{
    jobs::{Progress, run_cancellable},
    shell::quote,
};

/// One item of a remote folder.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// "Nov 18 12:34", as the server prints it.
    pub date: String,
}

/// A server we talk to.
#[derive(Clone, Debug, PartialEq)]
pub struct Conn {
    /// "host", "user@host" or an alias of `~/.ssh/config`.
    pub host: String,
    /// More `ssh -o` options (tests use them to reach a throwaway server).
    pub extra: Vec<String>,
}

/// A host name we accept: no option can hide in it (it never starts with `-`).
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() < 256
        && !host.starts_with('-')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._@%+:[]-".contains(c))
}

/// "user@host", "host:/path", "ssh://user@host/path" -> (host, path).
pub fn parse_target(input: &str) -> Option<(String, Option<String>)> {
    let input = input.trim();
    let input = input.strip_prefix("ssh://").unwrap_or(input);
    let (host, path) = match input.split_once(['/', ':']) {
        Some((host, rest)) => {
            let sep_was_slash = input.as_bytes().get(host.len()) == Some(&b'/');
            let path = if sep_was_slash {
                format!("/{rest}")
            } else {
                rest.to_string()
            };
            (host, Some(path).filter(|p| !p.is_empty()))
        }
        None => (input, None),
    };
    valid_host(host).then(|| (host.to_string(), path))
}

/// The aliases of `~/.ssh/config` (`Host` lines without wildcards).
pub fn load_hosts() -> Vec<String> {
    std::env::var_os("HOME")
        .map(|h| Path::new(&h).join(".ssh/config"))
        .and_then(|f| fs::read_to_string(f).ok())
        .map(|text| hosts_in(&text))
        .unwrap_or_default()
}

fn hosts_in(config: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        let Some(rest) = line
            .split_once(char::is_whitespace)
            .filter(|(k, _)| k.eq_ignore_ascii_case("host"))
        else {
            continue;
        };
        for name in rest.1.split_whitespace() {
            if !name.contains(['*', '?', '!'])
                && valid_host(name)
                && !hosts.iter().any(|h| h == name)
            {
                hosts.push(name.to_string());
            }
        }
    }
    hosts
}

/// Where the shared connections live (so a second command does not reconnect).
/// `None` if the folder is not safely ours.
fn control_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let uid = fs::metadata(&home).ok()?.uid();
    let dir = PathBuf::from(format!("/tmp/explorust-ssh-{uid}"));
    let _ = fs::create_dir_all(&dir);
    let _ = fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    let meta = fs::metadata(&dir).ok()?;
    (meta.uid() == uid && meta.mode() & 0o077 == 0).then_some(dir)
}

impl Conn {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_string(),
            extra: Vec::new(),
        }
    }

    fn options(&self) -> Vec<String> {
        let mut o: Vec<String> = [
            "BatchMode=yes",
            "ConnectTimeout=8",
            "ServerAliveInterval=5",
            "ServerAliveCountMax=2",
            "StrictHostKeyChecking=accept-new",
        ]
        .iter()
        .flat_map(|v| ["-o".to_string(), v.to_string()])
        .collect();
        if let Some(dir) = control_dir() {
            for v in [
                "ControlMaster=auto".to_string(),
                format!("ControlPath={}/%C", dir.display()),
                "ControlPersist=60".to_string(),
            ] {
                o.push("-o".into());
                o.push(v);
            }
        }
        o.extend(self.extra.clone());
        // Debug builds only: more `-o` options from the environment, to try the window
        // against a throwaway server.
        #[cfg(debug_assertions)]
        if let Ok(extra) = std::env::var("EXPLORUST_SSH_OPTS") {
            for opt in extra.split_whitespace() {
                o.push("-o".into());
                o.push(opt.to_string());
            }
        }
        o
    }

    /// `ssh <host> <command>`.
    fn ssh(&self, command: &str) -> Command {
        let mut c = Command::new("/usr/bin/ssh");
        c.args(self.options())
            .arg("--")
            .arg(&self.host)
            .arg(command);
        c
    }

    /// Run a command on the server; its output, or why it failed.
    fn run(&self, command: &str) -> Result<String, String> {
        let out = self.ssh(command).output().map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(explain(&out.stderr, out.status.code()))
        }
    }

    /// The server's home folder.
    pub fn home(&self) -> Result<String, String> {
        Ok(self.run("pwd")?.trim().to_string())
    }

    pub fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, String> {
        // `-L`: a link to a folder is a folder. A broken link makes `ls` exit with 1 but
        // the rest of the listing is good, so that alone is no failure.
        let out = self
            .ssh(&format!("cd {} && LC_ALL=C ls -lApL", quote(dir)))
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() && text.trim().is_empty() {
            return Err(explain(&out.stderr, out.status.code()));
        }
        Ok(parse_ls(&text))
    }

    pub fn mkdir(&self, path: &str) -> Result<(), String> {
        self.run(&format!("mkdir -- {}", quote(path))).map(|_| ())
    }

    /// Rename without overwriting anything.
    pub fn rename(&self, from: &str, to: &str) -> Result<(), String> {
        let check = format!(
            "test ! -e {to} && mv -- {from} {to}",
            from = quote(from),
            to = quote(to)
        );
        self.run(&check).map(|_| ()).map_err(|e| {
            if e.is_empty() {
                "the name is taken".into()
            } else {
                e
            }
        })
    }

    /// Delete for good. Refuses the root and the first level of the disk.
    pub fn remove(&self, path: &str) -> Result<(), String> {
        let depth = path.split('/').filter(|p| !p.is_empty()).count();
        if !path.starts_with('/') || depth < 2 || path.contains("/../") || path.ends_with("/..") {
            return Err("refusing to delete this folder".into());
        }
        self.run(&format!("rm -rf -- {}", quote(path))).map(|_| ())
    }

    fn scp(&self) -> Command {
        let mut c = Command::new("/usr/bin/scp");
        c.args(self.options()).arg("-r").arg("-q");
        c
    }

    /// Copy a remote file or folder into the local folder `into`.
    pub fn download(&self, remote: &str, into: &Path, progress: &Progress) -> Result<(), String> {
        let mut c = self.scp();
        c.arg("--").arg(format!("{}:{remote}", self.host)).arg(into);
        run_cancellable(&mut c, progress).map(|_| ())
    }

    /// Copy local files or folders into the remote folder `into`.
    pub fn upload(&self, local: &[PathBuf], into: &str, progress: &Progress) -> Result<(), String> {
        let mut c = self.scp();
        c.arg("--")
            .args(local)
            .arg(format!("{}:{}/", self.host, into.trim_end_matches('/')));
        run_cancellable(&mut c, progress).map(|_| ())
    }
}

/// What went wrong, in a sentence.
fn explain(stderr: &[u8], code: Option<i32>) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty() && !l.starts_with("Warning:"))
        .unwrap_or("")
        .trim();
    match line {
        "" => format!("the server answered with an error ({})", code.unwrap_or(-1)),
        l if l.contains("Permission denied") => {
            "permission denied (ExploRust uses your ssh keys; passwords cannot be typed here)"
                .into()
        }
        l => l.to_string(),
    }
}

/// Read `ls -lApL`: "perms links owner group size month day time-or-year name".
pub fn parse_ls(text: &str) -> Vec<RemoteEntry> {
    text.lines().filter_map(parse_ls_line).collect()
}

fn parse_ls_line(line: &str) -> Option<RemoteEntry> {
    // The 8 fields before the name are separated by runs of blanks; the name keeps its own.
    let mut rest = line;
    let mut fields = Vec::with_capacity(8);
    for _ in 0..8 {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace)?;
        fields.push(&rest[..end]);
        rest = &rest[end..];
    }
    let name = rest.strip_prefix(' ')?;
    let perms = fields[0];
    if perms.len() < 10 || !"-dlcbps".contains(&perms[..1]) {
        return None; // "total 12" and the like
    }
    let is_dir = perms.starts_with('d');
    let name = name.strip_suffix('/').unwrap_or(name);
    // A link not followed shows "name -> target": keep the name.
    let name = name.split(" -> ").next().unwrap_or(name);
    if name.is_empty() || name == "." || name == ".." {
        return None;
    }
    Some(RemoteEntry {
        name: name.to_string(),
        is_dir,
        size: fields[4].parse().unwrap_or(0),
        date: format!("{} {} {}", fields[5], fields[6], fields[7]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpStream,
        process::{Child, Stdio},
        time::{Duration, Instant},
    };

    #[test]
    fn only_safe_host_names_are_accepted() {
        for ok in [
            "example.com",
            "me@host",
            "user@10.0.0.1",
            "my-alias",
            "[::1]",
            "h_1",
        ] {
            assert!(valid_host(ok), "{ok}");
        }
        // An option disguised as a host would run a local command.
        for bad in [
            "",
            "-oProxyCommand=evil",
            "-v",
            "a b",
            "a;b",
            "a$(x)",
            "a\nb",
            "a'b",
        ] {
            assert!(!valid_host(bad), "{bad:?}");
        }
    }

    #[test]
    fn reads_what_the_user_types() {
        let p = |s: &str| parse_target(s);
        assert_eq!(p("me@host"), Some(("me@host".into(), None)));
        assert_eq!(
            p("host:/srv/www"),
            Some(("host".into(), Some("/srv/www".into())))
        );
        assert_eq!(
            p("ssh://me@host/var/log"),
            Some(("me@host".into(), Some("/var/log".into())))
        );
        assert_eq!(p("  host  "), Some(("host".into(), None)));
        assert_eq!(
            p("host:~/code"),
            Some(("host".into(), Some("~/code".into())))
        );
        assert_eq!(p("-oProxyCommand=x"), None);
        assert_eq!(p(""), None);
    }

    #[test]
    fn finds_the_aliases_of_the_ssh_config() {
        let config = "# my servers\nHost web db\n  HostName 1.2.3.4\nhost  *.internal staging\nHost *\n  ForwardAgent no\nHost !bad ok? -evil\nMatch all\nHost web\n";
        assert_eq!(hosts_in(config), ["web", "db", "staging"]);
    }

    #[test]
    fn reads_ls_output_from_linux_and_macos() {
        let text = "total 24\n\
            drwxr-xr-x  5 me  staff   160 Nov 18 12:34 projects/\n\
            -rw-r--r--  1 me  staff  1234 Nov  2  2025 notes with  spaces.txt\n\
            drwxr-xr-x  2 root root  4096 Jan  1 00:00 .config/\n\
            lrwxr-xr-x  1 me  staff    11 Nov 18 12:34 link -> target\n\
            -rwxr-xr-x  1 me  staff     0 Nov 18 12:34 run.sh\n\
            garbage\n";
        let entries = parse_ls(text);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "projects",
                "notes with  spaces.txt",
                ".config",
                "link",
                "run.sh"
            ]
        );
        assert!(entries[0].is_dir && !entries[1].is_dir);
        assert_eq!(entries[1].size, 1234);
        assert_eq!(entries[1].date, "Nov 2 2025");
        assert_eq!(entries[0].date, "Nov 18 12:34");
    }

    #[test]
    fn errors_are_put_in_words() {
        assert!(
            explain(b"user@h: Permission denied (publickey).\n", Some(255))
                .contains("permission denied")
        );
        assert_eq!(
            explain(
                b"ssh: connect to host x port 22: Connection refused\n",
                Some(255)
            ),
            "ssh: connect to host x port 22: Connection refused"
        );
        assert!(explain(b"", Some(3)).contains("(3)"));
    }

    #[test]
    fn dangerous_deletes_are_refused_before_ssh_is_even_called() {
        let c = Conn::new("example.invalid");
        for bad in ["/", "/home", "", "relative/path", "/home/me/../..", "/a/.."] {
            assert!(c.remove(bad).unwrap_err().contains("refusing"), "{bad}");
        }
    }

    // --- Against a real (throwaway) ssh server, run by this user on a local port. ---

    struct Server {
        child: Child,
        conn: Conn,
        dir: PathBuf,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    /// Starts `sshd` for this user only, with a one-time key. `None` if it cannot run here.
    fn server(name: &str) -> Option<Server> {
        let sshd = Path::new("/usr/sbin/sshd");
        if !sshd.exists() || !Path::new("/usr/bin/ssh-keygen").exists() {
            return None;
        }
        let dir =
            std::env::temp_dir().join(format!("explorust-sshd-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).ok()?;
        let key = |file: &str| {
            Command::new("/usr/bin/ssh-keygen")
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(dir.join(file))
                .status()
                .ok()
                .filter(|s| s.success())
        };
        key("host_key")?;
        key("client_key")?;
        fs::copy(dir.join("client_key.pub"), dir.join("authorized_keys")).ok()?;
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .ok()?
            .local_addr()
            .ok()?
            .port();
        fs::write(
            dir.join("sshd_config"),
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\nPidFile {d}/sshd.pid\n\
                 AuthorizedKeysFile {d}/authorized_keys\nPasswordAuthentication no\n\
                 KbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nLogLevel ERROR\n\
                 Subsystem sftp /usr/libexec/sftp-server\n",
                d = dir.display()
            ),
        )
        .ok()?;
        let child = Command::new(sshd)
            .args(["-D", "-e", "-f"])
            .arg(dir.join("sshd_config"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let started = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            if started.elapsed() > Duration::from_secs(5) {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut conn = Conn::new("127.0.0.1");
        for o in [
            format!("Port={port}"),
            format!("IdentityFile={}/client_key", dir.display()),
            "IdentitiesOnly=yes".into(),
            "UserKnownHostsFile=/dev/null".into(),
            "StrictHostKeyChecking=no".into(),
            "LogLevel=ERROR".into(),
        ] {
            conn.extra.push("-o".into());
            conn.extra.push(o);
        }
        let server = Server { child, conn, dir };
        // Make sure it answers before the test relies on it.
        server.conn.home().ok()?;
        Some(server)
    }

    #[test]
    fn browses_edits_and_transfers_files_on_a_real_server() {
        let Some(s) = server("full") else {
            eprintln!("(no sshd here: skipped)");
            return;
        };
        let remote = s.dir.join("remote home");
        fs::create_dir_all(remote.join("sub dir")).unwrap();
        fs::write(remote.join("hello.txt"), "hi there").unwrap();
        fs::write(remote.join("sub dir/inner.txt"), "x").unwrap();
        let r = remote.to_string_lossy().into_owned();

        // Listing.
        let list = s.conn.list(&r).unwrap();
        let names: Vec<_> = list.iter().map(|e| (e.name.as_str(), e.is_dir)).collect();
        assert!(names.contains(&("hello.txt", false)), "{names:?}");
        assert!(names.contains(&("sub dir", true)), "{names:?}");
        assert_eq!(list.iter().find(|e| e.name == "hello.txt").unwrap().size, 8);
        assert!(s.conn.list(&format!("{r}/nope")).is_err());

        // Download a file and a folder.
        let down = s.dir.join("down");
        fs::create_dir_all(&down).unwrap();
        s.conn
            .download(&format!("{r}/hello.txt"), &down, &Progress::default())
            .unwrap();
        s.conn
            .download(&format!("{r}/sub dir"), &down, &Progress::default())
            .unwrap();
        assert_eq!(
            fs::read_to_string(down.join("hello.txt")).unwrap(),
            "hi there"
        );
        assert_eq!(
            fs::read_to_string(down.join("sub dir/inner.txt")).unwrap(),
            "x"
        );

        // Upload.
        let local = s.dir.join("up file.txt");
        fs::write(&local, "uploaded").unwrap();
        s.conn.upload(&[local], &r, &Progress::default()).unwrap();
        assert_eq!(
            fs::read_to_string(remote.join("up file.txt")).unwrap(),
            "uploaded"
        );

        // New folder, rename (never over something), delete.
        s.conn.mkdir(&format!("{r}/it's new")).unwrap();
        assert!(remote.join("it's new").is_dir());
        s.conn
            .rename(&format!("{r}/it's new"), &format!("{r}/renamed"))
            .unwrap();
        assert!(remote.join("renamed").is_dir() && !remote.join("it's new").exists());
        assert!(
            s.conn
                .rename(&format!("{r}/renamed"), &format!("{r}/hello.txt"))
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(remote.join("hello.txt")).unwrap(),
            "hi there"
        );
        s.conn.remove(&format!("{r}/renamed")).unwrap();
        assert!(!remote.join("renamed").exists());
    }

    #[test]
    fn a_dead_server_is_reported_not_hung() {
        let mut conn = Conn::new("127.0.0.1");
        conn.extra = vec![
            "-o".into(),
            "Port=1".into(),
            "-o".into(),
            "ConnectTimeout=2".into(),
        ];
        let started = Instant::now();
        let err = conn.list("/").unwrap_err();
        assert!(!err.is_empty());
        assert!(started.elapsed() < Duration::from_secs(12));
    }
}
