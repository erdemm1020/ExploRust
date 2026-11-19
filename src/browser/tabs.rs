//! Tabs: several folders open in one window.
//!
//! The browser always holds the state of the *active* tab in its own fields
//! (`cwd`, `back`, `forward`…). A tab that is not shown is only parked here as
//! a place plus its history, and is shown again with `arrive`.

use std::path::{Path, PathBuf};

use gpui::{Context, IntoElement, MouseButton, SharedString, div, prelude::*, px};

use super::Browser;
use crate::theme::Theme;

const TAB_BAR_H: f32 = 30.0;
const TAB_MAX_W: f32 = 200.0;

/// A tab that is parked (or, for the active one, a stale copy).
#[derive(Clone, Default)]
pub(super) struct Tab {
    location: PathBuf,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
}

/// What a tab is called: its folder name (or the server for a root).
fn label(location: &Path) -> SharedString {
    if let Some((host, path)) = super::remote::split_location(location) {
        return match Path::new(&path).file_name() {
            Some(name) => name.to_string_lossy().into_owned().into(),
            None => host.into(),
        };
    }
    location
        .file_name()
        .map_or_else(
            || "Macintosh HD".into(),
            |n| n.to_string_lossy().into_owned(),
        )
        .into()
}

impl Browser {
    /// Put the active tab's state back into its slot.
    fn park_active(&mut self) {
        let tab = Tab {
            location: self.location(),
            back: std::mem::take(&mut self.back),
            forward: std::mem::take(&mut self.forward),
        };
        if let Some(slot) = self.tabs.get_mut(self.active_tab) {
            *slot = tab;
        }
    }

    /// Make tab `ix` the active one, whose slot is read into the browser.
    fn show_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        let tab = std::mem::take(&mut self.tabs[ix]);
        self.active_tab = ix;
        self.back = tab.back;
        self.forward = tab.forward;
        self.settings_open = false;
        self.arrive(tab.location, cx);
    }

    /// ⌘T: a new tab, on the same folder.
    pub(super) fn new_tab(&mut self, cx: &mut Context<Self>) {
        self.park_active();
        let here = self.tabs[self.active_tab].location.clone();
        let tab = Tab {
            location: here,
            ..Tab::default()
        };
        // The new tab shows the folder already on screen, with a clean history
        // (`park_active` emptied it).
        self.active_tab += 1;
        self.tabs.insert(self.active_tab, tab);
        self.settings_open = false;
        cx.notify();
    }

    pub(super) fn switch_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix == self.active_tab || ix >= self.tabs.len() {
            return;
        }
        self.park_active();
        self.show_tab(ix, cx);
    }

    /// Next (`delta` = 1) or previous (`-1`) tab, wrapping around.
    pub(super) fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.tabs.len() as isize;
        if n > 1 {
            let ix = (self.active_tab as isize + delta).rem_euclid(n);
            self.switch_tab(ix as usize, cx);
        }
    }

    /// ⌘W: close a tab. The last one stays.
    pub(super) fn close_tab(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 || ix >= self.tabs.len() {
            return;
        }
        if ix == self.active_tab {
            self.tabs.remove(ix);
            let next = ix.min(self.tabs.len() - 1);
            self.show_tab(next, cx);
        } else {
            // The shown tab keeps its state in the browser; its slot is not used.
            self.tabs.remove(ix);
            if ix < self.active_tab {
                self.active_tab -= 1;
            }
            cx.notify();
        }
    }

    /// The row of tabs, only when there are several.
    pub(super) fn tab_bar(&self, t: Theme, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if self.tabs.len() < 2 {
            return None;
        }
        let tabs = self.tabs.iter().enumerate().map(|(ix, tab)| {
            let active = ix == self.active_tab;
            let name = if active {
                label(&self.location())
            } else {
                label(&tab.location)
            };
            div()
                .id(("tab", ix))
                .flex_1()
                .min_w_0()
                .max_w(px(TAB_MAX_W))
                .h_full()
                .px_2()
                .flex()
                .items_center()
                .gap_1()
                .rounded_t_md()
                .cursor_pointer()
                .text_size(px(12.0))
                .when(active, |d| d.bg(t.hover).text_color(t.text))
                .when(!active, |d| {
                    d.text_color(t.dim)
                        .hover(|d| d.bg(t.hover).text_color(t.text))
                })
                .on_click(cx.listener(move |this, _, _, cx| this.switch_tab(ix, cx)))
                // Middle click anywhere on the tab closes it.
                .on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(move |this, _, _, cx| this.close_tab(ix, cx)),
                )
                .child(div().flex_1().min_w_0().truncate().child(name))
                .child(
                    div()
                        .id(("tab-close", ix))
                        .px_1()
                        .rounded_sm()
                        .text_color(t.dim)
                        .hover(|d| d.text_color(t.text).bg(t.border))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.close_tab(ix, cx);
                        }))
                        .child("×"),
                )
        });
        Some(
            div()
                .h(px(TAB_BAR_H))
                .flex_none()
                .px_2()
                .pt_1()
                .flex()
                .items_end()
                .gap_1()
                .border_b_1()
                .border_color(t.border)
                .children(tabs)
                .child(
                    div()
                        .id("tab-new")
                        .size(px(24.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .text_color(t.dim)
                        .cursor_pointer()
                        .hover(|d| d.bg(t.hover).text_color(t.text))
                        .on_click(cx.listener(|this, _, _, cx| this.new_tab(cx)))
                        .child("+"),
                ),
        )
    }
}
