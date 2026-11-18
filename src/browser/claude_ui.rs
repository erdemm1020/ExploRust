//! The Claude entries of the menu, the answer box, and the key line of the settings.

use std::path::Path;

use gpui::{AnyElement, FontWeight};

use super::*;
use crate::claude;

/// What the user asked Claude.
#[derive(Clone)]
enum Task {
    Explain(PathBuf),
    SuggestName(PathBuf),
    Summarize(PathBuf),
    Organize(PathBuf),
}

/// The answer box.
pub(super) struct AiPanel {
    title: String,
    body: String,
    busy: bool,
}

impl Browser {
    pub(super) fn claude_enabled(&self) -> bool {
        self.config.dev_tools && self.config.claude && self.claude_ready && self.remote.is_none()
    }

    /// Is a key saved? Asked to the Keychain in the background.
    pub(super) fn check_claude_key(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let ready = cx
                .background_executor()
                .spawn(async { claude::api_key().is_some() })
                .await;
            this.update(cx, |this, cx| {
                this.claude_ready = ready;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The Claude lines of the context menu.
    pub(super) fn claude_menu(&self, on_item: bool) -> Vec<MenuEntry> {
        if !self.claude_enabled() {
            return Vec::new();
        }
        let item = |label: &str, action| MenuEntry::Item(label.to_string().into(), action);
        let mut entries = vec![MenuEntry::Separator];
        if on_item {
            if self.selected.len() != 1 {
                return Vec::new();
            }
            entries.push(item("Claude: Explain This", MenuAction::ClaudeExplain));
            if self.selection_is_files() {
                entries.push(item("Claude: Suggest a Name", MenuAction::ClaudeName));
            }
        } else {
            entries.push(item(
                "Claude: Summarize This Folder",
                MenuAction::ClaudeSummarize,
            ));
            entries.push(item(
                "Claude: Suggest an Organization",
                MenuAction::ClaudeOrganize,
            ));
        }
        entries
    }

    pub(super) fn claude_task(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        let selected = self.selected_paths();
        let task = match (action, selected.as_slice()) {
            (MenuAction::ClaudeExplain, [one]) => Task::Explain(one.clone()),
            (MenuAction::ClaudeName, [one]) => Task::SuggestName(one.clone()),
            (MenuAction::ClaudeSummarize, _) => Task::Summarize(self.cwd.clone()),
            (MenuAction::ClaudeOrganize, _) => Task::Organize(self.cwd.clone()),
            _ => return,
        };
        self.run_claude(task, cx);
    }

    fn run_claude(&mut self, task: Task, cx: &mut Context<Self>) {
        let model = self.config.claude_model;
        let title = match &task {
            Task::Explain(p) | Task::SuggestName(p) => {
                format!(
                    "Claude · {}",
                    p.file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                )
            }
            Task::Summarize(_) => "Claude · this folder".to_string(),
            Task::Organize(_) => "Claude · organization".to_string(),
        };
        self.ai = Some(AiPanel {
            title,
            body: "Asking Claude…".into(),
            busy: true,
        });
        cx.notify();

        let job = task.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let key = claude::api_key().ok_or("no API key saved")?;
                    let prompt = build_prompt(&job)?;
                    claude::ask(&key, model, claude::SYSTEM, &prompt)
                })
                .await;
            this.update(cx, |this, cx| this.claude_answered(task, result, cx))
                .ok();
        })
        .detach();
    }

    fn claude_answered(
        &mut self,
        task: Task,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        match (task, result) {
            // A suggested name goes into the name box, to be accepted or edited.
            (Task::SuggestName(path), Ok(answer)) => {
                let current = path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                match claude::clean_name(&answer, &current) {
                    Some(name) => {
                        self.ai = None;
                        self.select_path(&path);
                        if let Some(&ix) = self.selected.first() {
                            self.renaming = Some(Renaming {
                                ix,
                                field: TextField::for_file_name(&name),
                            });
                        }
                    }
                    None => self.show_ai("Claude gave no usable name."),
                }
            }
            (_, Ok(answer)) => self.show_ai(&answer),
            (_, Err(e)) => self.show_ai(&format!("Could not ask Claude: {e}")),
        }
        cx.notify();
    }

    fn show_ai(&mut self, text: &str) {
        if let Some(panel) = self.ai.as_mut() {
            panel.body = text.to_string();
            panel.busy = false;
        }
    }

    /// Escape closes the box.
    pub(super) fn on_ai_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        if ev.keystroke.key == "escape" {
            self.ai = None;
            cx.notify();
        }
    }

    pub(super) fn ai_dialog(&self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let panel = self.ai.as_ref()?;
        let body = panel.body.clone();
        let busy = panel.busy;
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(Hsla::from(gpui::black()).opacity(0.25))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.ai = None;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(520.0))
                        .max_h(px(480.0))
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .rounded_lg()
                        .bg(t.bg)
                        .border_1()
                        .border_color(t.border)
                        .shadow_lg()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(panel.title.clone()),
                        )
                        .child(
                            div()
                                .id("ai-scroll")
                                .max_h(px(340.0))
                                .overflow_y_scroll()
                                .text_color(if busy { t.dim } else { t.text })
                                .child(body.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .when(!busy, |d| {
                                    d.child(pill_button(t, "ai-copy", "Copy").on_click(
                                        cx.listener(move |_, _, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                body.clone(),
                                            ))
                                        }),
                                    ))
                                })
                                .child(pill_button(t, "ai-close", "Close").on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.ai = None;
                                        cx.notify();
                                    },
                                ))),
                        ),
                )
                .into_any_element(),
        )
    }

    // --- The key, in the settings ---------------------------------------------------

    /// Save the API key found on the clipboard.
    fn save_key_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            return self.say_in_status("The clipboard has no text", cx);
        };
        if !claude::looks_like_key(&text) {
            return self.say_in_status(
                "That does not look like an Anthropic API key (sk-ant-…)",
                cx,
            );
        }
        match claude::save_key(text.trim()) {
            Ok(()) => {
                self.claude_ready = true;
                // The key is in the Keychain now: it does not stay on the clipboard.
                cx.write_to_clipboard(ClipboardItem::new_string(String::new()));
                self.say_in_status("API key saved in the Keychain", cx);
            }
            Err(e) => self.say_in_status(format!("Could not save the key: {e}"), cx),
        }
    }

    fn remove_key(&mut self, cx: &mut Context<Self>) {
        match claude::remove_key() {
            Ok(()) => {
                self.claude_ready = false;
                self.say_in_status("API key removed", cx);
            }
            Err(e) => self.say_in_status(e, cx),
        }
    }

    /// The buttons of the "API key" line.
    pub(super) fn key_buttons(&self, t: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .gap_2()
            .child(
                pill_button(
                    t,
                    "key-paste",
                    if self.claude_ready {
                        "Replace from Clipboard"
                    } else {
                        "Save from Clipboard"
                    },
                )
                .on_click(cx.listener(|this, _, _, cx| this.save_key_from_clipboard(cx))),
            )
            .when(self.claude_ready, |d| {
                d.child(
                    pill_button(t, "key-remove", "Remove")
                        .on_click(cx.listener(|this, _, _, cx| this.remove_key(cx))),
                )
            })
            .into_any_element()
    }
}

