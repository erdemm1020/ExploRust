use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use gpui::{
    AnyElement, ClickEvent, ClipboardItem, Context, ExternalPaths, FocusHandle, Focusable, Hsla,
    IntoElement, KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, NavigationDirection,
    PathPromptOptions, Pixels, Point, PromptLevel, Render, ScrollStrategy, SharedString,
    UniformListScrollHandle, Window, WindowControlArea, anchored, deferred, div, prelude::*, px,
    svg, uniform_list,
};

mod claude_ui;
mod git_ui;
mod ops;
mod project_ui;
mod remote;
mod scans;
mod settings;
mod updates;

use crate::{
    apps::{self, AppInfo},
    clipboard,
    config::Config,
    dates::DateFormat,
    fs::{self, Sort, SortKey},
    text_field::{KeyResult, TextField},
    theme::Theme,
    undo::UndoOp,
};

const TOOLBAR_H: f32 = 44.0;
const DATE_W: f32 = 170.0;
const SEARCH_W: f32 = 200.0;
const SIZE_W: f32 = 90.0;
const SIDEBAR_W: f32 = 180.0;

/// What is being dragged out of the list: the paths of the dragged rows.
#[derive(Clone)]
struct DragPaths(Vec<PathBuf>);

/// The small label that follows the cursor while dragging.
struct DragPreview {
    label: SharedString,
    position: Point<Pixels>,
    theme: Theme,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div().pl(self.position.x).pt(self.position.y).child(
            div()
                .px_3()
                .py_1()
                .rounded_md()
                .bg(t.accent)
                .text_color(t.on_accent)
                .text_size(px(12.0))
                .shadow_md()
                .child(self.label.clone()),
        )
    }
}

#[derive(Clone, Copy)]
enum MenuAction {
    Open,
    Reveal,
    Copy,
    CopyPath,
    Paste,
    OpenWith,
    OpenTerminal,
    OpenEditor,
    OpenInChoices,
    Info,
    ClaudeExplain,
    ClaudeName,
    ClaudeSummarize,
    ClaudeOrganize,
    RemoteOpen,
    RemoteDownload,
    RemoteDelete,
    RemoteNewFolder,
    RemoteTerminal,
    RemoteRefresh,
    RunTask(u8),
    GitStage,
    GitUnstage,
    GitDiscard,
    GitWeb,
    Compress,
    Extract,
    Rename,
    Trash,
    NewFolder,
    NewFile,
    ToggleHidden,
}

impl MenuAction {
    fn opens_submenu(self) -> bool {
        matches!(self, Self::OpenWith | Self::OpenInChoices)
    }

    fn is(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
}

enum MenuEntry {
    Item(SharedString, MenuAction),
    Separator,
}

#[derive(Clone, Copy, PartialEq)]
enum SubmenuKind {
    /// Apps that can open the selected files.
    Files,
    /// Code editors and terminals to open the selection (or this folder) in.
    OpenIn,
}

/// What an app of a submenu section is used for.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Editor,
    Terminal,
}

/// A group of apps in a submenu, one of which is the default.
struct Section {
    title: Option<&'static str>,
    /// `None`: apps that open the selected files (the default is per file type).
    role: Option<Role>,
    apps: Vec<AppInfo>,
    default: Option<PathBuf>,
}

/// The expanded submenu.
struct Submenu {
    kind: SubmenuKind,
    sections: Vec<Section>,
    /// For the files kind: the file whose type gets the default app.
    subject: PathBuf,
}

struct ContextMenu {
    position: Point<Pixels>,
    entries: Vec<MenuEntry>,
    /// `Some` once a submenu has been expanded.
    submenu: Option<Submenu>,
}

const MENU_W: f32 = 210.0;
const MENU_ITEM_H: f32 = 24.0;
const MENU_SEPARATOR_H: f32 = 9.0;

/// A row being renamed in place.
struct Renaming {
    ix: usize,
    field: TextField,
}

struct Place {
    name: SharedString,
    icon: &'static str,
    path: PathBuf,
}

pub struct Browser {
    cwd: PathBuf,
    /// Everything in the folder; `entries` is what the search lets through.
    all: Vec<fs::Entry>,
    entries: Vec<fs::Entry>,
    query: TextField,
    search_focused: bool,
    selected: BTreeSet<usize>,
    /// Fixed end of a shift-selection.
    anchor: Option<usize>,
    /// Moving end: where the arrow keys continue from.
    cursor: Option<usize>,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    sort: Sort,
    dates: DateFormat,
    menu: Option<ContextMenu>,
    config: Config,
    settings_open: bool,
    settings_scroll: gpui::ScrollHandle,
    claude_ready: bool,
    ai: Option<claude_ui::AiPanel>,
    /// The server whose folder is shown (`cwd` is then a path on that server).
    remote: Option<String>,
    remote_gen: u64,
    remote_loading: bool,
    remote_error: Option<String>,
    /// The "Connect to Server" box, while it is open.
    connect: Option<TextField>,
    /// An item to rename as soon as the listing that holds it arrives.
    rename_when_listed: Option<PathBuf>,
    ssh_hosts: Vec<String>,
    projects: std::collections::HashMap<PathBuf, crate::project::Kind>,
    project_scan_id: u64,
    /// The commands the open context menu offers, and the folder to run them in.
    menu_tasks: Option<(PathBuf, Vec<crate::project::Task>)>,
    git: Option<crate::git::Repo>,
    git_scan_id: u64,
    git_tick: u8,
    git_slow: bool,
    dir_sizes: std::collections::HashMap<PathBuf, u64>,
    sizing: std::collections::HashSet<PathBuf>,
    size_gen: u64,
    /// A listing was just made: look up what it needs, in the background.
    want_scans: bool,
    info_open: bool,
    info: Option<scans::InfoState>,
    jobs: Vec<ops::RunningJob>,
    next_job: u64,
    notice: Option<(String, std::time::Instant)>,
    undo: crate::undo::UndoStack,
    update: updates::UpdateState,
    /// Terminal apps found when the settings page was opened.
    terminals: Vec<AppInfo>,
    /// Code editors found when the settings page was opened.
    editors: Vec<AppInfo>,
    /// Modification time of the folder when it was listed, to spot changes.
    listed_at: Option<std::time::SystemTime>,
    renaming: Option<Renaming>,
    places: Vec<Place>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
}

impl Browser {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/".into());
        let home_name = home
            .file_name()
            .map_or_else(|| "Home".into(), |n| n.to_string_lossy().into_owned());

        let place = |name: &str, icon, path: PathBuf| Place {
            name: name.to_string().into(),
            icon,
            path,
        };
        let places = [
            place(&home_name, "icons/home.svg", home.clone()),
            place("Desktop", "icons/desktop.svg", home.join("Desktop")),
            place("Documents", "icons/file.svg", home.join("Documents")),
            place("Downloads", "icons/downloads.svg", home.join("Downloads")),
            place(
                "Applications",
                "icons/applications.svg",
                "/Applications".into(),
            ),
        ]
        .into_iter()
        .filter(|p| p.path.is_dir())
        .collect();

        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();

        let focus = cx.focus_handle();
        window.focus(&focus);

        let config = Config::load();
        crate::theme::apply_native_appearance(config.theme);

        // Start folder: `ExploRust /some/path`, else the last one if asked, else home.
        let start = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .or_else(|| {
                config
                    .reopen_last
                    .then(|| config.last_folder.clone())
                    .flatten()
            })
            .filter(|p| p.is_dir())
            .unwrap_or(home);

