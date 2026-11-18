//! Git in the window: badges in the list, the branch in the status bar, and the
//! few Git entries of the context menu.

use std::time::{Duration, Instant};

use gpui::{AnyElement, PromptLevel};

use super::*;
use crate::git::{self, Change, Repo};

/// Width of the badge column.
pub(super) const GIT_W: f32 = 18.0;

impl Browser {
    pub(super) fn git_enabled(&self) -> bool {
        self.config.dev_tools && self.config.git
    }

    /// Ask git about this folder, in the background.
    pub(super) fn scan_git(&mut self, cx: &mut Context<Self>) {
        self.git_scan_id += 1;
        if !self.git_enabled() || !git::available() {
            self.git = None;
            return;
        }
        let id = self.git_scan_id;
        let dir = self.cwd.clone();
        cx.spawn(async move |this, cx| {
            let started = Instant::now();
            let repo = cx
                .background_executor()
                .spawn(async move { git::scan(&dir) })
                .await;
            let slow = started.elapsed() > Duration::from_millis(400);
            this.update(cx, |this, cx| {
                if this.git_scan_id == id {
                    // Only repaint when something changed.
                    if this.git != repo {
                        this.git = repo;
                        cx.notify();
                    }
                    this.git_slow = slow;
                }
            })
            .ok();
        })
        .detach();
    }

    /// Called every second: look again every few seconds (edits do not change the
    /// folder's date), unless git is slow on this repository.
    pub(super) fn tick_git(&mut self, cx: &mut Context<Self>) {
        self.git_tick = self.git_tick.wrapping_add(1);
        if self.git_tick % 4 == 0 && self.git.is_some() && !self.git_slow && self.git_enabled() {
            self.scan_git(cx);
        }
    }

    fn entry_path(&self, e: &fs::Entry) -> Option<PathBuf> {
        self.git.as_ref().map(|r| r.dir.join(e.name.as_ref()))
    }

    /// Git ignores this item (and the user asked to see that).
    pub(super) fn is_git_ignored(&self, e: &fs::Entry) -> bool {
        self.config.git_dim_ignored
            && self
                .git
                .as_ref()
                .zip(self.entry_path(e))
                .is_some_and(|(repo, p)| repo.is_ignored(&p))
    }

    /// The little letter at the right of a name.
    pub(super) fn git_cell(&self, e: &fs::Entry, selected: bool, t: Theme) -> AnyElement {
        let letter = self
            .git
            .as_ref()
            .zip(self.entry_path(e))
            .and_then(|(repo, p)| repo.badge(&p, e.is_dir));
        let color = match letter {
            _ if selected => t.on_accent,
            Some("M") => gpui::rgb(0xd9951a),
            Some("A") | Some("?") => gpui::rgb(0x2fa84f),
            Some("D") | Some("!") => gpui::rgb(0xe5484d),
            Some("R") => gpui::rgb(0x3b82f6),
            // "Something inside changed".
            Some(_) => gpui::rgb(0xd9951a),
            None => t.dim,
        };
        div()
            .w(px(GIT_W))
            .flex_none()
            .flex()
            .justify_center()
            .text_size(px(if letter == Some("•") { 16.0 } else { 11.0 }))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(color)
            .children(letter)
            .into_any_element()
    }

    /// "⎇ main ↑2 ↓1 · 3 changes" for the status bar.
    pub(super) fn git_status(&self, t: Theme) -> Option<AnyElement> {
        let repo = self.git.as_ref()?;
        let mut text = format!("⎇ {}", repo.branch);
        if repo.ahead > 0 {
            text.push_str(&format!(" ↑{}", repo.ahead));
        }
        if repo.behind > 0 {
            text.push_str(&format!(" ↓{}", repo.behind));
        }
        match repo.change_count() {
            0 => {}
            1 => text.push_str(" · 1 change"),
            n => text.push_str(&format!(" · {n} changes")),
        }
        Some(
            div()
                .flex_none()
                .text_color(t.text)
                .child(text)
                .into_any_element(),
        )
    }