/// Reads the files and builds the question (runs in the background).
fn build_prompt(task: &Task) -> Result<String, String> {
    let name_of = |p: &Path| {
        p.file_name().map_or_else(
            || p.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    };
    match task {
        Task::Explain(path) | Task::SuggestName(path) => {
            if claude::looks_secret(path) {
                return Err(
                    "this looks like a secret file: ExploRust does not send it to Claude".into(),
                );
            }
            let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
            if meta.is_dir() {
                return match task {
                    Task::Explain(_) => folder_prompt(path),
                    _ => Err("only files can be renamed this way".into()),
                };
            }
            match task {
                Task::SuggestName(_) => {
                    let head = claude::read_head(path, 6_000);
                    Ok(claude::name_prompt(&name_of(path), head.as_deref()))
                }
                _ => {
                    let head = claude::read_head(path, 20_000);
                    Ok(claude::explain_file_prompt(
                        &name_of(path),
                        meta.len(),
                        head.as_deref(),
                    ))
                }
            }
        }
        Task::Summarize(dir) => folder_prompt(dir),
        Task::Organize(dir) => Ok(claude::organize_prompt(&name_of(dir), &listing(dir)?)),
    }
}

fn folder_prompt(dir: &Path) -> Result<String, String> {
    let readme = ["README.md", "README", "readme.md", "README.txt"]
        .iter()
        .find_map(|n| claude::read_head(&dir.join(n), 3_000));
    let name = dir.file_name().map_or_else(
        || "this folder".into(),
        |n| n.to_string_lossy().into_owned(),
    );
    Ok(claude::explain_folder_prompt(
        &name,
        &listing(dir)?,
        readme.as_deref(),
    ))
}

/// Names (and sizes) of the entries of `dir`, without the `.git` folder and secret-looking files.
fn listing(dir: &Path) -> Result<Vec<(String, bool, u64)>, String> {
    let mut items: Vec<(String, bool, u64)> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.file_name() != ".git" && !claude::looks_secret(&e.path()))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some((
                e.file_name().to_string_lossy().into_owned(),
                meta.is_dir(),
                meta.len(),
            ))
        })
        .collect();
    items.sort_by_key(|(n, d, _)| (!*d, n.to_lowercase()));
    Ok(items)
}

pub(super) fn pill_button(t: Theme, id: &'static str, label: &str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_3()
        .h(px(26.0))
        .flex()
        .items_center()
        .rounded_md()
        .whitespace_nowrap()
        .bg(t.hover)
        .cursor_pointer()
        .hover(|d| d.opacity(0.8))
        .child(label.to_string())
}