        let mut this = Self {
            cwd: start,
            all: Vec::new(),
            entries: Vec::new(),
            query: TextField::empty(),
            search_focused: false,
            selected: BTreeSet::new(),
            anchor: None,
            cursor: None,
            back: Vec::new(),
            forward: Vec::new(),
            sort: Sort::by(SortKey::Name).with_options(config.natural_sort, config.folders_on_top),
            dates: DateFormat::new(config.relative_dates),
            menu: None,
            settings_open: false,
            settings_scroll: gpui::ScrollHandle::new(),
            claude_ready: false,
            ai: None,
            remote: None,
            remote_gen: 0,
            remote_loading: false,
            remote_error: None,
            connect: None,
            rename_when_listed: None,
            ssh_hosts: crate::ssh::load_hosts(),
            projects: Default::default(),
            project_scan_id: 0,
            menu_tasks: None,
            git: None,
            git_scan_id: 0,
            git_tick: 0,
            git_slow: false,
            dir_sizes: Default::default(),
            sizing: Default::default(),
            size_gen: 0,
            want_scans: false,
            info_open: false,
            info: None,
            jobs: Vec::new(),
            next_job: 0,
            notice: None,
            undo: crate::undo::UndoStack::default(),
            update: updates::UpdateState::Idle,
            terminals: Vec::new(),
            editors: Vec::new(),
            listed_at: None,
            config,
            renaming: None,
            places,
            scroll: UniformListScrollHandle::new(),
            focus,
        };
        this.reload();

        if this.config.claude {
            this.check_claude_key(cx);
        }

        // `ExploRust --settings` opens the settings page.
        if std::env::args().any(|a| a == "--settings") {
            this.toggle_settings(cx);
        }

        // `ExploRust ssh://host/folder` opens a server straight away.
        if let Some(target) = std::env::args().nth(1).filter(|a| a.starts_with("ssh://"))
            && this.ssh_enabled()
        {
            this.connect_to(&target, cx);
        }

