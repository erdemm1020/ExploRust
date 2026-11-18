use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use gpui::{
    ClickEvent, Context, ExternalPaths, FocusHandle, Focusable, Hsla, IntoElement, KeyDownEvent,
    Modifiers, MouseButton, MouseDownEvent, NavigationDirection, Pixels, Point, Render,
    ScrollStrategy, SharedString, UniformListScrollHandle, Window, WindowControlArea, div,
    prelude::*, px, svg, uniform_list,
};

use crate::{fs, theme::Theme};

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
            places,
            scroll: UniformListScrollHandle::new(),
            focus,
        };
        this.reload();
        this
    }

    fn reload(&mut self) {
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
        let k = &ev.keystroke;
        let cmd = k.modifiers.platform;
        match k.key.as_str() {
            "up" if cmd => self.go_up(cx),
            "down" if cmd => self.open_selected(cx),
            "up" => self.move_selection(-1, k.modifiers.shift, cx),
            "down" => self.move_selection(1, k.modifiers.shift, cx),
            "enter" => self.open_selected(cx),
            "a" if cmd => {
                self.select_all();
                cx.notify();
            }
            "escape" => {
                self.clear_selection();
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
    }
}

impl Browser {
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
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(entry.name.clone()),
                                    )
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
