//! Project folders: a small tag in the list, and their commands in the menu.

use gpui::AnyElement;

use super::*;
use crate::project::{self, Kind, Task};

impl Browser {
    pub(super) fn project_enabled(&self) -> bool {
        self.config.dev_tools && self.config.project_detect
    }

    /// Look at the folders of the listing for project files, in the background.
    pub(super) fn scan_projects(&mut self, cx: &mut Context<Self>) {
        self.project_scan_id += 1;
        if !self.project_enabled() {
            self.projects.clear();
            return;
        }
        let id = self.project_scan_id;
        let dirs: Vec<PathBuf> = self
            .all
            .iter()
            .filter(|e| e.is_dir)
            .take(300)
            .map(|e| e.path.clone())
            .collect();
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    dirs.into_iter()
                        .filter_map(|d| project::detect(&d).map(|k| (d, k)))
                        .collect::<std::collections::HashMap<_, _>>()
                })
                .await;
            this.update(cx, |this, cx| {
                if this.project_scan_id == id {
                    this.projects = found;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The small "Rust" tag after a folder's name.
    pub(super) fn project_tag(
        &self,
        e: &fs::Entry,
        selected: bool,
        t: Theme,
    ) -> Option<AnyElement> {
        let kind = *self.projects.get(&e.path)?;
        Some(
            div()
                .flex_none()
                .px_1p5()
                .rounded_sm()
                .text_size(px(10.0))
                .bg(if selected {
                    Hsla::from(t.on_accent).opacity(0.25)
                } else {
                    Hsla::from(t.hover)
                })
                .text_color(if selected { t.on_accent } else { t.dim })
                .child(kind.label())
                .into_any_element(),
        )
    }

    /// The commands of the project the menu is about (one folder, or the one being browsed).
    pub(super) fn project_menu(&mut self, on_item: bool) -> Vec<MenuEntry> {
        self.menu_tasks = None;
        if !self.project_enabled() {
            return Vec::new();
        }
        let dir = if on_item {
            match self.selected_paths().as_slice() {
                [one] if one.is_dir() => one.clone(),
                _ => return Vec::new(),
            }
        } else {
            self.cwd.clone()
        };
        let Some(kind) = project::detect(&dir) else {
            return Vec::new();
        };
        let tasks = project::tasks(&dir, kind);
        if tasks.is_empty() {
            return Vec::new();
        }
        let mut entries = vec![MenuEntry::Separator];
        for (i, task) in tasks.iter().enumerate() {
            entries.push(MenuEntry::Item(
                format!("▶ {}", task.label).into(),
                MenuAction::RunTask(i as u8),
            ));
        }
        self.menu_tasks = Some((dir, tasks));
        entries
    }

    /// Run a project command in the user's terminal.
    pub(super) fn run_task(&mut self, index: u8, cx: &mut Context<Self>) {
        let Some((dir, tasks)) = self.menu_tasks.clone() else {
            return;
        };
        let Some(Task { command, .. }) = tasks.get(index as usize) else {
            return;
        };
        match project::write_script(&dir, command) {
            Ok(script) => apps::open_with(&[script], &self.terminal().path),
            Err(e) => self.say_in_status(format!("Could not start it: {e}"), cx),
        }
    }

    #[allow(dead_code)]
    fn _kind(_: Kind) {}
}
