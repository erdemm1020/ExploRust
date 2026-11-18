use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use gpui::{
    ClickEvent, ClipboardItem, Context, ExternalPaths, FocusHandle, Focusable, Hsla, IntoElement,
    KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, NavigationDirection, PathPromptOptions,
    Pixels, Point, Render, ScrollStrategy, SharedString, UniformListScrollHandle, Window,
    WindowControlArea, anchored, deferred, div, prelude::*, px, svg, uniform_list,
};

use crate::{
    apps::{self, AppInfo},
    clipboard,
    config::Config,
    fs,
    text_field::TextField,
    theme::Theme,
};

const TOOLBAR_H: f32 = 44.0;
const ROW_H: f32 = 26.0;
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
    TerminalChoices,
    Rename,
    Trash,
    NewFolder,
    NewFile,
    ToggleHidden,
}

impl MenuAction {
    fn opens_submenu(self) -> bool {
        matches!(self, Self::OpenWith | Self::TerminalChoices)
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
    /// Terminal apps to open a folder in.
    Terminals,
}

/// The expanded submenu: a list of apps, one of which is the default.
struct Submenu {
    kind: SubmenuKind,
    apps: Vec<AppInfo>,
    default: Option<PathBuf>,
    /// A file (its type gets the default app) or the folder to open in a terminal.
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
    entries: Vec<fs::Entry>,
    selected: BTreeSet<usize>,
    /// Fixed end of a shift-selection.
    anchor: Option<usize>,
    /// Moving end: where the arrow keys continue from.
    cursor: Option<usize>,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    show_hidden: bool,
    menu: Option<ContextMenu>,
    config: Config,
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

        // Optional start folder: `ExploRust /some/path`.
        let start = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or(home);

        let mut this = Self {
            cwd: start,
            entries: Vec::new(),
            selected: BTreeSet::new(),
            anchor: None,
            cursor: None,
            back: Vec::new(),
            forward: Vec::new(),
            show_hidden: false,
            menu: None,
            config: Config::load(),
            renaming: None,
            places,
            scroll: UniformListScrollHandle::new(),
            focus,
        };
        this.reload();
        this
    }

    fn reload(&mut self) {
        self.menu = None;
        self.renaming = None;
        // An unreadable folder (permissions) just shows up empty.
        self.entries = fs::read_dir(&self.cwd, self.show_hidden).unwrap_or_default();
        self.clear_selection();
    }

