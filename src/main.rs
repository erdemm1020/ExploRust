mod apps;
mod archive;
mod assets;
mod browser;
mod clipboard;
mod config;
mod dates;
mod fs;
mod info;
mod jobs;
mod sizes;
mod text_field;
mod theme;
mod trash;
mod undo;
mod updater;

use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, SystemMenuType, TitlebarOptions,
    WindowBounds, WindowOptions, actions, point, prelude::*, px, size,
};
use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;

use browser::Browser;

actions!(explorust, [Quit, About, OpenSettings, CheckForUpdates]);

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-,", OpenSettings, None),
            ]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            // The standard macOS "About" panel: icon, name and version from Info.plist.
            cx.on_action(|_: &About, _| {
                if let Some(mtm) = MainThreadMarker::new() {
                    NSApplication::sharedApplication(mtm).orderFrontStandardAboutPanel(None);
                }
            });
            cx.set_menus(vec![Menu {
                name: "ExploRust".into(),
                items: vec![
                    MenuItem::action("About ExploRust", About),
                    MenuItem::separator(),
                    MenuItem::action("Settings…", OpenSettings),
                    MenuItem::action("Check for Updates…", CheckForUpdates),
                    MenuItem::separator(),
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit ExploRust", Quit),
                ],
            }]);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(980.0), px(640.0)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ExploRust".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(14.0), px(16.0))),
                    }),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| Browser::new(window, cx)),
            )
            .unwrap();
            cx.activate(true);
        });
}
