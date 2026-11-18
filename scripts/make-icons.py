#!/usr/bin/env python3
"""Draws the file-type icons of assets/icons/ (the "advanced" icons).

Each icon is a document or a folder with a glyph punched out of it (an SVG hole), so the
app can tint it with any color. Run again if you change a glyph:  python3 scripts/make-icons.py
"""
import math, os

OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "icons")

# --- glyphs: lists of polygons in a 10 x 10 box; holes are punched with fill-rule evenodd -----
def rect(x0, y0, x1, y1):
    return [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]

def circle(cx, cy, r, n=14):
    return [(cx + r * math.cos(2 * math.pi * i / n), cy + r * math.sin(2 * math.pi * i / n)) for i in range(n)]

GLYPHS = {
    "lines": [rect(0, 1, 10, 2.3), rect(0, 4.4, 10, 5.7), rect(0, 7.7, 7, 9)],
    "markdown": [[(0, 9), (0, 1), (2, 1), (5, 5), (8, 1), (10, 1), (10, 9), (8, 9), (8, 4.2), (5, 8), (2, 4.2), (2, 9)]],
    "ribbon": [[(2, 0.5), (8, 0.5), (8, 9.5), (5, 7), (2, 9.5)]],
    "word": [[(0, 1), (2, 1), (3, 6.5), (5, 1), (7, 6.5), (8, 1), (10, 1), (8, 9), (6.2, 9), (5, 4.5), (3.8, 9), (2, 9)]],
    "grid": [rect(0, 0.5, 4.3, 3), rect(5.7, 0.5, 10, 3), rect(0, 3.8, 4.3, 6.2), rect(5.7, 3.8, 10, 6.2), rect(0, 7, 4.3, 9.5), rect(5.7, 7, 10, 9.5)],
    "screen": [rect(0, 1, 10, 8), rect(1.5, 2.5, 8.5, 6.5), rect(3.5, 8.6, 6.5, 9.6)],
    "mountain": [[(0, 9), (3.5, 3.5), (6, 7), (7.5, 5), (10, 9)], circle(7.5, 2.2, 1.3)],
    "note": [rect(5.8, 0.5, 7, 7.5), circle(4.2, 7.8, 2.0), [(7, 0.5), (10, 2), (10, 4), (7, 3)]],
    "play": [[(2.5, 0.5), (9.5, 5), (2.5, 9.5)]],
    "zip": [rect(4.2, 0.5, 5.8, 2), rect(4.2, 2.8, 5.8, 4.3), rect(4.2, 5.1, 5.8, 6.6), rect(3.6, 7.4, 6.4, 9.5)],
    "disk": [circle(5, 5, 4.7, 18), circle(5, 5, 1.4, 10)],
    "app": [rect(1, 1, 9, 9), rect(3.2, 3.2, 6.8, 6.8)],
    "font": [[(0, 9.5), (4, 0.5), (6, 0.5), (10, 9.5), (8, 9.5), (7.2, 7.6), (2.8, 7.6), (2, 9.5)], [(3.6, 6), (5, 2.8), (6.4, 6)]],
    "database": [rect(0.5, 0.5, 9.5, 3), rect(0.5, 3.8, 9.5, 6.2), rect(0.5, 7, 9.5, 9.5)],
    "sliders": [rect(0, 1.5, 10, 2.5), rect(6, 0.5, 8, 3.5), rect(0, 4.5, 10, 5.5), rect(2, 3.5, 4, 6.5), rect(0, 7.5, 10, 8.5), rect(5.5, 6.5, 7.5, 9.5)],
    "log": [rect(0, 1, 1.6, 2.6), rect(3, 1, 10, 2.6), rect(0, 4.2, 1.6, 5.8), rect(3, 4.2, 10, 5.8), rect(0, 7.4, 1.6, 9), rect(3, 7.4, 8, 9)],
    "shell": [[(0.3, 0.9), (1.9, 0.0), (5.9, 4.0), (1.9, 8.0), (0.3, 7.1), (3.4, 4.0)], rect(5.8, 7.5, 10, 9)],
    "key": [circle(2.8, 5, 2.7, 12), circle(2.8, 5, 1.0, 8), rect(5.6, 4.3, 10, 5.7), rect(8, 5.7, 9.2, 7.6), rect(6.2, 5.7, 7.2, 7)],
    "globe": [circle(5, 5, 4.7, 18), circle(5, 5, 3.6, 18), rect(4.4, 1.5, 5.6, 8.5), rect(1.5, 4.4, 4.4, 5.6), rect(5.6, 4.4, 8.5, 5.6)],
    "code": [[(3.8, 1.5), (5, 2.7), (2.7, 5), (5, 7.3), (3.8, 8.5), (0.3, 5)], [(6.2, 1.5), (9.7, 5), (6.2, 8.5), (5, 7.3), (7.3, 5), (5, 2.7)]],
    "book": [rect(0.5, 0.5, 4.6, 9.5), rect(5.4, 0.5, 9.5, 9.5)],
    "pen": [[(5, 0), (9.5, 5), (5, 10), (0.5, 5)]],
    "cube": [[(5, 0), (9.5, 2.5), (9.5, 7.5), (5, 10), (0.5, 7.5), (0.5, 2.5)]],
    "download": [[(4.2, 0), (5.8, 0), (5.8, 5), (8.2, 5), (5, 9), (1.8, 5), (4.2, 5)]],
    "books": [rect(0.5, 1, 2.8, 9), rect(3.5, 0.5, 5.8, 9), rect(6.6, 2, 9, 9)],
    "branch": [circle(2.5, 1.8, 1.3, 10), circle(2.5, 8.2, 1.3, 10), circle(7.6, 4.6, 1.3, 10), rect(2.1, 3.2, 2.9, 6.8), rect(2.9, 4.2, 6.2, 5.0)],
    "monitor": [rect(0.5, 1, 9.5, 7.5), rect(1.8, 2.3, 8.2, 6.2), rect(3.5, 8.2, 6.5, 9.3)],
}