        // Look for a new version a few seconds after start, off the critical path.
        if this.config.check_updates {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(3))
                    .await;
                this.update_in(cx, |this, window, cx| {
                    this.check_for_updates(false, window, cx)
                })
                .ok();
            })
            .detach();
        }

        // Live refresh: look at the folder once a second (one `stat`, no watcher).
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(1))
                    .await;
                if this.update(cx, |this, cx| this.poll_changes(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        this
    }

    /// A click on a column header: sort by it, or flip the direction.
    fn set_sort(&mut self, key: SortKey, cx: &mut Context<Self>) {
        let keep = self.selected_paths();
        self.sort = self.sort.clicked(key);
        fs::sort_entries(&mut self.all, self.sort);
        fs::sort_entries(&mut self.entries, self.sort);
        // Rows moved: find the selected items again.
        self.select_paths(&keep);
        cx.notify();
    }

    fn reload(&mut self) {
        self.menu = None;
        self.renaming = None;
        // A server's folder is listed by `fetch_remote`; here it is only sorted again.
        if self.remote.is_some() {
            fs::sort_entries(&mut self.all, self.sort);
            self.apply_filter();
            return;
        }
        // An unreadable folder (permissions) just shows up empty.
        self.all = fs::read_dir(&self.cwd, self.config.show_hidden, self.sort).unwrap_or_default();
        for e in &mut self.all {
            if e.is_dir {
                e.dir_size = self.dir_sizes.get(&e.path).copied();
            }
        }
        self.want_scans = true;
        self.listed_at = dir_mtime(&self.cwd);
        self.apply_filter();
        self.remember_folder();
    }

    fn remember_folder(&mut self) {
        if self.config.reopen_last && self.config.last_folder.as_ref() != Some(&self.cwd) {
            self.config.last_folder = Some(self.cwd.clone());
            let _ = self.config.save();
        }
    }

    /// Reload when the folder changed on disk (added, removed or renamed items).
    fn poll_changes(&mut self, cx: &mut Context<Self>) {
        self.expire_notice(cx);
        if !self.settings_open && self.menu.is_none() {
            self.tick_git(cx);
        }
        let busy = self.renaming.is_some()
            || self.menu.is_some()
            || self.settings_open
            || self.remote.is_some();
        if !self.config.live_refresh || busy || dir_mtime(&self.cwd) == self.listed_at {
            return;
        }
        let keep = self.selected_paths();
        self.reload();
        // Keep the selection, without scrolling the list around.
        self.selected = (0..self.entries.len())
            .filter(|&ix| keep.contains(&self.entries[ix].path))
            .collect();
        self.anchor = self.selected.first().copied();
        self.cursor = self.selected.last().copied();
        cx.notify();
    }

    /// Apply the settings that need more than a repaint, and save them.
    fn update_config(&mut self, change: impl FnOnce(&mut Config), cx: &mut Context<Self>) {
        change(&mut self.config);
        // Not being able to save only means the change is forgotten at next launch.
        let _ = self.config.save();
        self.sort = self
            .sort
            .with_options(self.config.natural_sort, self.config.folders_on_top);
        self.dates = DateFormat::new(self.config.relative_dates);
        crate::theme::apply_native_appearance(self.config.theme);
        self.reload();
        cx.notify();
    }

    /// Rebuild the visible list from the search text.
    fn apply_filter(&mut self) {
        let needle = self.query.text().trim().to_lowercase();
        self.entries = if needle.is_empty() {
            self.all.clone()
        } else {
            self.all
                .iter()
                .filter(|e| e.matches(&needle))
                .cloned()
                .collect()
        };
        self.clear_selection();
    }

    /// Going somewhere else: what was looked up about the old folder is dropped.
    fn navigated(&mut self) {
        self.clear_search();
        self.dir_sizes.clear();
        self.sizing.clear();
        self.info = None;
        self.git = None;
        self.projects.clear();
    }

    /// Leaving a folder leaves its search behind, like Finder.
    fn clear_search(&mut self) {
        self.query.clear();
        self.search_focused = false;
    }

    fn go_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // A place on a server.
        if let Some((host, path)) = remote::split_location(&path) {
            return self.go_remote(host, path, cx);
        }
        if !path.is_dir() || (self.remote.is_none() && path == self.cwd) {
            return;
        }
        self.back.push(self.location());
        self.forward.clear();
        self.remote = None;
        self.cwd = path;
        self.navigated();
        self.reload();
        cx.notify();
    }

    /// Show a place from the history (local, or on a server).
    fn arrive(&mut self, place: PathBuf, cx: &mut Context<Self>) {
        if let Some((host, path)) = remote::split_location(&place) {
            return self.set_remote(host, path, cx);
        }
        self.remote = None;
        self.cwd = place;
        self.navigated();
        self.reload();
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(prev) = self.back.pop() {
            self.forward.push(self.location());
            self.arrive(prev, cx);
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.forward.pop() {
            self.back.push(self.location());
            self.arrive(next, cx);
        }
    }

    fn go_up(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.cwd.parent() {
            let from = self.cwd.clone();
            match self.remote.clone() {
                Some(host) => self.go_remote(host, parent.to_string_lossy().into_owned(), cx),
                None => self.go_to(parent.to_path_buf(), cx),
            }
            // Keep the folder we just left selected, like Finder.
            self.select_path(&from);
        }
    }

    fn select_path(&mut self, path: &Path) {
        if let Some(ix) = self.entries.iter().position(|e| e.path == path) {
            self.select_only(ix);
            self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
    }

    fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
        self.cursor = None;
    }

    fn select_only(&mut self, ix: usize) {
        self.selected.clear();
        self.selected.insert(ix);
        self.anchor = Some(ix);
        self.cursor = Some(ix);
    }

    /// Select everything between the anchor and `ix`.
    fn select_range_to(&mut self, ix: usize) {
        let anchor = self.anchor.unwrap_or(ix);
        self.selected = (anchor.min(ix)..=anchor.max(ix)).collect();
        self.anchor = Some(anchor);
        self.cursor = Some(ix);
    }

    fn select_all(&mut self) {
        self.selected = (0..self.entries.len()).collect();
        self.anchor = self.selected.first().copied();
        self.cursor = self.selected.last().copied();
    }

    /// Mouse click: ⌘ toggles, ⇧ extends, plain replaces.
    fn click(&mut self, ix: usize, m: Modifiers) {
        if m.platform {
            if !self.selected.remove(&ix) {
                self.selected.insert(ix);
            }
            self.anchor = Some(ix);
            self.cursor = Some(ix);
        } else if m.shift {
            self.select_range_to(ix);
        } else {
            self.select_only(ix);
        }
    }

    /// A single folder is entered; files are opened with their default app.
    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            return self.open_remote(cx);
        }
        let targets: Vec<(PathBuf, bool)> = self
            .selected
            .iter()
            .filter_map(|&ix| self.entries.get(ix))
            .map(|e| (e.path.clone(), e.is_dir))
            .collect();
        match targets.as_slice() {
            [(path, true)] => self.go_to(path.clone(), cx),
            _ => {
                for (path, _) in targets.iter().filter(|(_, is_dir)| !is_dir) {
                    cx.open_with_system(path);
                }
            }
        }
    }

    /// Open the right-click menu: file actions on a row, folder actions elsewhere.
    fn open_menu(&mut self, position: Point<Pixels>, on_item: bool) {
        if self.remote.is_some() {
            return self.open_remote_menu(position, on_item);
        }
        use MenuAction::*;
        let item = |label: &str, action| MenuEntry::Item(label.to_string().into(), action);
        // "Open in <editor>", "Open in <terminal>", and the list to pick others from.
        let editor = self
            .editor()
            .map(|e| item(&format!("Open in {}", e.name), OpenEditor));
        let terminal = item(&format!("Open in {}", self.terminal().name), OpenTerminal);
        let open_in = item("Open in…", OpenInChoices);
        let project = self.project_menu(on_item);
        let claude = self.claude_menu(on_item);
        let selected: Vec<&fs::Entry> = self
            .selected
            .iter()
            .filter_map(|&ix| self.entries.get(ix))
            .collect();
        let single = selected.len() == 1;
        let pasted = clipboard::files().len();
        let paste = (pasted > 0).then(|| {
            let label = match pasted {
                1 => "Paste Item".to_string(),
                n => format!("Paste {n} Items"),
            };
            item(&label, Paste)
        });

        let mut entries = Vec::new();
        if on_item {
            entries.push(item("Open", Open));
            if selected.iter().all(|e| !e.is_dir) {
                entries.push(item("Open With", OpenWith));
            }
            entries.push(item("Reveal in Finder", Reveal));
            entries.extend(editor);
            if single && selected[0].is_dir {
                entries.push(terminal);
            }
            entries.push(open_in);
            entries.push(MenuEntry::Separator);
            entries.push(item("Get Info", Info));
            entries.push(item("Copy", Copy));
            entries.push(item("Copy Path", CopyPath));
            if single && selected[0].is_dir {
                entries.extend(paste);
            }
            entries.push(item(
                &match selected.len() {
                    1 => "Compress".to_string(),
                    n => format!("Compress {n} Items"),
                },
                Compress,
            ));
            if single && crate::archive::is_archive(&selected[0].path) {
                entries.push(item("Extract", Extract));
            }
            if single {
                entries.push(item("Rename", Rename));
            }
            entries.extend(self.git_menu(true));
            entries.extend(project);
            entries.extend(claude);
            entries.push(MenuEntry::Separator);
            entries.push(item("Move to Trash", Trash));
        } else {
            let hidden = if self.config.show_hidden {
                "Hide Hidden Files"
            } else {
                "Show Hidden Files"
            };
            entries.push(item("New Folder", NewFolder));
            entries.push(item("New File", NewFile));
            entries.extend(paste);
            entries.extend(editor);
            entries.push(terminal);
            entries.push(open_in);
            entries.extend(self.git_menu(false));
            entries.extend(project);
            entries.extend(claude);
            entries.push(MenuEntry::Separator);
            entries.push(item(hidden, ToggleHidden));
        }
        self.menu = Some(ContextMenu {
            position,
            entries,
            submenu: None,
        });
    }

    /// The terminal to use: the user's choice if it still exists, else the system one.
    fn terminal(&self) -> AppInfo {
        self.config
            .terminal
            .as_deref()
            .filter(|p| p.exists())
            .and_then(AppInfo::from_path)
            .unwrap_or_else(apps::terminal)
    }

    /// A single selected folder, else the folder being browsed.
    fn target_dir(&self) -> PathBuf {
        match self.selected_paths().as_slice() {
            [p] if p.is_dir() => p.clone(),
            _ => self.cwd.clone(),
        }
    }

    /// The editor to use: the user's choice if it still exists, else the first
    /// one found on this Mac. `None` when there is none.
    fn editor(&self) -> Option<AppInfo> {
        self.config
            .editor
            .as_deref()
            .filter(|p| p.exists())
            .and_then(AppInfo::from_path)
            .or_else(|| apps::installed_editors().into_iter().next())
    }

    /// What "Open in <editor>" opens: the selection, or this folder if nothing is selected.
    fn editor_targets(&self) -> Vec<PathBuf> {
        let selected = self.selected_paths();
        if selected.is_empty() {
            vec![self.cwd.clone()]
        } else {
            selected
        }
    }

    /// Every selected item is a file (no folder).
    fn selection_is_files(&self) -> bool {
        !self.selected.is_empty()
            && self
                .selected
                .iter()
                .filter_map(|&ix| self.entries.get(ix))
                .all(|e| !e.is_dir)
    }

    /// Expand (or collapse) a submenu; apps are looked up on first use.
    fn toggle_submenu(&mut self, kind: SubmenuKind, cx: &mut Context<Self>) {
        let expanded = self
            .menu
            .as_ref()
            .and_then(|m| m.submenu.as_ref())
            .map(|sm| sm.kind);
        let submenu = if expanded == Some(kind) {
            None
        } else {
            match kind {
                SubmenuKind::Files => {
                    let Some(file) = self
                        .selected
                        .first()
                        .and_then(|&ix| self.entries.get(ix))
                        .map(|e| e.path.clone())
                    else {
                        return;
                    };
                    Some(Submenu {
                        kind,
                        sections: vec![Section {
                            title: None,
                            role: None,
                            apps: apps::apps_for(&file),
                            default: apps::default_app(&file).map(|a| a.path),
                        }],
                        subject: file,
                    })
                }
                SubmenuKind::OpenIn => {
                    let mut sections = Vec::new();
                    let editors = apps::installed_editors();
                    let default = self.editor().map(|e| e.path);
                    sections.push(Section {
                        title: Some("Editors"),
                        role: Some(Role::Editor),
                        apps: with_default_first(editors, default.clone()),
                        default,
                    });
                    // Terminals open folders, not files.
                    if !self.selection_is_files() {
                        let default = self.terminal();
                        sections.push(Section {
                            title: Some("Terminals"),
                            role: Some(Role::Terminal),
                            apps: with_default_first(
                                apps::installed_terminals(),
                                Some(default.path.clone()),
                            ),
                            default: Some(default.path),
                        });
                    }
                    Some(Submenu {
                        kind,
                        sections,
                        subject: self.target_dir(),
                    })
                }
            }
        };
        if let Some(menu) = self.menu.as_mut() {
            menu.submenu = submenu;
        }
        cx.notify();
    }

    /// A submenu row was clicked: open with that app, once.
    fn open_with_app(&mut self, role: Option<Role>, app: &Path, cx: &mut Context<Self>) {
        let Some(sm) = self.menu.take().and_then(|m| m.submenu) else {
            return;
        };
        match role {
            None => apps::open_with(&self.selected_paths(), app),
            Some(Role::Editor) => apps::open_with(&self.editor_targets(), app),
            Some(Role::Terminal) => apps::open_with(&[sm.subject], app),
        }
        cx.notify();
    }

    /// "Set as Default" on a submenu row.
    fn set_default_app(&mut self, role: Option<Role>, app: &Path, cx: &mut Context<Self>) {
        let Some(sm) = self.menu.take().and_then(|m| m.submenu) else {
            return;
        };
        match role {
            None => apps::set_default(app, &sm.subject),
            Some(role) => self.set_preferred(role, app),
        }
        cx.notify();
    }

    /// Remember the editor or terminal the user prefers.
    fn set_preferred(&mut self, role: Role, app: &Path) {
        match role {
            Role::Editor => self.config.editor = Some(app.to_path_buf()),
            Role::Terminal => self.config.terminal = Some(app.to_path_buf()),
        }
        // Not being able to save only means the choice is forgotten at next launch.
        let _ = self.config.save();
    }

    /// "Choose…": pick any app with the system dialog; it becomes the default
    /// and opens what was asked right away.
    fn choose_app(&mut self, role: Role, cx: &mut Context<Self>) {
        let Some(sm) = self.menu.take().and_then(|m| m.submenu) else {
            return;
        };
        let targets = match role {
            Role::Editor => self.editor_targets(),
            Role::Terminal => vec![sm.subject],
        };
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = picked.await
                && let Some(app) = paths
                    .first()
                    .filter(|p| p.extension().is_some_and(|e| e == "app"))
            {
                this.update(cx, |this, cx| {
                    this.set_preferred(role, app);
                    apps::open_with(&targets, app);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.notify();
    }

    fn run_action(&mut self, action: MenuAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            MenuAction::OpenWith => return self.toggle_submenu(SubmenuKind::Files, cx),
            MenuAction::OpenInChoices => return self.toggle_submenu(SubmenuKind::OpenIn, cx),
            _ => {}
        }
        self.menu = None;
        match action {
            MenuAction::Open => self.open_selected(cx),
            MenuAction::Reveal => {
                for path in self.selected_paths() {
                    cx.reveal_path(&path);
                }
            }
            MenuAction::Copy => self.copy_selection(),
            MenuAction::CopyPath => self.copy_selection_path(cx),
            MenuAction::Paste => {
                let dest = self.target_dir();
                self.paste(&dest, false, cx)
            }
            MenuAction::OpenWith | MenuAction::OpenInChoices => {}
            MenuAction::OpenTerminal => {
                apps::open_with(&[self.target_dir()], &self.terminal().path)
            }
            MenuAction::OpenEditor => {
                if let Some(editor) = self.editor() {
                    apps::open_with(&self.editor_targets(), &editor.path);
                }
            }
            MenuAction::Info => self.toggle_info(cx),
            a @ (MenuAction::ClaudeExplain
            | MenuAction::ClaudeName
            | MenuAction::ClaudeSummarize
            | MenuAction::ClaudeOrganize) => self.claude_task(a, cx),
            MenuAction::RemoteOpen => self.open_remote(cx),
            MenuAction::RemoteDownload => self.download_remote(cx),
            MenuAction::RemoteDelete => return self.remote_delete(window, cx),
            MenuAction::RemoteNewFolder => self.remote_new_folder(cx),
            MenuAction::RemoteTerminal => self.remote_terminal(cx),
            MenuAction::RemoteRefresh => self.fetch_remote(cx),
            MenuAction::RunTask(i) => self.run_task(i, cx),
            MenuAction::GitStage => self.git_stage(true, cx),
            MenuAction::GitUnstage => self.git_stage(false, cx),
            MenuAction::GitDiscard => return self.git_discard(window, cx),
            MenuAction::GitWeb => self.git_web(cx),
            MenuAction::Compress => self.compress_selection(cx),
            MenuAction::Extract => self.extract_selection(cx),
            MenuAction::Rename => self.begin_rename(),
            MenuAction::Trash => return self.trash_selection(window, cx),
            MenuAction::NewFolder => self.create_item(fs::create_folder),
            MenuAction::NewFile => self.create_item(fs::create_file),
            MenuAction::ToggleHidden => {
                let show = !self.config.show_hidden;
                self.update_config(|c| c.show_hidden = show, cx);
            }
        }
        cx.notify();
    }

    /// Create something in the current folder, then let the user name it.
    fn create_item(&mut self, create: fn(&Path) -> std::io::Result<PathBuf>) {
        if let Ok(path) = create(&self.cwd) {
            self.undo
                .record("New item", UndoOp::Trash(vec![path.clone()]));
            self.reload();
            self.select_path(&path);
            self.begin_rename();
        }
    }

    /// ⌘⌫: move the selection to the Trash. A big folder asks first.
    fn trash_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self.selected_paths();
        // `None`: the user turned the warning off.
        let limit = self.config.trash_warning;
        let big: Vec<String> = paths
            .iter()
            .filter(|p| p.is_dir())
            .filter(|p| limit.is_some_and(|n| fs::count_items(p, n) > n))
            .filter_map(|p| p.file_name())
            .map(|n| format!("“{}”", n.to_string_lossy()))
            .collect();
        if paths.is_empty() {
            return;
        }
        if big.is_empty() {
            return self.trash_paths(&paths, cx);
        }

        let message = match paths.as_slice() {
            [_] => format!("Move {} to the Trash?", big[0]),
            all => format!("Move these {} items to the Trash?", all.len()),
        };
        let detail = format!(
            "{} contain{} more than {} items. You can get them back from the Trash.",
            big.join(", "),
            if big.len() == 1 { "s" } else { "" },
            limit.unwrap_or_default(),
        );
        // "Cancel" comes first so it is the button Return would press.
        let answer = window.prompt(
            PromptLevel::Warning,
            &message,
            Some(&detail),
            &["Cancel", "Move to Trash"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if let Ok(1) = answer.await {
                this.update(cx, |this, cx| this.trash_paths(&paths, cx))
                    .ok();
            }
        })
        .detach();
    }

    fn trash_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        // Where each item went in the Trash, so that ⌘Z can bring it back.
        let trashed: Vec<(PathBuf, PathBuf)> = paths
            .iter()
            .filter_map(|p| crate::trash::move_to_trash(p).map(|t| (t, p.clone())))
            .collect();
        if !trashed.is_empty() {
            self.undo.record("Move to Trash", UndoOp::MoveBack(trashed));
        }
        self.reload();
        cx.notify();
    }

    /// Start renaming the selected row, if exactly one is selected.
    fn begin_rename(&mut self) {
        let mut ixs = self.selected.iter();
        if let (Some(&ix), None) = (ixs.next(), ixs.next())
            && let Some(entry) = self.entries.get(ix)
        {
            self.renaming = Some(Renaming {
                ix,
                field: TextField::for_file_name(&entry.name),
            });
            self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
    }

    /// Apply the typed name. An invalid or taken name is dropped silently.
    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.renaming.take() else {
            return;
        };
        if self.remote.is_some() {
            if let Some(entry) = self.entries.get(r.ix)
                && r.field.text() != entry.name.as_ref()
            {
                let (from, name) = (entry.path.clone(), r.field.text().to_string());
                self.remote_rename(from, name, cx);
            }
            return;
        }
        if let Some(entry) = self.entries.get(r.ix)
            && r.field.text() != entry.name.as_ref()
            && let Ok(new_path) = fs::rename(&entry.path, r.field.text())
        {
            self.undo.record(
                "Rename",
                UndoOp::MoveBack(vec![(new_path.clone(), entry.path.clone())]),
            );
            self.reload();
            self.select_path(&new_path);
        }
        cx.notify();
    }

    fn on_rename_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(r) = self.renaming.as_mut() else {
            return;
        };
        let paste = || cx.read_from_clipboard().and_then(|c| c.text());
        match r.field.handle_key(&ev.keystroke, paste) {
            KeyResult::Enter => return self.commit_rename(cx),
            KeyResult::Escape => self.renaming = None,
            KeyResult::Edited | KeyResult::Ignored => {}
        }
        cx.notify();
    }

    /// Keys while the search box has the focus.
    fn on_search_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        match ev.keystroke.key.as_str() {
            // Hand over to the list, on its first row.
            "down" => {
                self.search_focused = false;
                self.move_selection(1, false, cx);
                return;
            }
            "f" if ev.keystroke.modifiers.platform => return,
            _ => {}
        }
        let paste = || cx.read_from_clipboard().and_then(|c| c.text());
        match self.query.handle_key(&ev.keystroke, paste) {
            KeyResult::Edited => self.apply_filter(),
            KeyResult::Enter => self.search_focused = false,
            KeyResult::Escape => {
                self.clear_search();
                self.apply_filter();
            }
            KeyResult::Ignored => {}
        }
        cx.notify();
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected
            .iter()
            .filter_map(|&ix| self.entries.get(ix))
            .map(|e| e.path.clone())
            .collect()
    }

    /// Move (or copy) dropped paths into `dest`, in the background.
    fn drop_paths(&mut self, paths: &[PathBuf], dest: &Path, copy: bool, cx: &mut Context<Self>) {
        if self.remote.is_some() {
            // Dropping files on a server's folder sends them there.
            let into = dest.to_string_lossy().into_owned();
            return self.upload_items(paths.to_vec(), into, cx);
        }
        self.transfer_items(paths.to_vec(), dest.to_path_buf(), copy, cx);
    }

    fn select_paths(&mut self, paths: &[PathBuf]) {
        self.selected = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| paths.contains(&e.path))
            .map(|(ix, _)| ix)
            .collect();
        self.anchor = self.selected.first().copied();
        self.cursor = self.selected.last().copied();
        if let Some(&ix) = self.selected.first() {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
    }

    /// ⌘C: put the selected files on the system pasteboard.
    fn copy_selection(&mut self) {
        let paths = self.selected_paths();
        if !paths.is_empty() {
            clipboard::copy_files(&paths);
        }
    }

    fn copy_selection_path(&mut self, cx: &mut Context<Self>) {
        let text = self
            .selected_paths()
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Copy (or move, with `mv`) the pasteboard files into `dest`.
    fn paste(&mut self, dest: &Path, mv: bool, cx: &mut Context<Self>) {
        let files = clipboard::files();
        if !files.is_empty() {
            self.drop_paths(&files, dest, !mv, cx);
        }
    }

    /// Make `el` a drop target: dragged rows are moved in (⌥ copies),
    /// files dropped from other apps are copied in.
    fn drop_target<E: InteractiveElement>(
        el: E,
        dest: PathBuf,
        t: Theme,
        cx: &mut Context<Self>,
    ) -> E {
        let over = Hsla::from(t.accent).opacity(0.25);
        let internal = dest.clone();
        el.drag_over::<DragPaths>(move |s, _, _, _| s.bg(over))
            .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(over))
            .on_drop(cx.listener(move |this, d: &DragPaths, window, cx| {
                this.drop_paths(&d.0, &internal, window.modifiers().alt, cx)
            }))
            .on_drop(cx.listener(move |this, d: &ExternalPaths, _, cx| {
                this.drop_paths(d.paths(), &dest, true, cx)
            }))
    }

    fn move_selection(&mut self, delta: isize, extend: bool, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() - 1;
        let ix = match self.cursor {
            None if delta > 0 => 0,
            None => last,
            Some(cur) => cur.saturating_add_signed(delta).min(last),
        };
        if extend {
            self.select_range_to(ix);
        } else {
            self.select_only(ix);
        }
        let strategy = if delta > 0 {
            ScrollStrategy::Bottom
        } else {
            ScrollStrategy::Top
        };
        self.scroll.scroll_to_item(ix, strategy);
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.ai.is_some() {
            return self.on_ai_key(ev, cx);
        }
        if self.connect.is_some() {
            return self.on_connect_key(ev, cx);
        }
        if self.renaming.is_some() {
            return self.on_rename_key(ev, cx);
        }
        if self.search_focused {
            return self.on_search_key(ev, cx);
        }
        // ⌘, toggles the settings (an action, see main.rs); Escape closes them.
        let k = &ev.keystroke;
        if self.settings_open {
            if k.key == "escape" {
                self.toggle_settings(cx);
            }
            return;
        }
        if k.key == "k" && k.modifiers.platform {
            return self.open_connect(cx);
        }
        if self.remote.is_some() {
            return self.on_remote_key(ev, window, cx);
        }
        let k = &ev.keystroke;
        let cmd = k.modifiers.platform;
        match k.key.as_str() {
            "up" if cmd => self.go_up(cx),
            "down" | "o" if cmd => self.open_selected(cx),
            "up" => self.move_selection(-1, k.modifiers.shift, cx),
            "down" => self.move_selection(1, k.modifiers.shift, cx),
            // Like Finder: Return renames, ⌘O / ⌘↓ opens.
            "enter" => {
                self.begin_rename();
                cx.notify();
            }
            "f" if cmd => {
                self.search_focused = true;
                cx.notify();
            }
            "a" if cmd => {
                self.select_all();
                cx.notify();
            }
            "n" if cmd && k.modifiers.shift => {
                self.create_item(fs::create_folder);
                cx.notify();
            }
            "backspace" if cmd => self.trash_selection(window, cx),
            // ⌘⌥C copies the path as text, like Finder.
            "i" if cmd => self.toggle_info(cx),
            "z" if cmd && k.modifiers.shift => self.redo_last(cx),
            "z" if cmd => self.undo_last(cx),
            "c" if cmd && k.modifiers.alt => self.copy_selection_path(cx),
            "c" if cmd => self.copy_selection(),
            // ⌘⌥V moves instead of copying, like Finder's "Move Item Here".
            // Keyboard paste always goes to the folder being browsed.
            "v" if cmd => {
                let dest = self.cwd.clone();
                self.paste(&dest, k.modifiers.alt, cx)
            }
            "escape" => {
                if self.menu.take().is_none() {
                    if self.selected.is_empty() && !self.query.is_empty() {
                        self.clear_search();
                        self.apply_filter();
                    } else {
                        self.clear_selection();
                    }
                }
                cx.notify();
            }
            "[" if cmd => self.go_back(cx),
            "]" if cmd => self.go_forward(cx),
            "." if cmd && k.modifiers.shift => {
                let show = !self.config.show_hidden;
                self.update_config(|c| c.show_hidden = show, cx);
            }
            _ => {}
        }
    }

    fn title(&self) -> SharedString {
        if let Some(host) = &self.remote {
            let name = self
                .cwd
                .file_name()
                .map_or_else(|| "/".to_string(), |n| n.to_string_lossy().into_owned());
            return format!("{host}: {name}").into();
        }
        self.cwd
            .file_name()
            .map_or_else(
                || "Macintosh HD".into(),
                |n| n.to_string_lossy().into_owned(),
            )
            .into()
    }
}

