//! Things looked up in the background after a folder is listed: folder sizes,
//! and the "Get Info" panel.

use std::time::Duration;

use gpui::{AnyElement, FontWeight};

use super::*;
use crate::{
    info::{self, Info},
    sizes,
};

/// The item shown in the info panel, with its details once they are known.
pub(super) struct InfoState {
    path: PathBuf,
    data: Option<Info>,
}

impl Browser {
    /// Called from `render` when a scan was asked for (a folder was listed).
    pub(super) fn start_scans(&mut self, cx: &mut Context<Self>) {
        self.start_sizes(cx);
    }

    /// Find the size of the folders of this listing, one after the other.
    fn start_sizes(&mut self, cx: &mut Context<Self>) {
        self.size_gen += 1;
        if !self.config.folder_sizes {
            return;
        }
        let generation = self.size_gen;
        // A cap keeps a huge folder of folders from keeping the disk busy.
        let todo: Vec<PathBuf> = self
            .all
            .iter()
            .filter(|e| e.is_dir && e.dir_size.is_none())
            .take(80)
            .map(|e| e.path.clone())
            .collect();
        if todo.is_empty() {
            return;
        }
        self.sizing = todo.iter().cloned().collect();
        cx.spawn(async move |this, cx| {
            for path in todo {
                let asked = path.clone();
                let size = cx
                    .background_executor()
                    .spawn(async move { sizes::folder_size(&asked, Duration::from_secs(4)) })
                    .await;
                // Stop as soon as the user went somewhere else.
                if !matches!(
                    this.update(cx, |this, cx| this.size_arrived(generation, path, size, cx)),
                    Ok(true)
                ) {
                    break;
                }
            }
        })
        .detach();
    }

    fn size_arrived(
        &mut self,
        generation: u64,
        path: PathBuf,
        size: Option<u64>,
        cx: &mut Context<Self>,
    ) -> bool {
        if generation != self.size_gen {
            return false;
        }
        self.sizing.remove(&path);
        if let Some(bytes) = size {
            self.dir_sizes.insert(path.clone(), bytes);
            for e in self.all.iter_mut().chain(self.entries.iter_mut()) {
                if e.path == path {
                    e.dir_size = Some(bytes);
                }
            }
            // Folders sorted by size move as their sizes come in.
            if self.sort.key == SortKey::Size {
                let keep = self.selected_paths();
                fs::sort_entries(&mut self.all, self.sort);
                fs::sort_entries(&mut self.entries, self.sort);
                self.selected = (0..self.entries.len())
                    .filter(|&i| keep.contains(&self.entries[i].path))
                    .collect();
            }
        }
        cx.notify();
        true
    }

    /// What the size column says for a row.
    pub(super) fn size_text(&self, e: &fs::Entry) -> SharedString {
        if e.is_dir && e.dir_size.is_none() && self.sizing.contains(&e.path) {
            "…".into()
        } else {
            e.size_label()
        }
    }

    pub(super) fn toggle_info(&mut self, cx: &mut Context<Self>) {
        self.info_open = !self.info_open;
        self.info = None;
        cx.notify();
    }

    /// Keep the info panel on what is selected (nothing selected: this folder).
    pub(super) fn sync_info(&mut self, cx: &mut Context<Self>) {
        if !self.info_open {
            return;
        }
        let target = match self.selected_paths().as_slice() {
            [] => Some(self.cwd.clone()),
            [one] => Some(one.clone()),
            _ => None,
        };
        let Some(target) = target else {
            self.info = None;
            return;
        };
        if self.info.as_ref().is_some_and(|i| i.path == target) {
            return;
        }
        self.info = Some(InfoState {
            path: target.clone(),
            data: None,
        });
        cx.spawn(async move |this, cx| {
            let asked = target.clone();
            let found = cx
                .background_executor()
                .spawn(async move { info::gather(&asked) })
                .await;
            this.update(cx, |this, cx| {
                // Only if it is still what the panel is about.
                if let Some(state) = this.info.as_mut()
                    && state.path == target
                {
                    state.data = found;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Make the item of the panel executable, or not.
    fn toggle_exec(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match info::toggle_executable(&path) {
            Ok(mode) => {
                // Look again: the permissions shown must be the new ones.
                self.info = None;
                self.say_in_status(format!("Permissions are now {}", info::rwx(mode)), cx);
            }
            Err(e) => self.say_in_status(format!("Could not change permissions: {e}"), cx),
        }
    }

    pub(super) fn info_panel(&self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        let line = |label: &str, value: String| {
            div()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(t.dim)
                        .child(label.to_string()),
                )
                .child(div().min_w_0().child(value))
        };
        let panel = div()
            .w(px(270.0))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(t.border)
            .bg(t.bg);

        let body = match self.info.as_ref().and_then(|i| i.data.as_ref()) {
            Some(info) => {
                let when = |time: Option<std::time::SystemTime>| {
                    time.map_or_else(|| "--".to_string(), |x| self.dates.format(x))
                };
                let size = info
                    .size
                    .map(|b| fs::format_size(b))
                    .unwrap_or_else(|| "--".into());
                let path = info.path.clone();
                let exec_path = info.path.clone();
                let is_file = !info.is_dir;
                let executable = info.mode & 0o111 != 0;
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(info.name.clone()),
                            )
                            .child(
                                div()
                                    .text_color(t.dim)
                                    .child(format!("{} · {size}", info.kind)),
                            ),
                    )
                    .child(
                        div()
                            .id("info-path")
                            .cursor_pointer()
                            .rounded_sm()
                            .hover(|d| d.bg(t.hover))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    path.to_string_lossy().into_owned(),
                                ));
                            }))
                            .child(line(
                                "Where (click to copy)",
                                info.path
                                    .parent()
                                    .map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
                            )),
                    )
                    .child(line("Created", when(info.created)))
                    .child(line("Modified", when(info.modified)))
                    .child(line("Last opened", when(info.accessed)))
                    .child(line(
                        "Permissions",
                        format!("{}  ({:o})", info::rwx(info.mode), info.mode),
                    ))
                    .child(line("Owner", info.owner.replace(':', " : ")))
                    .when_some(info.items, |d, n| d.child(line("Items", n.to_string())))
                    .when_some(info.link_target.clone(), |d, target| {
                        d.child(line("Link to", target.to_string_lossy().into_owned()))
                    })
                    .when(is_file, |d| {
                        d.child(
                            div().flex().child(
                                div()
                                    .id("toggle-exec")
                                    .px_3()
                                    .h(px(26.0))
                                    .flex()
                                    .items_center()
                                    .rounded_md()
                                    .bg(t.hover)
                                    .cursor_pointer()
                                    .hover(|d| d.opacity(0.8))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_exec(exec_path.clone(), cx)
                                    }))
                                    .child(if executable {
                                        "Remove execute permission"
                                    } else {
                                        "Make executable"
                                    }),
                            ),
                        )
                    })
            }
            None if self.info.is_some() => div().p_4().text_color(t.dim).child("Looking…"),
            None => div()
                .p_4()
                .text_color(t.dim)
                .child(format!("{} items selected", self.selected.len())),
        };
        panel
            .child(
                div()
                    .h(px(TOOLBAR_H))
                    .flex_none()
                    .px_4()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(t.border)
                    .text_color(t.dim)
                    .child("Info"),
            )
            .child(body)
            .into_any_element()
    }
}