DOC = "M7 3h6.5v4A1.5 1.5 0 0 0 15 8.5h4V19a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"
FOLD = "M13.5 3L19 8.5h-4A1.5 1.5 0 0 1 13.5 7z"
BACK = "M3 6.5A2.5 2.5 0 0 1 5.5 4h3.7c.6 0 1.2.3 1.6.7l1 1.1c.2.2.5.4.9.4h5.8A2.5 2.5 0 0 1 20.5 8.7V17.5A2.5 2.5 0 0 1 18 20H5.5A2.5 2.5 0 0 1 3 17.5z"
FRONT = "M3 10.5A2.5 2.5 0 0 1 5.5 8h13a2.5 2.5 0 0 1 2.5 2.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 17.5z"

def holes(name, box):
    """The glyph as path data, scaled into `box` = (x0, y0, x1, y1) of the 24 x 24 icon."""
    x0, y0, x1, y1 = box
    s = min((x1 - x0), (y1 - y0)) / 10.0
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    d = []
    for poly in GLYPHS[name]:
        pts = [(cx + (x - 5) * s, cy + (y - 5) * s) for x, y in poly]
        d.append("M" + " L".join(f"{x:.2f} {y:.2f}" for x, y in pts) + "z")
    return " ".join(d)

def doc(name, glyph):
    extra = " " + holes(glyph, (6.5, 10.1, 17.5, 19.5)) if glyph else ""
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <path fill="currentColor" fill-rule="evenodd" d="{DOC}{extra}"/>
  <path fill="currentColor" fill-opacity="0.55" d="{FOLD}"/>
</svg>
'''
    open(os.path.join(OUT, name + ".svg"), "w").write(svg)

def folder(name, glyph):
    extra = " " + holes(glyph, (6.6, 10.3, 17.4, 19.1)) if glyph else ""
    svg = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <path fill="currentColor" fill-opacity="0.55" d="{BACK}"/>
  <path fill="currentColor" fill-rule="evenodd" d="{FRONT}{extra}"/>
</svg>
'''
    open(os.path.join(OUT, name + ".svg"), "w").write(svg)

DOCS = {
    "doc-text": "lines", "doc-markdown": "markdown", "doc-pdf": "ribbon", "doc-word": "word", "doc-sheet": "grid",
    "doc-slides": "screen", "doc-image": "mountain", "doc-audio": "note", "doc-video": "play", "doc-archive": "zip",
    "doc-disk": "disk", "doc-app": "app", "doc-font": "font", "doc-database": "database", "doc-config": "sliders",
    "doc-log": "log", "doc-shell": "shell", "doc-secret": "key", "doc-web": "globe", "doc-code": "code",
    "doc-ebook": "book", "doc-design": "pen", "doc-model": "cube",
}
FOLDERS = {
    "dir-docs": "lines", "dir-downloads": "download", "dir-pictures": "mountain", "dir-music": "note",
    "dir-movies": "play", "dir-apps": "app", "dir-library": "books", "dir-code": "code", "dir-git": "branch",
    "dir-packages": "cube", "dir-desktop": "monitor",
}

if __name__ == "__main__":
    for n, g in DOCS.items():
        doc(n, g)
    for n, g in FOLDERS.items():
        folder(n, g)
    print(" ".join(sorted(list(DOCS) + list(FOLDERS))))
