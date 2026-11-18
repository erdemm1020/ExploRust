//! User settings, kept in a tiny `key = value` file:
//! `~/Library/Application Support/ExploRust/config`.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// A setting with a few named choices, stored by key and shown by label.
macro_rules! choices {
    ($(#[$meta:meta])* $name:ident { $($var:ident = $key:literal, $label:literal;)+ } default $default:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $name { $($var),+ }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl $name {
            pub const ALL: &[Self] = &[$(Self::$var),+];

            pub fn key(self) -> &'static str {
                match self { $(Self::$var => $key),+ }
            }

            #[allow(dead_code)]
            pub fn label(self) -> &'static str {
                match self { $(Self::$var => $label),+ }
            }

            fn parse(s: &str) -> Option<Self> {
                match s { $($key => Some(Self::$var),)+ _ => None }
            }
        }
    };
}

choices! {
    /// Follow the system, or force one look.
    ThemeMode {
        System = "system", "System";
        Light = "light", "Light";
        Dark = "dark", "Dark";
    } default System
}

choices! {
    Accent {
        System = "system", "System";
        Blue = "blue", "Blue";
        Purple = "purple", "Purple";
        Pink = "pink", "Pink";
        Red = "red", "Red";
        Orange = "orange", "Orange";
        Green = "green", "Green";
    } default System
}

choices! {
    /// Height of a row in the list.
    Density {
        Compact = "compact", "Compact";
        Comfortable = "comfortable", "Comfortable";
        Spacious = "spacious", "Spacious";
    } default Comfortable
}

impl Accent {
    /// The fixed color, or `None` for the system accent.
    pub fn rgb(self) -> Option<u32> {
        match self {
            Self::System => None,
            Self::Blue => Some(0x007aff),
            Self::Purple => Some(0xaf52de),
            Self::Pink => Some(0xff2d55),
            Self::Red => Some(0xff3b30),
            Self::Orange => Some(0xff9500),
            Self::Green => Some(0x34c759),
        }
    }
}

impl Density {
    pub fn row_height(self) -> f32 {
        match self {
            Self::Compact => 22.0,
            Self::Comfortable => 26.0,
            Self::Spacious => 32.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub theme: ThemeMode,
    pub accent: Accent,
    pub density: Density,
    /// "file2" sorts before "file10".
    pub natural_sort: bool,
    pub folders_on_top: bool,
    /// Open folders with one click instead of two.
    pub single_click: bool,
    /// Update the list when the folder changes on disk.
    pub live_refresh: bool,
    /// "Today at 12:40" instead of a full date.
    pub relative_dates: bool,
    /// Ask before trashing a folder holding more than this many items
    /// (`None`: never ask).
    pub trash_warning: Option<usize>,
    pub show_hidden: bool,
    /// Start in the folder that was open last time.
    pub reopen_last: bool,
    pub last_folder: Option<PathBuf>,
    /// Look for a new version when the app starts.
    pub check_updates: bool,
    /// A version the user chose not to be asked about again.
    pub skipped_version: Option<String>,
    /// Terminal app chosen by the user (`None`: use the system one).
    pub terminal: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeMode::default(),
            accent: Accent::default(),
            density: Density::default(),
            natural_sort: true,
            folders_on_top: true,
            single_click: false,
            live_refresh: true,
            relative_dates: true,
            trash_warning: Some(100),
            show_hidden: false,
            reopen_last: false,
            last_folder: None,
            check_updates: true,
            skipped_version: None,
            terminal: None,
        }
    }
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