impl Focusable for Browser {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Browser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::resolve(&self.config, window.appearance());
        if std::mem::take(&mut self.want_scans) {
            self.start_scans(cx);
        }
        self.sync_info(cx);

        div()
            .id("root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_key(ev, window, cx)))
            // From the menu bar (and ⌘, for the settings).
            .on_action(cx.listener(|this, _: &crate::OpenSettings, _, cx| this.toggle_settings(cx)))
            .on_action(cx.listener(|this, _: &crate::CheckForUpdates, window, cx| {
                this.check_for_updates(true, window, cx)
            }))
            // A click anywhere while renaming applies the new name and nothing else.
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                // Clicking elsewhere leaves the search box; clicking it focuses it again.
                if this.search_focused {
                    this.search_focused = false;
                    cx.notify();
                }
                if this.renaming.is_some() {
                    this.commit_rename(cx);
                    cx.stop_propagation();
                }
            }))
            // Side mouse buttons: back / forward.
            .on_mouse_up(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, _, cx| this.go_back(cx)),
            )
            .on_mouse_up(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, _, cx| this.go_forward(cx)),
            )
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.text)
            .text_size(px(13.0))
            .child(self.sidebar(t, cx))
            .child(if self.settings_open {
                self.settings_page(t, cx).into_any_element()
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(self.toolbar(t, cx))
                            .child(self.list(t, cx))
                            .child(self.status_bar(t, cx)),
                    )
                    .when(self.info_open, |d| d.child(self.info_panel(t, cx)))
                    .into_any_element()
            })
            .children(self.context_menu(t, cx))
            .children(self.connect_dialog(t, cx))
            .children(self.ai_dialog(t, cx))
    }
}

