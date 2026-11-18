//! The settings page: opened from the gear at the bottom of the sidebar (⌘,).

use gpui::{AnyElement, FontWeight, transparent_black};

use super::*;
use crate::config::{Accent, ClaudeModel, Density, IconStyle, ThemeMode};

impl Browser {
    pub(super) fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            // Looked up once here, not at every repaint.
            self.terminals = apps::installed_terminals();
            self.editors = apps::installed_editors();
            // Debug builds: start scrolled down, to look at the lower sections.
            #[cfg(debug_assertions)]
            if let Some(px_down) = std::env::var("EXPLORUST_SETTINGS_SCROLL")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
            {
                self.settings_scroll
                    .set_offset(gpui::point(px(0.0), px(-px_down)));
            }
        }
        cx.notify();
    }

    pub(super) fn settings_page(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let c = &self.config;

        let appearance = vec![
            self.row(
                t,
                "Theme",
                "Follow macOS, or force one look.",
                self.segmented(
                    t,
                    "theme",
                    ThemeMode::ALL
                        .iter()
                        .map(|m| (*m, m.label().into()))
                        .collect(),
                    c.theme,
                    |c, v| c.theme = v,
                    cx,
                ),
            ),
            self.row(
                t,
                "Accent color",
                "Selection and highlights.",
                self.accent_picker(t, cx),
            ),
            self.row(
                t,
                "File icons",
                "Advanced tells types apart: PDF, sheets, code by language…",
                self.segmented(
                    t,
                    "icon-style",
                    IconStyle::ALL
                        .iter()
                        .map(|m| (*m, m.label().into()))
                        .collect(),
                    c.icon_style,
                    |c, v| c.icon_style = v,
                    cx,
                ),
            ),
            self.row(
                t,
                "Row height",
                "How much fits on screen.",
                self.segmented(
                    t,
                    "density",
                    Density::ALL
                        .iter()
                        .map(|d| (*d, d.label().into()))
                        .collect(),
                    c.density,
                    |c, v| c.density = v,
                    cx,
                ),
            ),
        ];

        let files = vec![
            self.switch_row(
                t,
                "natural",
                "Natural sort order",
                "“file2” comes before “file10”.",
                c.natural_sort,
                |c, v| c.natural_sort = v,
                cx,
            ),
            self.switch_row(
                t,
                "folders-top",
                "Folders on top",
                "Keep folders above files, whatever the sort.",
                c.folders_on_top,
                |c, v| c.folders_on_top = v,
                cx,
            ),
            self.switch_row(
                t,
                "hidden",
                "Show hidden files",
                "Also on ⌘⇧.",
                c.show_hidden,
                |c, v| c.show_hidden = v,
                cx,
            ),
            self.switch_row(
                t,
                "folder-sizes",
                "Folder sizes",
                "Show the space folders take (found out in the background).",
                c.folder_sizes,
                |c, v| c.folder_sizes = v,
                cx,
            ),
            self.switch_row(
                t,
                "relative-dates",
                "Relative dates",
                "“Today at 12:40” instead of a full date.",
                c.relative_dates,
                |c, v| c.relative_dates = v,
                cx,
            ),
        ];

        let mut terminals: Vec<(Option<PathBuf>, SharedString)> = vec![(None, "System".into())];
        terminals.extend(
            self.terminals
                .iter()
                .map(|a| (Some(a.path.clone()), a.name.clone().into())),
        );
        // A terminal picked with "Choose Application…" is not in the known list.
        if let Some(custom) = c.terminal.as_ref()
            && !terminals.iter().any(|(p, _)| p.as_ref() == Some(custom))
            && let Some(app) = AppInfo::from_path(custom)
        {
            terminals.push((Some(custom.clone()), app.name.into()));
        }

        let mut editors: Vec<(Option<PathBuf>, SharedString)> = vec![(None, "Auto".into())];
        editors.extend(
            self.editors
                .iter()
                .map(|a| (Some(a.path.clone()), a.name.clone().into())),
        );
        if let Some(custom) = c.editor.as_ref()
            && !editors.iter().any(|(p, _)| p.as_ref() == Some(custom))
            && let Some(app) = AppInfo::from_path(custom)
        {
            editors.push((Some(custom.clone()), app.name.into()));
        }

        // Everything for people who code. One switch hides the lot.
        let mut developer = vec![self.switch_row(
            t,
            "dev-tools",
            "Developer tools",
            "Git, projects, servers, Claude. Turn off if you do not code.",
            c.dev_tools,
            |c, v| c.dev_tools = v,
            cx,
        )];
        if c.dev_tools {
            developer.extend([
                self.switch_row(
                    t,
                    "git",
                    "Git",
                    "Badges on changed files, branch in the status bar, menu actions.",
                    c.git,
                    |c, v| c.git = v,
                    cx,
                ),
                self.switch_row(
                    t,
                    "git-dim",
                    "Dim ignored files",
                    "Files that .gitignore leaves out look fainter.",
                    c.git_dim_ignored,
                    |c, v| c.git_dim_ignored = v,
                    cx,
                ),
                self.switch_row(
                    t,
                    "projects",
                    "Project folders",
                    "A tag on project folders, and their commands in the menu.",
                    c.project_detect,
                    |c, v| c.project_detect = v,
                    cx,
                ),
                self.switch_row(
                    t,
                    "ssh",
                    "Servers (SSH)",
                    "Browse servers like folders (⌘K), with your ssh keys.",
                    c.ssh,
                    |c, v| c.ssh = v,
                    cx,
                ),
                self.switch_row(
                    t,
                    "claude",
                    "Claude",
                    "Ask about a file or folder. It is sent to Anthropic only when you ask.",
                    c.claude,
                    |c, v| c.claude = v,
                    cx,
                ),
            ]);
            if c.claude {
                developer.push(
                    self.row_stacked(
                        t,
                        "Claude model",
                        "Opus is the most capable, Haiku the fastest and cheapest.",
                        self.segmented(
                            t,
                            "claude-model",
                            ClaudeModel::ALL
                                .iter()
                                .map(|m| (*m, m.label().into()))
                                .collect(),
                            c.claude_model,
                            |c, v| c.claude_model = v,
                            cx,
                        ),
                    ),
                );
                developer.push(self.row_stacked(
                    t,
                    "API key",
                    if self.claude_ready {
                        "Saved in your Keychain."
                    } else {
                        "Copy your sk-ant key, then save it. It goes to the Keychain."
                    },
                    self.key_buttons(t, cx),
                ));
            }
        }

        let behavior = vec![
            self.switch_row(
                t,
                "single-click",
                "Open folders with one click",
                "No double click needed.",
                c.single_click,
                |c, v| c.single_click = v,
                cx,
            ),
            self.switch_row(
                t,
                "live",
                "Live refresh",
                "The list follows changes made on disk.",
                c.live_refresh,
                |c, v| c.live_refresh = v,
                cx,
            ),
            self.switch_row(
                t,
                "reopen",
                "Reopen last folder",
                "Start where you left off.",
                c.reopen_last,
                |c, v| c.reopen_last = v,
                cx,
            ),
            self.row(
                t,
                "Warn before trashing big folders",
                "Ask first above this many items.",
                self.segmented(
                    t,
                    "trash-warning",
                    vec![
                        (None, "Off".into()),
                        (Some(50), "50".into()),
                        (Some(100), "100".into()),
                        (Some(500), "500".into()),
                    ],
                    c.trash_warning,
                    |c, v| c.trash_warning = v,
                    cx,
                ),
            ),
            self.row_stacked(
                t,
                "Editor",
                "Used by “Open in …”. Auto uses the first one found.",
                self.segmented(
                    t,
                    "editor",
                    editors,
                    c.editor.clone(),
                    |c, v| c.editor = v,
                    cx,
                ),
            ),
            self.row_stacked(
                t,
                "Terminal",
                "Used by “Open in Terminal”.",
                self.segmented(
                    t,
                    "terminal",
                    terminals,
                    c.terminal.clone(),
                    |c, v| c.terminal = v,
                    cx,
                ),
            ),
        ];

        let reset = div().flex().child(
            div()
                .id("reset-settings")
                .px_3()
                .h(px(26.0))
                .flex()
                .items_center()
                .rounded_md()
                .text_color(t.dim)
                .cursor_pointer()
                .hover(|d| d.bg(t.hover).text_color(t.text))
                .on_click(cx.listener(|this, _, _, cx| {
                    // Everything back to normal, except where the user was.
                    this.update_config(
                        |c| {
                            *c = Config {
                                last_folder: c.last_folder.take(),
                                ..Config::default()
                            }
                        },
                        cx,
                    )
                }))
                .child("Reset to Defaults"),
        );

        let header = div()
            .h(px(TOOLBAR_H))
            .flex_none()
            .px_4()
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(t.border)
            .window_control_area(WindowControlArea::Drag)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Settings"))
            .child(
                div()
                    .id("settings-done")
                    .px_3()
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(t.accent)
                    .text_color(t.on_accent)
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_settings(cx)))
                    .child("Done"),
            );

        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("settings-scroll")
                    .track_scroll(&self.settings_scroll)
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(640.0))
                            .mx_auto()
                            .p_6()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .child(section(t, "Appearance", appearance))
                            .child(section(t, "Files", files))
                            .child(section(t, "Behavior", behavior))
                            .child(section(t, "Developer", developer))
                            .child(section(t, "Updates", self.update_rows(t, cx)))
                            .child(reset)
                            .child(
                                div()
                                    .pt_2()
                                    .flex()
                                    .justify_center()
                                    .text_size(px(11.5))
                                    .text_color(t.dim)
                                    .child(format!(
                                        "ExploRust v{} · made with Rust and GPUI",
                                        crate::updater::VERSION
                                    )),
                            ),
                    ),
            )
    }

    /// A line of the page: title and hint on the left, the control on the right.
    pub(super) fn row(
        &self,
        t: Theme,
        title: &str,
        hint: &str,
        control: impl IntoElement,
    ) -> AnyElement {
        div()
            .px_4()
            .py_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .min_w_0()
                    .child(title.to_string())
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.dim)
                            .child(hint.to_string()),
                    ),
            )
            .child(div().flex_none().child(control))
            .into_any_element()
    }

    /// Like `row`, with the control under the text: for controls too wide to sit beside it.
    pub(super) fn row_stacked(
        &self,
        t: Theme,
        title: &str,
        hint: &str,
        control: impl IntoElement,
    ) -> AnyElement {
        div()
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(title.to_string())
                    .child(
                        div()
                            .text_size(px(11.5))
                            .text_color(t.dim)
                            .child(hint.to_string()),
                    ),
            )
            // `flex`: the control keeps its own width instead of stretching.
            .child(div().flex().child(control))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn switch_row(
        &self,
        t: Theme,
        id: &'static str,
        title: &str,
        hint: &str,
        on: bool,
        set: fn(&mut Config, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let switch = div()
            .id(id)
            .w(px(36.0))
            .h(px(20.0))
            .px(px(2.0))
            .flex()
            .items_center()
            .rounded_full()
            .cursor_pointer()
            .bg(if on { t.accent } else { t.border })
            .when(on, |d| d.justify_end())
            .on_click(cx.listener(move |this, _, _, cx| this.update_config(|c| set(c, !on), cx)))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(gpui::white())
                    .shadow_sm(),
            );
        self.row(t, title, hint, switch)
    }

    /// Pick one of a few values.
    fn segmented<T: Clone + PartialEq + 'static>(
        &self,
        t: Theme,
        id: &'static str,
        options: Vec<(T, SharedString)>,
        current: T,
        set: fn(&mut Config, T),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .p(px(2.0))
            .gap_0p5()
            .rounded_md()
            .bg(t.hover)
            .children(options.into_iter().enumerate().map(|(i, (value, label))| {
                let active = value == current;
                div()
                    .id((id, i))
                    .px_3()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded(px(5.0))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .when(active, |d| d.bg(t.bg).shadow_sm())
                    .when(!active, |d| {
                        d.text_color(t.dim).hover(|d| d.text_color(t.text))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let value = value.clone();
                        this.update_config(|c| set(c, value), cx)
                    }))
                    .child(label)
            }))
    }

    fn accent_picker(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.config.accent;
        let swatches: Vec<_> = Accent::ALL
            .iter()
            .filter_map(|&accent| {
                let rgb = accent.rgb()?;
                let active = accent == current;
                Some(
                    div()
                        .id(("accent", accent as usize))
                        .p(px(2.0))
                        .rounded_full()
                        .border_2()
                        .border_color(if active {
                            t.text.into()
                        } else {
                            transparent_black()
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.update_config(|c| c.accent = accent, cx)
                        }))
                        .child(div().size(px(14.0)).rounded_full().bg(gpui::rgb(rgb))),
                )
            })
            .collect();

        div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.segmented(
                t,
                "accent-system",
                vec![(Accent::System, "System".into())],
                current,
                |c, v| c.accent = v,
                cx,
            ))
            .child(div().flex().items_center().gap_0p5().children(swatches))
    }
}

/// A titled group of rows in a bordered card.
fn section(t: Theme, title: &str, rows: Vec<AnyElement>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(
            div()
                .px_1()
                .text_size(px(11.0))
                .text_color(t.dim)
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_uppercase()),
        )
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .rounded_lg()
                .border_1()
                .border_color(t.border)
                .overflow_hidden()
                .children(rows.into_iter().enumerate().map(|(i, row)| {
                    div()
                        .w_full()
                        .when(i > 0, |d| d.border_t_1().border_color(t.border))
                        .child(row)
                })),
        )
}
