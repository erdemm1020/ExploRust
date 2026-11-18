use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use gpui::{
    ClickEvent, ClipboardItem, Context, ExternalPaths, FocusHandle, Focusable, Hsla, IntoElement,
    KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, NavigationDirection, Pixels, Point,
    Render, ScrollStrategy, SharedString, UniformListScrollHandle, Window, WindowControlArea,
    anchored, deferred, div, prelude::*, px, svg, uniform_list,
};

use crate::{fs, text_field::TextField, theme::Theme};

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
    CopyPath,
    Rename,
    Trash,
    NewFolder,
    ToggleHidden,
}

enum MenuEntry {
    Item(SharedString, MenuAction),
    Separator,
}

struct ContextMenu {
    position: Point<Pixels>,
    entries: Vec<MenuEntry>,
}

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
        let entries = if on_item {
            vec![
                item("Open", Open),
                item("Reveal in Finder", Reveal),
                MenuEntry::Separator,
                item("Copy Path", CopyPath),
            ]
            .into_iter()
            .chain((self.selected.len() == 1).then(|| item("Rename", Rename)))
            .chain([MenuEntry::Separator, item("Move to Trash", Trash)])
            .collect()
        } else {
            let hidden = if self.show_hidden {
                "Hide Hidden Files"
            } else {
                "Show Hidden Files"
            };
            vec![
                item("New Folder", NewFolder),
                MenuEntry::Separator,
                item(hidden, ToggleHidden),
            ]
        };
        self.menu = Some(ContextMenu { position, entries });
    }

    fn run_action(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        self.menu = None;
        match action {
            MenuAction::Open => self.open_selected(cx),
            MenuAction::Reveal => {
                for path in self.selected_paths() {
                    cx.reveal_path(&path);
                }
            }
            MenuAction::CopyPath => {
                let text = self
                    .selected_paths()
                    .iter()
                    .map(|p| p.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("\n");
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            MenuAction::Rename => self.begin_rename(),
            MenuAction::Trash => {
                for path in self.selected_paths() {
                    let _ = crate::trash::move_to_trash(&path);
                }
                self.reload();
            }
            MenuAction::NewFolder => {
                if let Ok(path) = fs::create_folder(&self.cwd) {
                    self.reload();
                    self.select_path(&path);
                    self.begin_rename();
                }
            }
            MenuAction::ToggleHidden => {
                self.show_hidden = !self.show_hidden;
                self.reload();
            }
        }
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
        for path in paths {
            // A failed item (permissions, ...) must not stop the others.
            let _ = fs::transfer(path, dest, copy);
        }
        self.reload();
        cx.notify();
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
                    div()
                        .id(("menu-item", i))
                        .h(px(24.0))
                        .px_3()
                        .flex()
                        .items_center()
                        .rounded_md()
                        .hover(|d| d.bg(t.accent).text_color(t.on_accent))
                        .on_click(cx.listener(move |this, _, _, cx| this.run_action(action, cx)))
                        .child(label.clone())
                        .into_any_element()
                }
            });

        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .occlude()
                            .w(px(210.0))
                            .p_1()
                            .flex()
                            .flex_col()
                            .rounded_lg()
                            .bg(t.bg)
                            .border_1()
                            .border_color(t.border)
                            .shadow_lg()
                            .text_color(t.text)
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.menu = None;
                                cx.notify();
                            }))
                            .children(entries),
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