    /// The Git lines of the context menu for the selection (or this folder).
    pub(super) fn git_menu(&self, on_item: bool) -> Vec<MenuEntry> {
        let Some(repo) = self.git.as_ref().filter(|_| self.git_enabled()) else {
            return Vec::new();
        };
        let item = |label: &str, action| MenuEntry::Item(label.to_string().into(), action);
        let mut entries = vec![MenuEntry::Separator];
        let changed = self
            .selected_paths()
            .iter()
            .any(|p| repo.changes.keys().any(|c| c == p || c.starts_with(p)));
        if on_item && changed {
            entries.push(item("Git: Stage", MenuAction::GitStage));
            entries.push(item("Git: Unstage", MenuAction::GitUnstage));
            entries.push(item("Git: Discard Changes…", MenuAction::GitDiscard));
        }
        entries.push(item(
            if on_item {
                "Git: Open on the Web"
            } else {
                "Git: Open Repository on the Web"
            },
            MenuAction::GitWeb,
        ));
        entries
    }

    /// What the Git actions apply to: the selection, or this folder.
    fn git_targets(&self) -> Vec<PathBuf> {
        let selected = self.selected_paths();
        if selected.is_empty() {
            vec![self.cwd.clone()]
        } else {
            selected
        }
    }

    pub(super) fn git_stage(&mut self, stage: bool, cx: &mut Context<Self>) {
        let Some(root) = self.git.as_ref().map(|r| r.root.clone()) else {
            return;
        };
        let targets = self.git_targets();
        let result = if stage {
            git::stage(&root, &targets)
        } else {
            git::unstage(&root, &targets)
        };
        match result {
            Ok(()) => self.say_in_status(if stage { "Staged" } else { "Unstaged" }, cx),
            Err(e) => self.say_in_status(format!("Git: {e}"), cx),
        }
        self.scan_git(cx);
    }

    /// Throwing changes away cannot be undone: ask first.
    pub(super) fn git_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repo) = self.git.clone() else { return };
        // Only tracked files with changes come back; new files are left alone.
        let targets: Vec<PathBuf> = self
            .git_targets()
            .into_iter()
            .filter(|p| {
                repo.changes
                    .iter()
                    .any(|(c, kind)| (c == p || c.starts_with(p)) && *kind != Change::Untracked)
            })
            .collect();
        if targets.is_empty() {
            return self.say_in_status("Nothing to discard", cx);
        }
        let detail = match targets.as_slice() {
            [one] => format!(
                "The changes in “{}” will be lost for good.",
                one.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            ),
            many => format!(
                "The changes in these {} items will be lost for good.",
                many.len()
            ),
        };
        // "Cancel" first: it is the button Return would press.
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard the changes?",
            Some(&detail),
            &["Cancel", "Discard Changes"],
            cx,
        );
        let root = repo.root;
        cx.spawn(async move |this, cx| {
            if let Ok(1) = answer.await {
                this.update(cx, |this, cx| {
                    match git::discard(&root, &targets) {
                        Ok(()) => this.say_in_status("Changes discarded", cx),
                        Err(e) => this.say_in_status(format!("Git: {e}"), cx),
                    }
                    this.reload();
                    this.scan_git(cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// Open the repository, or the selected item, on GitHub / GitLab.
    pub(super) fn git_web(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.git.as_ref() else {
            return;
        };
        let target = match self.selected_paths().as_slice() {
            [one] => Some(one.clone()),
            _ => None,
        };
        let target = target.map(|p| repo.dir.join(p.file_name().unwrap_or_default()));
        match git::web_url(&repo.root, &repo.branch, target.as_deref()) {
            Some(url) => cx.open_url(&url),
            None => self.say_in_status("This repository has no web address", cx),
        }
    }

    #[allow(dead_code)]
    fn _unused(&self, _: &Repo) {}
}
