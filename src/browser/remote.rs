//! Browsing a server over SSH: the folder is listed by `ssh`, files are copied with
//! `scp`. A server's folder is shown in the same list as a local one, with the
//! actions that make sense there.

use gpui::{AnyElement, PromptLevel};

use super::*;
use crate::{
    ssh::{self, Conn},
    text_field::KeyResult,
};

/// "ssh://host/path" -> ("host", "/path"). History keeps places as paths, and a place
/// on a server is written this way.
pub(super) fn split_location(p: &Path) -> Option<(String, String)> {
    let text = p.to_str()?.strip_prefix("ssh://")?;
    let slash = text.find('/').unwrap_or(text.len());
    let (host, path) = text.split_at(slash);
    Some((
        host.to_string(),
        if path.is_empty() {
            "/".into()
        } else {
            path.to_string()
        },
    ))
}

impl Browser {
    pub(super) fn ssh_enabled(&self) -> bool {
        self.config.dev_tools && self.config.ssh
    }

    /// Where we are, in the form history keeps.
    pub(super) fn location(&self) -> PathBuf {
        match &self.remote {
            Some(host) => PathBuf::from(format!("ssh://{host}{}", self.cwd.display())),
            None => self.cwd.clone(),
        }
    }

    fn conn(&self) -> Option<Conn> {
        self.remote.as_deref().map(Conn::new)
    }

    /// Go to a folder of a server, adding where we were to the history.
    pub(super) fn go_remote(&mut self, host: String, path: String, cx: &mut Context<Self>) {
        if self.remote.as_deref() == Some(host.as_str()) && self.cwd == Path::new(&path) {
            return;
        }
        self.back.push(self.location());
        self.forward.clear();
        self.set_remote(host, path, cx);
    }

    /// Show `path` of `host`, without touching the history.
    pub(super) fn set_remote(&mut self, host: String, path: String, cx: &mut Context<Self>) {
        self.remote = Some(host);
        self.cwd = PathBuf::from(path);
        self.settings_open = false;
        self.navigated();
        self.all.clear();
        self.entries.clear();
        self.clear_selection();
        self.fetch_remote(cx);
    }