    fn go_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if path == self.cwd || !path.is_dir() {
            return;
        }
        self.back.push(std::mem::replace(&mut self.cwd, path));
        self.forward.clear();
        self.reload();
        cx.notify();
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(prev) = self.back.pop() {
            self.forward.push(std::mem::replace(&mut self.cwd, prev));
            self.reload();
            cx.notify();
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(next) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.cwd, next));
            self.reload();
            cx.notify();
        }
    }

    fn go_up(&mut self, cx: &mut Context<Self>) {
        if let Some(parent) = self.cwd.parent() {
            let from = self.cwd.clone();
            self.go_to(parent.to_path_buf(), cx);
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
        use MenuAction::*;
        let item = |label: &str, action| MenuEntry::Item(label.to_string().into(), action);
        let terminal = [
            item(&format!("Open in {}", self.terminal().name), OpenTerminal),
            item("Open in…", TerminalChoices),
        ];
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
            if single && selected[0].is_dir {
                entries.extend(terminal);
            }
            entries.push(MenuEntry::Separator);
            entries.push(item("Copy", Copy));
            entries.push(item("Copy Path", CopyPath));
            if single && selected[0].is_dir {
                entries.extend(paste);
            }
            if single {
                entries.push(item("Rename", Rename));
            }
            entries.push(MenuEntry::Separator);
            entries.push(item("Move to Trash", Trash));
        } else {
            let hidden = if self.show_hidden {
                "Hide Hidden Files"
            } else {
                "Show Hidden Files"
            };
            entries.push(item("New Folder", NewFolder));
            entries.push(item("New File", NewFile));
            entries.extend(paste);
            entries.extend(terminal);
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
                        apps: apps::apps_for(&file),
                        default: apps::default_app(&file).map(|a| a.path),
                        subject: file,
                    })
                }
                SubmenuKind::Terminals => {
                    let default = self.terminal();
                    let mut terminals = apps::installed_terminals();
                    // A terminal picked with "Choose…" is not in the known list.
                    if !terminals.iter().any(|t| t.path == default.path) {
                        terminals.push(default.clone());
                    }
                    terminals.sort_by_cached_key(|t| t.path != default.path);
                    Some(Submenu {
                        kind,
                        apps: terminals,
                        default: Some(default.path),
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
    fn open_with_app(&mut self, app: &Path, cx: &mut Context<Self>) {
        let Some(sm) = self.menu.take().and_then(|m| m.submenu) else {
            return;
        };
        match sm.kind {
            SubmenuKind::Files => apps::open_with(&self.selected_paths(), app),
            SubmenuKind::Terminals => apps::open_with(&[sm.subject], app),
        }
        cx.notify();
    }

    /// "Set as Default" on a submenu row.
    fn set_default_app(&mut self, app: &Path, cx: &mut Context<Self>) {
        let Some(sm) = self.menu.take().and_then(|m| m.submenu) else {
            return;
        };
        match sm.kind {
            SubmenuKind::Files => apps::set_default(app, &sm.subject),
            SubmenuKind::Terminals => self.set_terminal(app),
        }
        cx.notify();
    }

    fn set_terminal(&mut self, app: &Path) {
        self.config.terminal = Some(app.to_path_buf());
        // Not being able to save only means the choice is forgotten at next launch.
        let _ = self.config.save();
    }

    /// "Choose…": pick any app with the system dialog; it becomes the default
    /// terminal and opens the folder right away.
    fn choose_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(sm) = self.menu.take().and_then(|m| m.submenu) else {
            return;
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
                    this.set_terminal(app);
                    apps::open_with(&[sm.subject], app);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.notify();
    }

    fn run_action(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        match action {
            MenuAction::OpenWith => return self.toggle_submenu(SubmenuKind::Files, cx),
            MenuAction::TerminalChoices => {
                return self.toggle_submenu(SubmenuKind::Terminals, cx);
            }
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
            MenuAction::OpenWith | MenuAction::TerminalChoices => {}
            MenuAction::OpenTerminal => {
                apps::open_with(&[self.target_dir()], &self.terminal().path)
            }
            MenuAction::Rename => self.begin_rename(),
            MenuAction::Trash => {
                for path in self.selected_paths() {
                    let _ = crate::trash::move_to_trash(&path);
                }
                self.reload();
            }
            MenuAction::NewFolder => self.create_item(fs::create_folder),
            MenuAction::NewFile => self.create_item(fs::create_file),
            MenuAction::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                self.reload();
            }
        }
        cx.notify();
    }

    /// Create something in the current folder, then let the user name it.
    fn create_item(&mut self, create: fn(&Path) -> std::io::Result<PathBuf>) {
        if let Ok(path) = create(&self.cwd) {
            self.reload();
            self.select_path(&path);
            self.begin_rename();
        }
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
        if let Some(entry) = self.entries.get(r.ix)
            && r.field.text() != entry.name.as_ref()
            && let Ok(new_path) = fs::rename(&entry.path, r.field.text())
        {
            self.reload();
            self.select_path(&new_path);
        }
        cx.notify();
    }

    fn on_rename_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let (cmd, shift) = (k.modifiers.platform, k.modifiers.shift);
        let Some(r) = self.renaming.as_mut() else {
            return;
        };
        let f = &mut r.field;
        match k.key.as_str() {
            "enter" => return self.commit_rename(cx),
            "escape" => self.renaming = None,
            "backspace" => f.backspace(),
            "delete" => f.delete(),
            "left" if cmd => f.home(shift),
            "right" if cmd => f.end(shift),
            "left" => f.left(shift),
            "right" => f.right(shift),
            "a" if cmd => f.select_all(),
            "v" if cmd => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    f.insert(&text.replace(['\n', '/'], " "));
                }
            }
            _ if !cmd && !k.modifiers.control => {
                if let Some(ch) = k.key_char.as_deref()
                    && !ch.chars().any(char::is_control)
                {
                    f.insert(ch);
                }
            }
            _ => {}
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

    /// Move (or copy) dropped paths into `dest`, then refresh the listing.
    fn drop_paths(&mut self, paths: &[PathBuf], dest: &Path, copy: bool, cx: &mut Context<Self>) {
        // A failed item (permissions, ...) must not stop the others.
        let created: Vec<PathBuf> = paths
            .iter()
            .filter_map(|p| fs::transfer(p, dest, copy).ok())
            .collect();
        self.reload();
        // Like Finder, what just landed in this folder is selected.
        if dest == self.cwd {
            self.select_paths(&created);
        }
        cx.notify();
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

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.renaming.is_some() {
            return self.on_rename_key(ev, cx);
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
            "a" if cmd => {
                self.select_all();
                cx.notify();
            }
            "n" if cmd && k.modifiers.shift => {
                self.create_item(fs::create_folder);
                cx.notify();
            }
            // ⌘⌥C copies the path as text, like Finder.
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
                    self.clear_selection();
                }
                cx.notify();
            }
            "[" if cmd => self.go_back(cx),
            "]" if cmd => self.go_forward(cx),
            "." if cmd && k.modifiers.shift => {
                self.show_hidden = !self.show_hidden;
                self.reload();
                cx.notify();
            }
            _ => {}
        }
    }

    fn title(&self) -> SharedString {
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
        let t = Theme::for_appearance(window.appearance());

        div()
            .id("root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev, _, cx| this.on_key(ev, cx)))
            // A click anywhere while renaming applies the new name and nothing else.
            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
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
            .children(self.context_menu(t, cx))
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
            SubmenuKind::Terminals => MenuAction::TerminalChoices,
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
                        .on_click(cx.listener(move |this, _, _, cx| this.run_action(action, cx)))
                        .child(label.clone())
                        .when(action.opens_submenu(), |d| d.child("›"))
                        .into_any_element()
                }
            });

        let submenu = menu.submenu.as_ref().map(|sm| {
            let mut rows: Vec<_> = sm
                .apps
                .iter()
                .enumerate()
                .map(|(i, app)| {
                    let is_default = sm.default.as_ref() == Some(&app.path);
                    let group: SharedString = format!("app-{i}").into();
                    let (open_path, default_path) = (app.path.clone(), app.path.clone());
                    div()
                        .id(("app", i))
                        .group(group.clone())
                        .h(px(MENU_ITEM_H))
                        .px_3()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .rounded_md()
                        .hover(|d| d.bg(t.accent).text_color(t.on_accent))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.open_with_app(&open_path, cx)),
                        )
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
                                .id(("set-default", i))
                                .flex_none()
                                .px_1()
                                .rounded_sm()
                                .text_size(px(11.0))
                                .opacity(0.0)
                                .group_hover(group, |s| s.opacity(1.0))
                                .hover(|d| d.bg(Hsla::from(t.on_accent).opacity(0.25)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.set_default_app(&default_path, cx)
                                }))
                                .child("Set as Default")
                                .into_any_element()
                        })
                        .into_any_element()
                })
                .collect();

            if sm.kind == SubmenuKind::Terminals {
                rows.push(
                    div()
                        .h(px(1.0))
                        .mx_2()
                        .my_1()
                        .bg(t.border)
                        .into_any_element(),
                );
                rows.push(
                    div()
                        .id("choose-terminal")
                        .h(px(MENU_ITEM_H))
                        .px_3()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .hover(|d| d.bg(t.accent).text_color(t.on_accent))
                        .on_click(cx.listener(|this, _, _, cx| this.choose_terminal(cx)))
                        .child("Choose Application…")
                        .into_any_element(),
                );
            } else if rows.is_empty() {
                rows.push(
                    div()
                        .h(px(MENU_ITEM_H))
                        .px_3()
                        .flex()
                        .items_center()
                        .text_color(t.dim)
                        .child("No application")
                        .into_any_element(),
                );
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
            let active = place.path == self.cwd;
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
                .on_click(cx.listener(move |this, _, _, cx| this.go_to(target.clone(), cx)))
                .child(icon(place.icon, t.folder))
                .child(place.name.clone());
            Self::drop_target(el, place.path.clone(), t, cx)
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
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.title()),
            )
    }

    fn list(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let header = div()
            .h(px(24.0))
            .flex_none()
            .px_4()
            .flex()
            .items_center()
            .text_size(px(12.0))
            .text_color(t.dim)
            .border_b_1()
            .border_color(t.border)
            .child(div().flex_1().child("Name"))
            .child(div().w(px(90.0)).text_right().child("Size"));

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
                                    .h(px(ROW_H))
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
                                    .on_drag(DragPaths(drag_paths), move |_, position, _, cx| {
                                        cx.new(|_| DragPreview {
                                            label: label.clone(),
                                            position,
                                            theme: t,
                                        })
                                    })
                                    .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                                        let m = ev.modifiers();
                                        this.click(ix, m);
                                        if ev.click_count() >= 2 && !m.platform && !m.shift {
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
                                            .truncate()
                                            .child(entry.name.clone())
                                            .into_any_element(),
                                    })
                                    .child(
                                        div()
                                            .w(px(90.0))
                                            .text_right()
                                            .text_color(dim)
                                            .child(entry.size_label()),
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
            .child(list)
    }

    fn status_bar(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        // Clickable path: each component jumps to that ancestor.
        let mut crumbs = Vec::new();
        let mut acc = PathBuf::from("/");
        crumbs.push(("Macintosh HD".to_string(), acc.clone()));
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
            bar = bar.child(Self::drop_target(crumb, dest, t, cx));
        }

        let count = self.entries.len();
        let status = match self.selected.len() {
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
            .child(div().flex_none().child(status))
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

/// The text box shown in place of a name while renaming.
fn rename_input(field: &TextField, t: Theme) -> impl IntoElement {
    let text = field.text();
    let (start, end) = field.selection();
    let caret = || div().w(px(1.0)).h(px(15.0)).flex_none().bg(t.accent);

    div()
        .flex_1()
        .min_w_0()
        .h(px(22.0))
        .px_1()
        .flex()
        .items_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .rounded_sm()
        .bg(t.bg)
        .text_color(t.text)
        .border_1()
        .border_color(t.accent)
        .child(text[..start].to_string())
        .when(field.cursor() == start, |d| d.child(caret()))
        .child(
            div()
                .bg(Hsla::from(t.accent).opacity(0.3))
                .child(text[start..end].to_string()),
        )
        .when(field.cursor() == end && start != end, |d| d.child(caret()))
        .child(text[end..].to_string())
}