impl Browser {
    fn context_menu(&self, t: Theme, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let menu = self.menu.as_ref()?;
        let panel = |w: f32| {
            div()
                .w(px(w))
                .p_1()
                .flex_none()
                .flex()
                .flex_col()
                .rounded_lg()
                .bg(t.bg)
                .border_1()
                .border_color(t.border)
                .shadow_lg()
        };

        // Vertical offset of the row that opened the submenu, so both line up.
        let trigger = menu.submenu.as_ref().map(|sm| match sm.kind {
            SubmenuKind::Files => MenuAction::OpenWith,
            SubmenuKind::OpenIn => MenuAction::OpenInChoices,
        });
        let mut submenu_y = 0.0;
        for entry in &menu.entries {
            match entry {
                MenuEntry::Item(_, a) if trigger.is_some_and(|t| t.is(*a)) => break,
                MenuEntry::Item(..) => submenu_y += MENU_ITEM_H,
                MenuEntry::Separator => submenu_y += MENU_SEPARATOR_H,
            }
        }

        let entries = menu
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| match entry {
                MenuEntry::Separator => div()
                    .h(px(1.0))
                    .mx_2()
                    .my_1()
                    .bg(t.border)
                    .into_any_element(),
                MenuEntry::Item(label, action) => {
                    let action = *action;
                    let expanded = trigger.is_some_and(|t| t.is(action));
                    div()
                        .id(("menu-item", i))
                        .h(px(MENU_ITEM_H))
                        .px_3()
                        .flex()
                        .items_center()
                        .justify_between()
                        .rounded_md()
                        .when(expanded, |d| d.bg(t.hover))
                        .hover(|d| d.bg(t.accent).text_color(t.on_accent))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_action(action, window, cx)
                        }))
                        .child(label.clone())
                        .when(action.opens_submenu(), |d| d.child("›"))
                        .into_any_element()
                }
            });

        let submenu = menu.submenu.as_ref().map(|sm| {
            let mut rows: Vec<AnyElement> = Vec::new();
            for (si, section) in sm.sections.iter().enumerate() {
                if let Some(title) = section.title {
                    rows.push(
                        div()
                            .h(px(MENU_ITEM_H))
                            .px_3()
                            .flex()
                            .items_center()
                            .text_size(px(11.0))
                            .text_color(t.dim)
                            .child(title.to_uppercase())
                            .into_any_element(),
                    );
                }
                let role = section.role;
                for (i, app) in section.apps.iter().enumerate() {
                    let is_default = section.default.as_ref() == Some(&app.path);
                    let key = si * 100 + i;
                    let group: SharedString = format!("app-{key}").into();
                    let (open_path, default_path) = (app.path.clone(), app.path.clone());
                    rows.push(
                        div()
                            .id(("app", key))
                            .group(group.clone())
                            .h(px(MENU_ITEM_H))
                            .px_3()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .rounded_md()
                            .hover(|d| d.bg(t.accent).text_color(t.on_accent))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_with_app(role, &open_path, cx)
                            }))
                            .child(div().min_w_0().truncate().child(app.name.clone()))
                            .child(if is_default {
                                div()
                                    .flex_none()
                                    .text_size(px(11.0))
                                    .opacity(0.6)
                                    .child("Default")
                                    .into_any_element()
                            } else {
                                // Only revealed while the row is hovered.
                                div()
                                    .id(("set-default", key))
                                    .flex_none()
                                    .px_1()
                                    .rounded_sm()
                                    .text_size(px(11.0))
                                    .opacity(0.0)
                                    .group_hover(group, |s| s.opacity(1.0))
                                    .hover(|d| d.bg(Hsla::from(t.on_accent).opacity(0.25)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.set_default_app(role, &default_path, cx)
                                    }))
                                    .child("Set as Default")
                                    .into_any_element()
                            })
                            .into_any_element(),
                    );
                }
                if section.apps.is_empty() {
                    rows.push(
                        div()
                            .h(px(MENU_ITEM_H))
                            .px_3()
                            .flex()
                            .items_center()
                            .text_color(t.dim)
                            .child("None installed")
                            .into_any_element(),
                    );
                }
                // Editors and terminals can be picked from the whole Mac.
                if let Some(role) = role {
                    let label = match role {
                        Role::Editor => "Choose Editor…",
                        Role::Terminal => "Choose Terminal…",
                    };
                    rows.push(
                        div()
                            .id(("choose-app", si))
                            .h(px(MENU_ITEM_H))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .hover(|d| d.bg(t.accent).text_color(t.on_accent))
                            .on_click(cx.listener(move |this, _, _, cx| this.choose_app(role, cx)))
                            .child(label)
                            .into_any_element(),
                    );
                }
                if si + 1 < sm.sections.len() {
                    rows.push(
                        div()
                            .h(px(1.0))
                            .mx_2()
                            .my_1()
                            .bg(t.border)
                            .into_any_element(),
                    );
                }
            }
            panel(290.0).mt(px(submenu_y)).children(rows)
        });

        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        // One wrapper around both panels, so a click in the submenu
                        // does not count as "outside the menu".
                        div()
                            .occlude()
                            .flex()
                            .items_start()
                            .gap_1()
                            .text_color(t.text)
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.menu = None;
                                cx.notify();
                            }))
                            .child(panel(MENU_W).children(entries))
                            .children(submenu),
                    ),
            )
            .with_priority(1),
        )
    }

    fn sidebar(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.places.iter().enumerate().map(|(i, place)| {
            let active = place.path == self.cwd && !self.settings_open;
            let target = place.path.clone();
            let el = div()
                .id(("place", i))
                .h(px(28.0))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded_md()
                .cursor_pointer()
                .when(active, |d| d.bg(t.hover))
                .hover(|d| d.bg(t.hover))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.settings_open = false;
                    this.go_to(target.clone(), cx);
                    cx.notify();
                }))
                .child(icon(place.icon, t.folder))
                .child(place.name.clone());
            // Dropping on a local place makes no sense while a server is shown.
            if self.remote.is_some() {
                el
            } else {
                Self::drop_target(el, place.path.clone(), t, cx)
            }
        });

        div()
            .w(px(SIDEBAR_W))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .bg(t.sidebar)
            .border_r_1()
            .border_color(t.border)
            // Room for the traffic lights; also draggable.
            .child(
                div()
                    .h(px(TOOLBAR_H + 8.0))
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(div().px_2().flex().flex_col().gap_0p5().children(items))
            .children(self.servers_section(t, cx))
            // Pushes the settings button to the bottom.
            .child(div().flex_1())
            .child(
                div().p_2().child(
                    div()
                        .id("settings-button")
                        .h(px(28.0))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .cursor_pointer()
                        .text_color(if self.settings_open { t.text } else { t.dim })
                        .when(self.settings_open, |d| d.bg(t.hover))
                        .hover(|d| d.bg(t.hover).text_color(t.text))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_settings(cx)))
                        .child(icon(
                            "icons/settings.svg",
                            if self.settings_open { t.text } else { t.dim },
                        ))
                        .child("Settings"),
                ),
            )
            .child(
                div()
                    .px_4()
                    .pb_3()
                    .text_size(px(11.0))
                    .text_color(t.dim)
                    .child(format!("ExploRust v{}", crate::updater::VERSION)),
            )
    }

    fn toolbar(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let nav = |id: &'static str, label: &'static str, enabled: bool| {
            div()
                .id(id)
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .text_size(px(20.0))
                .text_color(if enabled { t.text } else { t.border })
                .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(t.hover)))
                .child(label)
        };

        div()
            .h(px(TOOLBAR_H))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(t.border)
            .window_control_area(WindowControlArea::Drag)
            .child(
                nav("back", "‹", !self.back.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.go_back(cx))),
            )
            .child(
                nav("forward", "›", !self.forward.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.go_forward(cx))),
            )
            .child(
                div()
                    .ml_2()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.title()),
            )
            .child(self.search_box(t, cx))
    }

    fn search_box(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.search_focused;
        div()
            .id("search")
            .w(px(SEARCH_W))
            .h(px(26.0))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1p5()
            .rounded_md()
            .bg(t.hover)
            .border_1()
            .border_color(if focused { t.accent } else { t.hover })
            .cursor_text()
            .on_click(cx.listener(|this, _, _, cx| {
                this.search_focused = true;
                cx.notify();
            }))
            .child(icon("icons/search.svg", t.dim).size(px(14.0)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(if self.query.is_empty() && !focused {
                        div().text_color(t.dim).child("Search").into_any_element()
                    } else {
                        text_runs(&self.query, focused, t).into_any_element()
                    }),
            )
            .when(!self.query.is_empty(), |d| {
                d.child(
                    div()
                        .id("search-clear")
                        .px_1()
                        .rounded_sm()
                        .text_color(t.dim)
                        .cursor_pointer()
                        .hover(|d| d.text_color(t.text))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.clear_search();
                            this.apply_filter();
                            cx.notify();
                        }))
                        .child("×"),
                )
            })
    }

    fn list(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let sort = self.sort;
        let column = |id: &'static str, key: SortKey, label: &'static str| {
            let active = sort.key == key;
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_1()
                .cursor_pointer()
                .when(active, |d| d.text_color(t.text))
                .hover(|d| d.text_color(t.text))
                .on_click(cx.listener(move |this, _, _, cx| this.set_sort(key, cx)))
                .child(label)
                .when(active, |d| {
                    let arrow = if sort.ascending {
                        "icons/sort-asc.svg"
                    } else {
                        "icons/sort-desc.svg"
                    };
                    d.child(
                        svg()
                            .path(arrow)
                            .size(px(9.0))
                            .flex_none()
                            .text_color(t.text),
                    )
                })
        };
        let name = column("sort-name", SortKey::Name, "Name");
        let modified = column("sort-modified", SortKey::Modified, "Date Modified");
        let size = column("sort-size", SortKey::Size, "Size");

        let header = div()
            .h(px(24.0))
            .flex_none()
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(12.0))
            .text_color(t.dim)
            .border_b_1()
            .border_color(t.border)
            .child(div().flex_1().flex().child(name))
            .when(self.git.is_some(), |d| {
                d.child(div().w(px(git_ui::GIT_W)).flex_none())
            })
            .child(div().w(px(DATE_W)).flex().child(modified))
            .child(div().w(px(SIZE_W)).flex().justify_end().child(size));

        let list = uniform_list(
            "entries",
            self.entries.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                range
                    .filter_map(|ix| {
                        let entry = this.entries.get(ix)?;
                        let selected = this.selected.contains(&ix);
                        let (fg, dim) = if selected {
                            (t.on_accent, t.on_accent)
                        } else {
                            (t.text, t.dim)
                        };
                        let icon = entry_icon(entry, t);
                        let is_dir = entry.is_dir;
                        let row_h = this.config.density.row_height();
                        let drag_paths = if selected {
                            this.selected_paths()
                        } else {
                            vec![entry.path.clone()]
                        };
                        let label: SharedString = match drag_paths.as_slice() {
                            [_] => entry.name.clone(),
                            all => format!("{} items", all.len()).into(),
                        };
                        Some(
                            div().w_full().px_2().child(
                                div()
                                    .id(ix)
                                    .h(px(row_h))
                                    .when(this.is_git_ignored(entry), |d| d.opacity(0.5))
                                    .w_full()
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .rounded_md()
                                    .text_color(fg)
                                    .when(selected, |d| d.bg(t.accent))
                                    .when(!selected, |d| d.hover(|d| d.bg(t.hover)))
                                    // Pressing an unselected row selects it, so a drag
                                    // carries what the user is pointing at.
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                            let m = ev.modifiers;
                                            if !this.selected.contains(&ix)
                                                && !m.platform
                                                && !m.shift
                                            {
                                                this.select_only(ix);
                                                cx.notify();
                                            }
                                        }),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                            if !this.selected.contains(&ix) {
                                                this.select_only(ix);
                                            }
                                            this.open_menu(ev.position, true);
                                            cx.stop_propagation();
                                            cx.notify();
                                        }),
                                    )
                                    // Items of a server are not dragged out (only dropped in).
                                    .when(this.remote.is_none(), |d| {
                                        d.on_drag(
                                            DragPaths(drag_paths),
                                            move |_, position, _, cx| {
                                                cx.new(|_| DragPreview {
                                                    label: label.clone(),
                                                    position,
                                                    theme: t,
                                                })
                                            },
                                        )
                                    })
                                    .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                                        let m = ev.modifiers();
                                        this.click(ix, m);
                                        let plain = !m.platform && !m.shift;
                                        // Double click, or a single click on a folder if asked.
                                        let single = this.config.single_click && is_dir;
                                        if plain && (ev.click_count() >= 2 || single) {
                                            this.open_selected(cx);
                                        }
                                        cx.notify();
                                    }))
                                    .when(entry.is_dir, |d| {
                                        Self::drop_target(d, entry.path.clone(), t, cx)
                                    })
                                    .child(icon)
                                    .child(match this.renaming.as_ref().filter(|r| r.ix == ix) {
                                        Some(r) => rename_input(&r.field, t).into_any_element(),
                                        None => div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .child(entry.name.clone()),
                                            )
                                            .children(this.project_tag(entry, selected, t))
                                            .into_any_element(),
                                    })
                                    .when(this.git.is_some(), |d| {
                                        d.child(this.git_cell(entry, selected, t))
                                    })
                                    .child(div().w(px(DATE_W)).truncate().text_color(dim).child(
                                        entry.modified.map_or_else(
                                            || {
                                                entry
                                                    .date_label
                                                    .clone()
                                                    .unwrap_or_else(|| "--".to_string())
                                            },
                                            |m| this.dates.format(m),
                                        ),
                                    ))
                                    .child(
                                        div()
                                            .w(px(SIZE_W))
                                            .text_right()
                                            .text_color(dim)
                                            .child(this.size_text(entry)),
                                    ),
                            ),
                        )
                    })
                    .collect()
            }),
        )
        .track_scroll(self.scroll.clone())
        .flex_1()
        .py_1();

        // Files dropped on the empty part of the list land in the current folder.
        let cwd = self.cwd.clone();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_drop(cx.listener(move |this, d: &ExternalPaths, _, cx| {
                this.drop_paths(d.paths(), &cwd, true, cx)
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    this.clear_selection();
                    this.open_menu(ev.position, false);
                    cx.notify();
                }),
            )
            .child(header)
            .child(if let Some(notice) = self.remote_notice(t, cx) {
                notice
            } else if self.entries.is_empty() && !self.query.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(t.dim)
                    .child("No results")
                    .into_any_element()
            } else {
                list.into_any_element()
            })
    }

    fn status_bar(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        // Clickable path: each component jumps to that ancestor.
        let mut crumbs = Vec::new();
        // On a server, places are written "ssh://host/path" (that is what going there takes).
        let mut acc = match &self.remote {
            Some(host) => PathBuf::from(format!("ssh://{host}/")),
            None => PathBuf::from("/"),
        };
        crumbs.push((
            self.remote
                .clone()
                .unwrap_or_else(|| "Macintosh HD".to_string()),
            acc.clone(),
        ));
        for part in self.cwd.components().skip(1) {
            acc.push(part);
            crumbs.push((part.as_os_str().to_string_lossy().into_owned(), acc.clone()));
        }

        let mut bar = div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .overflow_hidden();
        let last = crumbs.len() - 1;
        for (i, (name, path)) in crumbs.into_iter().enumerate() {
            if i > 0 {
                bar = bar.child("›");
            }
            let dest = path.clone();
            let crumb = div()
                .id(("crumb", i))
                .px_1()
                .rounded_sm()
                .whitespace_nowrap()
                .when(i == last, |d| d.text_color(t.text))
                .cursor_pointer()
                .hover(|d| d.bg(t.hover))
                .on_click(cx.listener(move |this, _, _, cx| this.go_to(path.clone(), cx)))
                .child(name);
            bar = if self.remote.is_some() {
                bar.child(crumb)
            } else {
                bar.child(Self::drop_target(crumb, dest, t, cx))
            };
        }

        let count = self.entries.len();
        let installing = match &self.update {
            updates::UpdateState::Installing(v) => Some(format!("Installing version {v}…")),
            _ => None,
        };
        let status = match self.selected.len() {
            0 if !self.query.is_empty() => match count {
                1 => "1 result".to_string(),
                n => format!("{n} results"),
            },
            0 => format!("{count} items"),
            n => format!("{n} of {count} selected"),
        };
        div()
            .h(px(28.0))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .text_size(px(12.0))
            .text_color(t.dim)
            .border_t_1()
            .border_color(t.border)
            .child(bar)
            // A running job (bar), or a short message, or the usual count.
            .children(self.git_status(t))
            .children(self.jobs_bar(t, cx))
            .child(
                div().flex_none().child(
                    installing
                        .or_else(|| self.current_notice().map(str::to_string))
                        .unwrap_or(status),
                ),
            )
    }
}