    /// List the current server folder, in the background.
    pub(super) fn fetch_remote(&mut self, cx: &mut Context<Self>) {
        let Some(conn) = self.conn() else { return };
        self.remote_gen += 1;
        let id = self.remote_gen;
        self.remote_loading = true;
        self.remote_error = None;
        let dir = self.cwd.to_string_lossy().into_owned();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let asked = dir.clone();
            let result = cx
                .background_executor()
                .spawn(async move { conn.list(&asked) })
                .await;
            this.update(cx, |this, cx| {
                if this.remote_gen != id {
                    return;
                }
                this.remote_loading = false;
                match result {
                    Ok(items) => {
                        let base = PathBuf::from(&dir);
                        this.all = items
                            .into_iter()
                            .filter(|e| this.config.show_hidden || !e.name.starts_with('.'))
                            .map(|e| {
                                fs::Entry::remote(
                                    base.join(&e.name),
                                    e.name,
                                    e.is_dir,
                                    e.size,
                                    e.date,
                                )
                            })
                            .collect();
                        fs::sort_entries(&mut this.all, this.sort);
                        this.apply_filter();
                        this.finish_pending_rename();
                    }
                    Err(e) => {
                        this.all.clear();
                        this.entries.clear();
                        this.remote_error = Some(e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Connect to "user@host", "host:/path"... (the start folder is the home folder).
    pub(super) fn connect_to(&mut self, input: &str, cx: &mut Context<Self>) {
        let Some((host, path)) = ssh::parse_target(input) else {
            return self.say_in_status("That is not a valid server address", cx);
        };
        self.say_in_status(format!("Connecting to {host}…"), cx);
        cx.spawn(async move |this, cx| {
            let h = host.clone();
            let start = cx
                .background_executor()
                .spawn(async move {
                    let conn = Conn::new(&h);
                    match path {
                        Some(p) if p.starts_with('/') => Ok(p),
                        // "~" and "~/x" are the server's home.
                        Some(p) if p == "~" || p.starts_with("~/") => {
                            conn.home().map(|home| format!("{home}{}", &p[1..]))
                        }
                        Some(p) => Ok(format!("/{p}")),
                        None => conn.home(),
                    }
                })
                .await;
            this.update(cx, |this, cx| match start {
                Ok(path) => this.go_remote(host, path, cx),
                Err(e) => this.say_in_status(format!("Could not connect: {e}"), cx),
            })
            .ok();
        })
        .detach();
    }

    // --- The "Connect to Server" box -----------------------------------------------

    pub(super) fn open_connect(&mut self, cx: &mut Context<Self>) {
        if self.ssh_enabled() {
            self.connect = Some(TextField::empty());
            cx.notify();
        }
    }

    pub(super) fn on_connect_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(field) = self.connect.as_mut() else {
            return;
        };
        let paste = || cx.read_from_clipboard().and_then(|c| c.text());
        match field.handle_key(&ev.keystroke, paste) {
            KeyResult::Enter => {
                let input = field.text().to_string();
                self.connect = None;
                self.connect_to(&input, cx);
            }
            KeyResult::Escape => self.connect = None,
            KeyResult::Edited | KeyResult::Ignored => {}
        }
        cx.notify();
    }

    pub(super) fn connect_dialog(&self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let field = self.connect.as_ref()?;
        let hosts = ssh::load_hosts();
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(Hsla::from(gpui::black()).opacity(0.25))
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.connect = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(420.0))
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .rounded_lg()
                        .bg(t.bg)
                        .border_1()
                        .border_color(t.border)
                        .shadow_lg()
                        // A click inside must not close it.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child("Connect to Server"))
                        .child(
                            div()
                                .h(px(30.0))
                                .px_2()
                                .flex()
                                .items_center()
                                .rounded_md()
                                .border_1()
                                .border_color(t.accent)
                                .child(text_runs(field, true, t)),
                        )
                        .child(div().text_size(px(11.5)).text_color(t.dim).child(
                            "user@host, host:/folder or an alias of ~/.ssh/config. Uses your ssh keys (no password).",
                        ))
                        .when(!hosts.is_empty(), |d| {
                            d.child(div().text_size(px(11.5)).text_color(t.dim).child(format!(
                                "Known: {}",
                                hosts.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
                            )))
                        }),
                )
                .into_any_element(),
        )
    }

    // --- Actions on the files of the server ----------------------------------------

    /// The selected items, as "/remote/path" strings.
    fn remote_selection(&self) -> Vec<(String, bool)> {
        self.selected
            .iter()
            .filter_map(|&i| self.entries.get(i))
            .map(|e| (e.path.to_string_lossy().into_owned(), e.is_dir))
            .collect()
    }

    /// Enter a folder, or bring a file here and open it.
    pub(super) fn open_remote(&mut self, cx: &mut Context<Self>) {
        let (Some(conn), items) = (self.conn(), self.remote_selection()) else {
            return;
        };
        match items.as_slice() {
            [(path, true)] => self.go_remote(conn.host.clone(), path.clone(), cx),
            _ => {
                let files: Vec<String> = items
                    .into_iter()
                    .filter(|(_, d)| !d)
                    .map(|(p, _)| p)
                    .collect();
                if files.is_empty() {
                    return;
                }
                let temp =
                    std::env::temp_dir().join(format!("explorust-remote-{}", std::process::id()));
                let into = temp.clone();
                self.background_job(
                    format!("Opening {} file(s)…", files.len()),
                    move |p| {
                        std::fs::create_dir_all(&into).map_err(|e| e.to_string())?;
                        let mut got = Vec::new();
                        for f in &files {
                            // A fresh folder per file: two files of the same name never clash.
                            let dir = fs::unique_path(&into, std::ffi::OsStr::new("file"));
                            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                            conn.download(f, &dir, p)?;
                            if let Some(name) = Path::new(f).file_name() {
                                got.push(dir.join(name));
                            }
                        }
                        Ok(got)
                    },
                    |this, result, cx| match result {
                        Ok(paths) => paths.iter().for_each(|p| cx.open_with_system(p)),
                        Err(e) => this.say_in_status(format!("Could not open: {e}"), cx),
                    },
                    cx,
                );
            }
        }
    }

    /// Copy the selection to the Downloads folder.
    pub(super) fn download_remote(&mut self, cx: &mut Context<Self>) {
        let (Some(conn), items) = (self.conn(), self.remote_selection()) else {
            return;
        };
        if items.is_empty() {
            return;
        }
        let downloads = std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Downloads"))
            .unwrap_or_else(std::env::temp_dir);
        self.background_job(
            format!("Downloading {} item(s)…", items.len()),
            move |p| {
                let mut landed = Vec::new();
                for (path, _) in &items {
                    // Into a scratch folder first, so nothing in Downloads is overwritten.
                    let scratch =
                        fs::unique_path(&downloads, std::ffi::OsStr::new(".explorust-download"));
                    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
                    let result = conn.download(path, &scratch, p).and_then(|()| {
                        let name = Path::new(path).file_name().ok_or("no name")?;
                        fs::transfer(&scratch.join(name), &downloads, false)
                            .map_err(|e| e.to_string())
                    });
                    let _ = std::fs::remove_dir_all(&scratch);
                    landed.push(result?);
                }
                Ok(landed)
            },
            |this, result, cx| match result {
                Ok(paths) => {
                    this.say_in_status(
                        format!("Downloaded {} item(s) to Downloads", paths.len()),
                        cx,
                    );
                    if let Some(first) = paths.first() {
                        cx.reveal_path(first);
                    }
                }
                Err(e) => this.say_in_status(format!("Download failed: {e}"), cx),
            },
            cx,
        );
    }

    /// Copy local files or folders into a folder of the server.
    pub(super) fn upload_items(
        &mut self,
        local: Vec<PathBuf>,
        into: String,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.conn() else { return };
        if local.is_empty() {
            return;
        }
        self.background_job(
            format!("Uploading {} item(s)…", local.len()),
            move |p| conn.upload(&local, &into, p),
            |this, result, cx| {
                match result {
                    Ok(()) => this.say_in_status("Uploaded", cx),
                    Err(e) => this.say_in_status(format!("Upload failed: {e}"), cx),
                }
                this.fetch_remote(cx);
            },
            cx,
        );
    }

    pub(super) fn remote_new_folder(&mut self, cx: &mut Context<Self>) {
        let Some(conn) = self.conn() else { return };
        // A name no item of this folder has.
        let taken: Vec<String> = self.all.iter().map(|e| e.name.to_string()).collect();
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    "untitled folder".to_string()
                } else {
                    format!("untitled folder {n}")
                }
            })
            .find(|n| !taken.contains(n))
            .unwrap_or_default();
        let path = self.cwd.join(&name);
        let target = path.to_string_lossy().into_owned();
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_executor()
                .spawn(async move { conn.mkdir(&target) })
                .await;
            this.update(cx, |this, cx| match made {
                Ok(()) => {
                    this.fetch_remote(cx);
                    // Once the listing is back, the name box opens on the new folder.
                    this.rename_when_listed = Some(path);
                }
                Err(e) => this.say_in_status(format!("Could not create it: {e}"), cx),
            })
            .ok();
        })
        .detach();
    }

    /// After a listing: select and rename the item we are waiting for.
    pub(super) fn finish_pending_rename(&mut self) {
        if let Some(path) = self.rename_when_listed.take() {
            self.select_path(&path);
            self.begin_rename();
        }
    }

    pub(super) fn remote_rename(
        &mut self,
        from: PathBuf,
        new_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(conn) = self.conn() else { return };
        if new_name.is_empty() || new_name.contains('/') {
            return;
        }
        let to = from.with_file_name(&new_name);
        let (a, b) = (
            from.to_string_lossy().into_owned(),
            to.to_string_lossy().into_owned(),
        );
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move { conn.rename(&a, &b) })
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = done {
                    this.say_in_status(format!("Could not rename: {e}"), cx);
                }
                this.fetch_remote(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Deleting on a server cannot be undone: always ask.
    pub(super) fn remote_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(conn), items) = (self.conn(), self.remote_selection()) else {
            return;
        };
        if items.is_empty() {
            return;
        }
        let what = match items.as_slice() {
            [(p, _)] => format!(
                "“{}”",
                Path::new(p)
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            ),
            many => format!("these {} items", many.len()),
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete {what} from {}?", conn.host),
            Some("It is deleted on the server right away. It does not go to a trash and cannot be brought back."),
            &["Cancel", "Delete"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if let Ok(1) = answer.await {
                let results = cx
                    .background_executor()
                    .spawn(async move {
                        items
                            .iter()
                            .map(|(p, _)| conn.remove(p))
                            .collect::<Vec<_>>()
                    })
                    .await;
                this.update(cx, |this, cx| {
                    if let Some(Err(e)) = results.into_iter().find(Result::is_err) {
                        this.say_in_status(format!("Could not delete: {e}"), cx);
                    }
                    this.fetch_remote(cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// A shell on the server, in the user's terminal, in this folder.
    pub(super) fn remote_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.remote.clone() else {
            return;
        };
        let command = format!(
            "ssh -t {} {}",
            crate::shell::quote(&host),
            crate::shell::quote(&format!(
                "cd {} && exec $SHELL -l",
                crate::shell::quote(&self.cwd.to_string_lossy())
            ))
        );
        match crate::project::write_script(&std::env::temp_dir(), &command) {
            Ok(script) => apps::open_with(&[script], &self.terminal().path),
            Err(e) => self.say_in_status(format!("Could not start it: {e}"), cx),
        }
    }

    /// The "Servers" part of the sidebar: the aliases of ~/.ssh/config and "Connect…".
    pub(super) fn servers_section(&self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.ssh_enabled() {
            return None;
        }
        let row = |id: (&'static str, usize), label: String, active: bool| {
            div()
                .id(id)
                .h(px(28.0))
                .px_2()
                .flex()
                .items_center()
                .gap_2()
                .rounded_md()
                .cursor_pointer()
                .when(active, |d| d.bg(t.hover))
                .hover(|d| d.bg(t.hover))
                .child(icon("icons/server.svg", t.dim))
                .child(div().min_w_0().truncate().child(label))
        };
        let mut section = div().px_2().pt_3().flex().flex_col().gap_0p5().child(
            div()
                .px_2()
                .pb_1()
                .text_size(px(11.0))
                .text_color(t.dim)
                .child("SERVERS"),
        );
        for (i, host) in self.ssh_hosts.iter().take(12).enumerate() {
            let target = host.clone();
            section = section.child(
                row(
                    ("server", i),
                    host.clone(),
                    self.remote.as_deref() == Some(host.as_str()),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.connect_to(&target, cx))),
            );
        }
        section = section.child(
            row(("server-connect", 0), "Connect to Server…".into(), false)
                .text_color(t.dim)
                .on_click(cx.listener(|this, _, _, cx| this.open_connect(cx))),
        );
        Some(section.into_any_element())
    }

    /// The right-click menu on a server's folder.
    pub(super) fn open_remote_menu(&mut self, position: Point<Pixels>, on_item: bool) {
        use MenuAction::*;
        let item = |label: &str, action| MenuEntry::Item(label.to_string().into(), action);
        let mut entries = Vec::new();
        if on_item {
            entries.push(item("Open", RemoteOpen));
            entries.push(item("Download", RemoteDownload));
            entries.push(MenuEntry::Separator);
            entries.push(item("Copy Path", CopyPath));
            if self.selected.len() == 1 {
                entries.push(item("Rename", Rename));
            }
            entries.push(MenuEntry::Separator);
            entries.push(item("Delete…", RemoteDelete));
        } else {
            entries.push(item("New Folder", RemoteNewFolder));
            entries.push(item(
                &format!("Open a shell in {}", self.terminal().name),
                RemoteTerminal,
            ));
            entries.push(item("Refresh", RemoteRefresh));
        }
        self.menu = Some(ContextMenu {
            position,
            entries,
            submenu: None,
        });
    }

    /// Keys while a server's folder is shown: only what works there.
    pub(super) fn on_remote_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let k = &ev.keystroke;
        let (cmd, shift) = (k.modifiers.platform, k.modifiers.shift);
        match k.key.as_str() {
            "up" if cmd => self.go_up(cx),
            "down" | "o" if cmd => self.open_remote(cx),
            "up" => self.move_selection(-1, shift, cx),
            "down" => self.move_selection(1, shift, cx),
            "enter" => {
                self.begin_rename();
                cx.notify();
            }
            "[" if cmd => self.go_back(cx),
            "]" if cmd => self.go_forward(cx),
            "a" if cmd => {
                self.select_all();
                cx.notify();
            }
            "f" if cmd => {
                self.search_focused = true;
                cx.notify();
            }
            "r" if cmd => self.fetch_remote(cx),
            "backspace" if cmd => self.remote_delete(window, cx),
            "n" if cmd && shift => self.remote_new_folder(cx),
            "escape" => {
                self.menu = None;
                self.clear_selection();
                cx.notify();
            }
            _ => {}
        }
    }

    /// What the list area says instead of rows, while connecting or after a failure.
    pub(super) fn remote_notice(&self, t: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let message = if let Some(e) = &self.remote_error {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .max_w(px(420.0))
                        .text_color(t.dim)
                        .child(format!("Could not open this folder: {e}")),
                )
                .child(
                    div()
                        .id("retry-remote")
                        .px_3()
                        .h(px(26.0))
                        .flex()
                        .items_center()
                        .rounded_md()
                        .bg(t.hover)
                        .cursor_pointer()
                        .on_click(cx.listener(|this, _, _, cx| this.fetch_remote(cx)))
                        .child("Try Again"),
                )
        } else if self.remote_loading && self.entries.is_empty() {
            div().text_color(t.dim).child("Loading…")
        } else {
            return None;
        };
        Some(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(message)
                .into_any_element(),
        )
    }
}
