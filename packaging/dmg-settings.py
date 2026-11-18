# Layout of the DMG window, read by dmgbuild (see scripts/make-dmg.sh).
import os

app = defines.get("app", "dist/ExploRust.app")  # noqa: F821  (`defines` comes from dmgbuild)
app_name = os.path.basename(app)

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}

icon = "assets/AppIcon.icns"  # the icon of the mounted volume
background = "assets/dmg-background.tiff"

# Same size as the background picture (660 x 400 points).
window_rect = ((200, 140), (660, 400))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False

icon_size = 128
text_size = 13
arrange_by = None
# Centers of the two icons: the arrow in the picture sits between them.
icon_locations = {
    app_name: (165, 195),
    "Applications": (495, 195),
}