    /// Unknown keys and bad values are ignored: the default stays.
    fn parse(text: &str) -> Self {
        let mut c = Self::default();
        let flag = |v: &str| match v {
            "true" | "yes" | "on" => Some(true),
            "false" | "no" | "off" => Some(false),
            _ => None,
        };
        let path = |v: &str| (!v.is_empty()).then(|| PathBuf::from(v));

        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "theme" => c.theme = ThemeMode::parse(value).unwrap_or(c.theme),
                "accent" => c.accent = Accent::parse(value).unwrap_or(c.accent),
                "density" => c.density = Density::parse(value).unwrap_or(c.density),
                "natural_sort" => c.natural_sort = flag(value).unwrap_or(c.natural_sort),
                "folders_on_top" => c.folders_on_top = flag(value).unwrap_or(c.folders_on_top),
                "single_click" => c.single_click = flag(value).unwrap_or(c.single_click),
                "live_refresh" => c.live_refresh = flag(value).unwrap_or(c.live_refresh),
                "relative_dates" => c.relative_dates = flag(value).unwrap_or(c.relative_dates),
                "show_hidden" => c.show_hidden = flag(value).unwrap_or(c.show_hidden),
                "check_updates" => c.check_updates = flag(value).unwrap_or(c.check_updates),
                "skipped_version" => {
                    c.skipped_version = (!value.is_empty()).then(|| value.to_string())
                }
                "reopen_last" => c.reopen_last = flag(value).unwrap_or(c.reopen_last),
                "trash_warning" => match value {
                    "off" => c.trash_warning = None,
                    n => c.trash_warning = n.parse().ok().or(c.trash_warning),
                },
                "last_folder" => c.last_folder = path(value),
                "terminal" => c.terminal = path(value),
                _ => {}
            }
        }
        c
    }

    fn render(&self) -> String {
        let flag = |b: bool| if b { "true" } else { "false" };
        let mut out = format!(
            "theme = {}\naccent = {}\ndensity = {}\nnatural_sort = {}\nfolders_on_top = {}\n\
             single_click = {}\nlive_refresh = {}\nrelative_dates = {}\nshow_hidden = {}\n\
             reopen_last = {}\ncheck_updates = {}\ntrash_warning = {}\n",
            self.theme.key(),
            self.accent.key(),
            self.density.key(),
            flag(self.natural_sort),
            flag(self.folders_on_top),
            flag(self.single_click),
            flag(self.live_refresh),
            flag(self.relative_dates),
            flag(self.show_hidden),
            flag(self.reopen_last),
            flag(self.check_updates),
            self.trash_warning
                .map_or("off".to_string(), |n| n.to_string()),
        );
        for (key, value) in [
            ("last_folder", &self.last_folder),
            ("terminal", &self.terminal),
        ] {
            if let Some(p) = value {
                out.push_str(&format!("{key} = {}\n", p.display()));
            }
        }
        if let Some(v) = &self.skipped_version {
            out.push_str(&format!("skipped_version = {v}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip() {
        let c = Config::default();
        assert_eq!(Config::parse(&c.render()), c);
    }

    #[test]
    fn every_setting_round_trips() {
        let c = Config {
            theme: ThemeMode::Dark,
            accent: Accent::Pink,
            density: Density::Compact,
            natural_sort: false,
            folders_on_top: false,
            single_click: true,
            live_refresh: false,
            relative_dates: false,
            trash_warning: None,
            show_hidden: true,
            reopen_last: true,
            last_folder: Some("/Users/me/My Docs".into()),
            check_updates: false,
            skipped_version: Some("0.3.0".into()),
            terminal: Some("/Applications/Ghostty.app".into()),
        };
        assert_eq!(Config::parse(&c.render()), c);
    }

    #[test]
    fn ignores_junk_and_keeps_defaults() {
        let c = Config::parse(
            "garbage\n# comment\nterminal =   /Applications/Warp.app  \nother = 1\n\
             theme = neon\ndensity=\nsingle_click = maybe\ntrash_warning = lots\n",
        );
        assert_eq!(c.terminal, Some("/Applications/Warp.app".into()));
        assert_eq!(c.theme, ThemeMode::System);
        assert!(!c.single_click);
        assert_eq!(c.trash_warning, Some(100));
        assert_eq!(Config::parse("terminal =\n"), Config::default());
    }

    #[test]
    fn every_choice_has_a_unique_key() {
        for keys in [
            ThemeMode::ALL.iter().map(|c| c.key()).collect::<Vec<_>>(),
            Accent::ALL.iter().map(|c| c.key()).collect(),
            Density::ALL.iter().map(|c| c.key()).collect(),
        ] {
            let mut sorted = keys.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), keys.len());
        }
    }
}