fn icon(name: &'static str, color: gpui::Rgba) -> gpui::Svg {
    svg()
        .path(name)
        .size(px(18.0))
        .flex_none()
        .text_color(color)
}

/// Icon for a list entry, picked from the file extension.
fn entry_icon(entry: &fs::Entry, t: Theme) -> gpui::Svg {
    if entry.is_dir {
        return icon("icons/folder.svg", t.folder);
    }
    let ext = entry
        .path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let name = match ext.as_str() {
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
    };
    icon(name, t.file)
}

/// Text with its selection highlighted and, if `caret`, the caret drawn.
fn text_runs(field: &TextField, caret: bool, t: Theme) -> impl IntoElement {
    let text = field.text();
    let (start, end) = field.selection();
    let bar = || div().w(px(1.0)).h(px(15.0)).flex_none().bg(t.accent);

    div()
        .flex()
        .items_center()
        .whitespace_nowrap()
        .child(text[..start].to_string())
        .when(caret && field.cursor() == start, |d| d.child(bar()))
        .child(
            div()
                .bg(Hsla::from(t.accent).opacity(0.3))
                .child(text[start..end].to_string()),
        )
        .when(caret && field.cursor() == end && start != end, |d| {
            d.child(bar())
        })
        .child(text[end..].to_string())
}

