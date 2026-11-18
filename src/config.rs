//! User settings, kept in a tiny `key = value` file:
//! `~/Library/Application Support/ExploRust/config`.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Config {
    /// Terminal app chosen by the user (`None`: use the system one).
    pub terminal: Option<PathBuf>,
}

impl Config {
    fn file() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(Path::new(&home).join("Library/Application Support/ExploRust/config"))
    }

    pub fn load() -> Self {
        Self::file()
            .and_then(|f| fs::read_to_string(f).ok())
            .map(|s| Self::parse(&s))
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        let file = Self::file().ok_or_else(|| io::Error::other("no HOME"))?;
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(file, self.render())
    }

    fn parse(text: &str) -> Self {
        let mut config = Self::default();
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=')
                && key.trim() == "terminal"
                && !value.trim().is_empty()
            {
                config.terminal = Some(PathBuf::from(value.trim()));
            }
        }
        config
    }

    fn render(&self) -> String {
        match &self.terminal {
            Some(t) => format!("terminal = {}\n", t.display()),
            None => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let c = Config {
            terminal: Some("/Applications/Ghostty.app".into()),
        };
        assert_eq!(Config::parse(&c.render()), c);
        assert_eq!(
            Config::parse(&Config::default().render()),
            Config::default()
        );
    }

    #[test]
    fn ignores_junk() {
        let c =
            Config::parse("garbage\n# comment\nterminal =   /Applications/Warp.app  \nother = 1\n");
        assert_eq!(c.terminal, Some("/Applications/Warp.app".into()));
        assert_eq!(Config::parse("terminal =\n"), Config::default());
    }
}
