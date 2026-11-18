//! File operations that run in the background (with a progress bar), the undo
//! history, and the short messages shown in the status bar.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{AnyElement, relative};

use super::*;
use crate::{
    jobs::{self, Progress},
    undo::UndoOp,
};

/// How long a message stays in the status bar.
const NOTICE_FOR: Duration = Duration::from_secs(4);

pub(super) struct RunningJob {
    id: u64,
    label: String,
    progress: Arc<Progress>,
}

impl Browser {
    /// Say something short in the status bar.
    pub(super) fn say_in_status(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some((text.into(), Instant::now()));
        cx.notify();
    }

    /// Called every second: forget a message once it has been there long enough.
    pub(super) fn expire_notice(&mut self, cx: &mut Context<Self>) {
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > NOTICE_FOR)
        {
            self.notice = None;
            cx.notify();
        }
    }

    /// The message to show now, if any.
    pub(super) fn current_notice(&self) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(_, at)| at.elapsed() <= NOTICE_FOR)
            .map(|(text, _)| text.as_str())
    }

    /// Move or copy `items` into `dest` in the background. The window keeps
    /// working, a bar shows the progress, and the result can be undone.
    pub(super) fn transfer_items(
        &mut self,
        items: Vec<PathBuf>,
        dest: PathBuf,
        copy: bool,
        cx: &mut Context<Self>,
    ) {
        if items.is_empty() {
            return;
        }
        let what = match items.as_slice() {
            [one] => format!(
                "“{}”",
                one.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            ),
            many => format!("{} items", many.len()),
        };
        let verb = if copy { "Copying" } else { "Moving" };
        let progress = Arc::new(Progress::default());
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.push(RunningJob {
            id,
            label: format!("{verb} {what}"),
            progress: progress.clone(),
        });
        cx.notify();

        // Keep the bar moving while the work happens elsewhere.
        let ticking = progress.clone();
        cx.spawn(async move |this, cx| {
            while !ticking.is_finished() {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let work = progress.clone();
            let from = items.clone();
            let target = dest.clone();
            let results = cx
                .background_executor()
                .spawn(async move {
                    if copy {
                        work.set_total(jobs::tree_size(&from));
                    }
                    // A failed item (permissions, ...) must not stop the others.
                    from.iter()
                        .map(|p| (p.clone(), fs::transfer_with(p, &target, copy, &work)))
                        .collect::<Vec<_>>()
                })
                .await;
            progress.finish();
            this.update(cx, |this, cx| {
                this.finish_transfer(id, results, copy, dest, cx)
            })
            .ok();
        })
        .detach();
    }

    fn finish_transfer(
        &mut self,
        id: u64,
        results: Vec<(PathBuf, std::io::Result<PathBuf>)>,
        copy: bool,
        dest: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.jobs.retain(|j| j.id != id);
        let failed = results.iter().filter(|(_, r)| r.is_err()).count();
        let cancelled = results.iter().any(|(_, r)| {
            r.as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::Interrupted)
        });
        let done: Vec<(PathBuf, PathBuf)> = results
            .into_iter()
            .filter_map(|(from, r)| r.ok().map(|to| (from, to)))
            .collect();

        if !done.is_empty() {
            let label = if copy { "Copy" } else { "Move" };
            if copy {
                self.undo.record(
                    label,
                    UndoOp::Trash(done.iter().map(|(_, to)| to.clone()).collect()),
                );
            } else {
                // A move onto itself changes nothing: nothing to undo.
                let moved: Vec<_> = done
                    .iter()
                    .filter(|(from, to)| from != to)
                    .map(|(from, to)| (to.clone(), from.clone()))
                    .collect();
                if !moved.is_empty() {
                    self.undo.record(label, UndoOp::MoveBack(moved));
                }
            }
        }
        self.reload();
        // Like Finder, what just landed in this folder is selected.
        if dest == self.cwd {
            let created: Vec<PathBuf> = done.iter().map(|(_, to)| to.clone()).collect();
            self.select_paths(&created);
        }
        if cancelled {
            self.say_in_status("Cancelled", cx);
        } else if failed > 0 {
            self.say_in_status(
                format!(
                    "{failed} item(s) could not be {}",
                    if copy { "copied" } else { "moved" }
                ),
                cx,
            );
        }
        cx.notify();
    }

    /// Run `work` in the background as a job with a progress bar. It makes one new
    /// item (a zip, an extracted folder); that item is selected and the creation can be undone.
    fn run_job(
        &mut self,
        label: String,
        undo_label: &'static str,
        work: impl FnOnce(&Progress) -> Result<PathBuf, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let progress = Arc::new(Progress::default());
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.push(RunningJob {
            id,
            label,
            progress: progress.clone(),
        });
        cx.notify();

        let ticking = progress.clone();
        cx.spawn(async move |this, cx| {
            while !ticking.is_finished() {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let p = progress.clone();
            let result = cx
                .background_executor()
                .spawn(async move { work(&p) })
                .await;
            progress.finish();
            this.update(cx, |this, cx| {
                this.jobs.retain(|j| j.id != id);
                match result {
                    Ok(created) => {
                        this.undo
                            .record(undo_label, UndoOp::Trash(vec![created.clone()]));
                        this.reload();
                        this.select_paths(&[created]);
                    }
                    Err(e) if e == "cancelled" => this.say_in_status("Cancelled", cx),
                    Err(e) => this.say_in_status(format!("Failed: {e}"), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Run `work` in the background with a progress bar, then call `done` with its result.
    pub(super) fn background_job<R: Send + 'static>(
        &mut self,
        label: String,
        work: impl FnOnce(&Progress) -> Result<R, String> + Send + 'static,
        done: impl FnOnce(&mut Browser, Result<R, String>, &mut Context<Browser>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let progress = Arc::new(Progress::default());
        let id = self.next_job;
        self.next_job += 1;
        self.jobs.push(RunningJob {
            id,
            label,
            progress: progress.clone(),
        });
        cx.notify();

        let ticking = progress.clone();
        cx.spawn(async move |this, cx| {
            while !ticking.is_finished() {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            let p = progress.clone();
            let result = cx
                .background_executor()
                .spawn(async move { work(&p) })
                .await;
            progress.finish();
            this.update(cx, |this, cx| {
                this.jobs.retain(|j| j.id != id);
                done(this, result, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Zip the selection.
    pub(super) fn compress_selection(&mut self, cx: &mut Context<Self>) {
        let items = self.selected_paths();
        if items.is_empty() {
            return;
        }
        let dir = self.cwd.clone();
        let label = match items.as_slice() {
            [_] => "Compressing…".to_string(),
            many => format!("Compressing {} items…", many.len()),
        };
        self.run_job(
            label,
            "Compress",
            move |p| crate::archive::compress(&items, &dir, p),
            cx,
        );
    }

    /// Unpack the selected archive next to itself.
    pub(super) fn extract_selection(&mut self, cx: &mut Context<Self>) {
        if let [archive] = self.selected_paths().as_slice()
            && crate::archive::is_archive(archive)
        {
            let archive = archive.clone();
            self.run_job(
                "Extracting…".into(),
                "Extract",
                move |p| crate::archive::extract(&archive, p),
                cx,
            );
        }
    }

    /// ⌘Z.
    pub(super) fn undo_last(&mut self, cx: &mut Context<Self>) {
        let outcome = self.undo.undo();
        self.after_undo(outcome, "Undid", "Nothing to undo", cx);
    }

    /// ⌘⇧Z.
    pub(super) fn redo_last(&mut self, cx: &mut Context<Self>) {
        let outcome = self.undo.redo();
        self.after_undo(outcome, "Redid", "Nothing to redo", cx);
    }

    fn after_undo(
        &mut self,
        outcome: Option<Result<String, String>>,
        done: &str,
        nothing: &str,
        cx: &mut Context<Self>,
    ) {
        let text = match outcome {
            Some(Ok(label)) => {
                self.reload();
                format!("{done} {}", label.to_lowercase())
            }
            Some(Err(e)) => format!("Could not do it: {e}"),
            None => nothing.to_string(),
        };
        self.say_in_status(text, cx);
    }

    /// The progress bar of the running jobs, for the status bar.
    pub(super) fn jobs_bar(&self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let job = self.jobs.first()?;
        let more = self.jobs.len() - 1;
        let progress = job.progress.clone();
        let fraction = progress.fraction();
        let label = match fraction {
            Some(f) => format!("{} · {:.0}%", job.label, f * 100.0),
            None => job.label.clone(),
        };
        // Without a known total the bar just stays half full.
        let filled = fraction.unwrap_or(0.5);
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .text_color(t.text)
                .child(label)
                .child(
                    div()
                        .w(px(110.0))
                        .h(px(5.0))
                        .rounded_full()
                        .bg(t.border)
                        .child(
                            div()
                                .h_full()
                                .w(relative(filled))
                                .rounded_full()
                                .bg(t.accent),
                        ),
                )
                .when(more > 0, |d| d.child(format!("+{more}")))
                .child(
                    div()
                        .id("cancel-job")
                        .px_1()
                        .rounded_sm()
                        .cursor_pointer()
                        .hover(|d| d.bg(t.hover))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            progress.cancel();
                            cx.notify();
                        }))
                        .child("✕"),
                )
                .into_any_element(),
        )
    }
}
