use std::path::{Path, PathBuf};

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, Render, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, WindowControlArea, div, prelude::*, px, svg,
    uniform_list,
};

use crate::{fs, theme::Theme};

const TOOLBAR_H: f32 = 44.0;
const ROW_H: f32 = 26.0;
const SIDEBAR_W: f32 = 180.0;

struct Place {
    name: SharedString,
    icon: &'static str,
    path: PathBuf,
}

pub struct Browser {
    cwd: PathBuf,
    entries: Vec<fs::Entry>,
    selected: Option<usize>,
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

        let mut this = Self {
            cwd: home,
            entries: Vec::new(),
            selected: None,
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
        self.selected = None;
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
        self.selected = self.entries.iter().position(|e| e.path == path);
        if let Some(ix) = self.selected {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
    }

    fn open(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(entry) = self.entries.get(ix) else {
            return;
        };
        let path = entry.path.clone();
        if entry.is_dir {
            self.go_to(path, cx);
        } else {
            cx.open_with_system(&path);
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() - 1;
        let ix = match self.selected {
            None if delta > 0 => 0,
            None => last,
            Some(cur) => cur.saturating_add_signed(delta).min(last),
        };
        self.selected = Some(ix);
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
            "down" if cmd => {
                if let Some(ix) = self.selected {
                    self.open(ix, cx);
                }
            }
            "up" => self.move_selection(-1, cx),
            "down" => self.move_selection(1, cx),
            "enter" => {
                if let Some(ix) = self.selected {
                    self.open(ix, cx);
                }
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
            div()
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
                .child(place.name.clone())
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
                        let selected = this.selected == Some(ix);
                        let (fg, dim) = if selected {
                            (t.on_accent, t.on_accent)
                        } else {
                            (t.text, t.dim)
                        };
                        let icon = entry_icon(entry, t);
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
                                    .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                                        this.selected = Some(ix);
                                        if ev.click_count() >= 2 {
                                            this.open(ix, cx);
                                        }
                                        cx.notify();
                                    }))
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

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
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
            bar = bar.child(
                div()
                    .id(("crumb", i))
                    .px_1()
                    .rounded_sm()
                    .whitespace_nowrap()
                    .when(i == last, |d| d.text_color(t.text))
                    .cursor_pointer()
                    .hover(|d| d.bg(t.hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.go_to(path.clone(), cx)))
                    .child(name),
            );
        }

        let count = self.entries.len();
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
            .child(div().flex_none().child(format!("{count} items")))
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
