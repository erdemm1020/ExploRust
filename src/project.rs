//! Recognizing what kind of project a folder is, and its everyday commands.

use std::{
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::shell::quote;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rust,
    Node,
    Go,
    Swift,
    Python,
    Java,
    Ruby,
    Php,
    Make,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Node => "Node",
            Self::Go => "Go",
            Self::Swift => "Swift",
            Self::Python => "Python",
            Self::Java => "Java",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Make => "Make",
        }
    }
}

/// A command the menu offers, run in the user's terminal.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub label: String,
    pub command: String,
}

fn task(command: &str) -> Task {
    Task {
        label: command.to_string(),
        command: command.to_string(),
    }
}

/// The kind of project in `dir`, from the files that give it away. Cheap: a few `exists`.
pub fn detect(dir: &Path) -> Option<Kind> {
    let has = |name: &str| dir.join(name).exists();
    if has("Cargo.toml") {
        Some(Kind::Rust)
    } else if has("package.json") {
        Some(Kind::Node)
    } else if has("go.mod") {
        Some(Kind::Go)
    } else if has("Package.swift") {
        Some(Kind::Swift)
    } else if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        Some(Kind::Python)
    } else if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
        Some(Kind::Java)
    } else if has("Gemfile") {
        Some(Kind::Ruby)
    } else if has("composer.json") {
        Some(Kind::Php)
    } else if has("Makefile") {
        Some(Kind::Make)
    } else {
        None
    }
}

/// The handful of commands worth a menu entry for this project.
pub fn tasks(dir: &Path, kind: Kind) -> Vec<Task> {
    let has = |name: &str| dir.join(name).exists();
    match kind {
        Kind::Rust => ["cargo run", "cargo build", "cargo test", "cargo check"]
            .map(task)
            .into(),
        Kind::Go => ["go run .", "go build ./...", "go test ./..."]
            .map(task)
            .into(),
        Kind::Swift => ["swift run", "swift build", "swift test"].map(task).into(),
        Kind::Node => {
            let pm = if has("pnpm-lock.yaml") {
                "pnpm"
            } else if has("yarn.lock") {
                "yarn"
            } else if has("bun.lockb") || has("bun.lock") {
                "bun"
            } else {
                "npm"
            };
            let mut out = vec![task(&format!("{pm} install"))];
            let scripts = package_scripts(&dir.join("package.json"));
            for name in ["dev", "start", "build", "test", "lint"] {
                if scripts.iter().any(|s| s == name) {
                    out.push(task(&format!("{pm} run {name}")));
                }
            }
            out
        }
        Kind::Python => {
            let mut out = Vec::new();
            if has("uv.lock") {
                out.push(task("uv sync"));
            } else if has("requirements.txt") {
                out.push(task("python3 -m pip install -r requirements.txt"));
            }
            if has("tests") || has("test") {
                out.push(task("python3 -m pytest"));
            }
            out
        }
        Kind::Java => {
            if has("pom.xml") {
                ["mvn test", "mvn package"].map(task).into()
            } else if has("gradlew") {
                ["./gradlew build", "./gradlew test"].map(task).into()
            } else {
                ["gradle build", "gradle test"].map(task).into()
            }
        }
        Kind::Ruby => {
            let mut out = vec![task("bundle install")];
            if has("Rakefile") {
                out.push(task("bundle exec rake"));
            }
            out
        }
        Kind::Php => vec![task("composer install")],
        Kind::Make => {
            let mut out = vec![task("make")];
            if fs::read_to_string(dir.join("Makefile"))
                .is_ok_and(|m| m.lines().any(|l| l.starts_with("test:")))
            {
                out.push(task("make test"));
            }
            out
        }
    }
}

