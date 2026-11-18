//! Checking for a new version, asking the user, and installing it.

use gpui::{AnyElement, Div, PromptLevel, Stateful};

use super::*;
use crate::updater::{self, Release};

pub(super) enum UpdateState {
    /// Nothing to say yet.
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// Downloading and installing this version.
    Installing(String),
    Failed(String),
}

impl Browser {
    /// Ask GitHub for the latest release, in the background. `manual`: the user
    /// asked, so say something even when there is nothing new or it went wrong.
    pub(super) fn check_for_updates(
        &mut self,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.update,
            UpdateState::Checking | UpdateState::Installing(_)
        ) {
            return;
        }
        self.update = UpdateState::Checking;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { updater::check() })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.finish_check(result, manual, window, cx)
            })
            .ok();
        })
        .detach();
    }

    fn finish_check(
        &mut self,
        result: Result<Option<Release>, String>,
        manual: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some(release)) => {
                let skipped = self.config.skipped_version.as_deref() == Some(&release.version);
                self.update = UpdateState::Available(release.clone());
                if manual || !skipped {
                    self.prompt_update(release, window, cx);
                }
            }
            Ok(None) => self.update = UpdateState::UpToDate,
            // A check nobody asked for fails quietly (no network, GitHub down...).
            Err(e) => {
                self.update = if manual {
                    UpdateState::Failed(e)
                } else {
                    UpdateState::Idle
                };
            }
        }
        cx.notify();
    }

    /// The "new version" pop-up.
    pub(super) fn prompt_update(
        &mut self,
        release: Release,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = format!("ExploRust {} is available", release.version);
        let mut detail = format!("You have version {}.", updater::VERSION);
        let notes: String = release.notes.chars().take(400).collect();
        if !notes.is_empty() {
            detail.push_str(&format!("\n\n{notes}"));
        }
        let answer = window.prompt(
            PromptLevel::Info,
            &message,
            Some(&detail),
            &["Install and Restart", "Later", "Skip This Version"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| match answer.await {
            Ok(0) => {
                this.update_in(cx, |this, window, cx| {
                    this.install_update(release, window, cx)
                })
                .ok();
            }
            Ok(2) => {
                this.update(cx, |this, cx| {
                    this.update_config(|c| c.skipped_version = Some(release.version.clone()), cx)
                })
                .ok();
            }
            _ => {}
        })
        .detach();
    }

    /// Download and install in the background, then restart into the new version.
    pub(super) fn install_update(
        &mut self,
        release: Release,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update = UpdateState::Installing(release.version.clone());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let job = release.clone();
            let result = cx
                .background_executor()
                .spawn(async move { updater::install(&job) })
                .await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(app) => {
                    updater::relaunch(&app);
                    cx.quit();
                }
                Err(e) => {
                    this.update = UpdateState::Failed(e.clone());
                    this.offer_manual_download(release, &e, window, cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The install failed: say why, and offer the download page instead.
    fn offer_manual_download(
        &mut self,
        release: Release,
        error: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let answer = window.prompt(
            PromptLevel::Warning,
            "The update could not be installed",
            Some(&format!(
                "{error}\n\nYou can download version {} yourself.",
                release.version
            )),
            &["Open Download Page", "Close"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(0) = answer.await {
                this.update_in(cx, |_, _, cx| cx.open_url(&release.page_url))
                    .ok();
            }
        })
        .detach();
    }

    /// The "Updates" lines of the settings page.
    pub(super) fn update_rows(&self, t: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let hint = match &self.update {
            UpdateState::Idle => "Looks for new versions on GitHub.".to_string(),
            UpdateState::Checking => "Checking…".to_string(),
            UpdateState::UpToDate => "You have the latest version.".to_string(),
            UpdateState::Available(r) => format!("Version {} is available.", r.version),
            UpdateState::Installing(v) => format!("Installing version {v}…"),
            UpdateState::Failed(e) => e.clone(),
        };
        let button = match &self.update {
            UpdateState::Checking | UpdateState::Installing(_) => {
                button(t, "update-busy", "Please wait", false).into_any_element()
            }
            UpdateState::Available(r) => {
                let release = r.clone();
                button(t, "update-install", "Install", true)
                    .bg(t.accent)
                    .text_color(t.on_accent)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.install_update(release.clone(), window, cx)
                    }))
                    .into_any_element()
            }
            _ => button(t, "update-check", "Check for Updates", true)
                .on_click(
                    cx.listener(|this, _, window, cx| this.check_for_updates(true, window, cx)),
                )
                .into_any_element(),
        };

        vec![
            self.switch_row(
                t,
                "auto-update",
                "Check automatically",
                "Look for a new version when ExploRust starts.",
                self.config.check_updates,
                |c, v| c.check_updates = v,
                cx,
            ),
            self.row(t, &format!("ExploRust {}", updater::VERSION), &hint, button),
        ]
    }
}

fn button(t: Theme, id: &'static str, label: &str, enabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .h(px(26.0))
        .flex()
        .items_center()
        .rounded_md()
        .whitespace_nowrap()
        .bg(t.hover)
        .when(enabled, |d| d.cursor_pointer().hover(|d| d.opacity(0.8)))
        .when(!enabled, |d| d.text_color(t.dim))
        .child(label.to_string())
}