/// The text box shown in place of a name while renaming.
fn rename_input(field: &TextField, t: Theme) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .h(px(22.0))
        .px_1()
        .flex()
        .items_center()
        .overflow_hidden()
        .rounded_sm()
        .bg(t.bg)
        .text_color(t.text)
        .border_1()
        .border_color(t.accent)
        .child(text_runs(field, true, t))
}

/// When the folder's content last changed (items added, removed, renamed).
fn dir_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir).and_then(|m| m.modified()).ok()
}

/// `apps` with the default one first (added if it is not in the list, e.g. picked
/// by hand), the others in their order.
fn with_default_first(mut apps: Vec<AppInfo>, default: Option<PathBuf>) -> Vec<AppInfo> {
    if let Some(default) = default {
        if !apps.iter().any(|a| a.path == default)
            && let Some(app) = AppInfo::from_path(&default)
        {
            apps.push(app);
        }
        apps.sort_by_key(|a| a.path != default);
    }
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(path: &str) -> AppInfo {
        AppInfo::from_path(Path::new(path)).unwrap()
    }

    fn names(apps: &[AppInfo]) -> Vec<&str> {
        apps.iter().map(|a| a.name.as_str()).collect()
    }

    #[test]
    fn default_app_comes_first_and_others_keep_their_order() {
        let list = vec![app("/A/Code.app"), app("/A/Zed.app"), app("/A/Cursor.app")];
        let ordered = with_default_first(list, Some("/A/Zed.app".into()));
        assert_eq!(names(&ordered), ["Zed", "Code", "Cursor"]);
    }

    #[test]
    fn a_hand_picked_default_is_added_when_missing() {
        let list = vec![app("/A/Code.app")];
        let ordered = with_default_first(list, Some("/B/Nova.app".into()));
        assert_eq!(names(&ordered), ["Nova", "Code"]);
    }

    #[test]
    fn without_a_default_the_order_is_untouched() {
        let list = vec![app("/A/Code.app"), app("/A/Zed.app")];
        assert_eq!(names(&with_default_first(list, None)), ["Code", "Zed"]);
    }
}