/// The keys of `"scripts"` in a package.json.
fn package_scripts(file: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(file) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    json["scripts"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// A `.command` file that runs `command` in `dir` in a login shell (so PATH has cargo, node...)
/// and waits for a key at the end, so the output can be read. Opening it with a terminal runs it.
pub fn write_script(dir: &Path, command: &str) -> io::Result<PathBuf> {
    let script = format!(
        "#!/bin/zsh -il\n\
         cd {dir} || exit 1\n\
         printf '\\033[1m▶ %s\\033[0m\\n\\n' {shown}\n\
         {command}\n\
         code=$?\n\
         printf '\\n\\033[2m[exited with %s — press any key to close]\\033[0m' \"$code\"\n\
         read -k 1\n\
         exit $code\n",
        dir = quote(&dir.to_string_lossy()),
        shown = quote(command),
    );
    // Unique even for two scripts made in the same millisecond.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let path = std::env::temp_dir().join(format!(
        "explorust-run-{}-{stamp}-{n}.command",
        std::process::id()
    ));
    fs::write(&path, script)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn project(files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "explorust-proj-{}-{}",
            std::process::id(),
            files.join("_").replace('/', "-")
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for f in files {
            let p = dir.join(f);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(p, "").unwrap();
        }
        dir
    }

    #[test]
    fn recognizes_projects_from_their_files() {
        let cases = [
            ("Cargo.toml", Kind::Rust),
            ("package.json", Kind::Node),
            ("go.mod", Kind::Go),
            ("Package.swift", Kind::Swift),
            ("pyproject.toml", Kind::Python),
            ("requirements.txt", Kind::Python),
            ("pom.xml", Kind::Java),
            ("build.gradle.kts", Kind::Java),
            ("Gemfile", Kind::Ruby),
            ("composer.json", Kind::Php),
            ("Makefile", Kind::Make),
        ];
        for (file, kind) in cases {
            assert_eq!(detect(&project(&[file])), Some(kind), "{file}");
        }
        assert_eq!(detect(&project(&["notes.txt"])), None);
        // The most specific wins: a Rust project with a Makefile is a Rust project.
        assert_eq!(
            detect(&project(&["Cargo.toml", "Makefile"])),
            Some(Kind::Rust)
        );
    }

    #[test]
    fn node_commands_follow_the_package_manager_and_the_scripts() {
        let dir = project(&["yarn.lock"]);
        fs::write(
            dir.join("package.json"),
            r#"{"scripts":{"build":"x","test":"y","weird":"z"}}"#,
        )
        .unwrap();
        let labels: Vec<_> = tasks(&dir, Kind::Node)
            .into_iter()
            .map(|t| t.label)
            .collect();
        assert_eq!(labels, ["yarn install", "yarn run build", "yarn run test"]);

        let npm = project(&["package.json"]);
        assert_eq!(tasks(&npm, Kind::Node).len(), 1); // just "npm install": no scripts
        fs::write(npm.join("package.json"), "not json").unwrap();
        assert_eq!(tasks(&npm, Kind::Node).len(), 1);
    }

    #[test]
    fn other_projects_have_their_usual_commands() {
        let rust = tasks(&project(&["Cargo.toml"]), Kind::Rust);
        assert_eq!(rust[0].command, "cargo run");
        assert_eq!(rust.len(), 4);
        let py = project(&["requirements.txt", "tests/test_a.py"]);
        let labels: Vec<_> = tasks(&py, Kind::Python)
            .into_iter()
            .map(|t| t.command)
            .collect();
        assert_eq!(
            labels,
            [
                "python3 -m pip install -r requirements.txt",
                "python3 -m pytest"
            ]
        );
        let mvn = tasks(&project(&["pom.xml"]), Kind::Java);
        assert_eq!(mvn[0].command, "mvn test");
        let make = project(&["Makefile"]);
        fs::write(make.join("Makefile"), "all:\n\techo\ntest:\n\techo\n").unwrap();
        assert_eq!(tasks(&make, Kind::Make).len(), 2);
    }

    #[test]
    fn the_run_script_is_valid_and_quotes_the_folder() {
        let dir = std::env::temp_dir().join("it's a \"weird\" $HOME dir");
        fs::create_dir_all(&dir).unwrap();
        let script = write_script(&dir, "echo hello").unwrap();
        // It is executable, and the shell accepts it.
        assert_eq!(
            fs::metadata(&script).unwrap().permissions().mode() & 0o111,
            0o111
        );
        let check = Command::new("/bin/zsh")
            .arg("-n")
            .arg(&script)
            .status()
            .unwrap();
        assert!(check.success());
        let text = fs::read_to_string(&script).unwrap();
        assert!(text.starts_with("#!/bin/zsh -il\n"));
        assert!(text.contains("it'\\''s a \"weird\" $HOME dir'"), "{text}");
        // `status` is read-only in zsh: the script must not assign it.
        assert!(!text.contains("status="), "{text}");
        let _ = fs::remove_file(script);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_script_really_runs_the_command_in_the_folder() {
        let dir = project(&["marker.txt"]);
        // Run by zsh itself (no rc files, no terminal): everything up to the final
        // "press any key" is checked; that last `read` needs a real terminal.
        let script = write_script(&dir, "ls marker.txt").unwrap();
        let out = Command::new("/bin/zsh")
            .arg("-f")
            .arg(&script)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("marker.txt"), "{text}");
        assert!(text.contains("exited with 0"), "{text}");
        let _ = fs::remove_file(script);
    }
}
