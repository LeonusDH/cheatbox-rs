#!/usr/bin/env python3
"""Render cheatbox sheets to PNG previews (images/<name>.png).

Runs the real binary with --color always, parses the ANSI output and draws
it with Pillow. Box-drawing borders are drawn as vector lines so they stay
continuous regardless of line height.

    cargo build --release
    pip install pillow
    python tools/preview.py                 # all sheets
    python tools/preview.py linux git       # some sheets
    python tools/preview.py --font path/to/JetBrainsMono-Regular.ttf

Font lookup: --font, then JetBrains Mono / Cascadia Mono / Consolas /
DejaVu Sans Mono in usual system locations. Glyphs missing from the main
font fall back to DejaVu Sans Mono when available.
"""
import argparse
import os
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent

# ---------------------------------------------------------------- palette
BG = (24, 22, 22)
FG = (214, 206, 190)
ANSI16 = [
    (40, 38, 37), (226, 106, 96), (152, 190, 101), (222, 177, 92),
    (102, 153, 214), (198, 120, 200), (86, 182, 194), (214, 206, 190),
    (110, 104, 98), (255, 128, 112), (178, 214, 120), (245, 204, 120),
    (130, 178, 240), (222, 150, 224), (120, 210, 220), (250, 246, 236),
]


def xterm(i):
    if i < 16:
        return ANSI16[i]
    if i < 232:
        i -= 16
        lv = [0, 95, 135, 175, 215, 255]
        return (lv[i // 36], lv[(i // 6) % 6], lv[i % 6])
    g = 8 + (i - 232) * 10
    return (g, g, g)


def blend(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


# ---------------------------------------------------------------- ansi
SGR = re.compile(r"\x1b\[([0-9;]*)m")


class St:
    __slots__ = ("fg", "fgi", "bg", "bold", "dim", "italic", "under", "rev", "strike")

    def __init__(self):
        self.reset()

    def reset(self):
        self.fg = self.bg = self.fgi = None
        self.bold = self.dim = self.italic = self.under = self.rev = self.strike = False

    def copy(self):
        s = St()
        for k in self.__slots__:
            setattr(s, k, getattr(self, k))
        return s

    def apply(self, params):
        p = [int(x) if x else 0 for x in params.split(";")] if params else [0]
        i = 0
        while i < len(p):
            c = p[i]
            if c == 0:
                self.reset()
            elif c == 1:
                self.bold = True
            elif c == 2:
                self.dim = True
            elif c == 3:
                self.italic = True
            elif c == 4:
                self.under = True
            elif c == 7:
                self.rev = True
            elif c == 9:
                self.strike = True
            elif c == 22:
                self.bold = self.dim = False
            elif 30 <= c <= 37:
                self.fg, self.fgi = ANSI16[c - 30], c - 30
            elif c == 39:
                self.fg = self.fgi = None
            elif 40 <= c <= 47:
                self.bg = ANSI16[c - 40]
            elif c == 49:
                self.bg = None
            elif 90 <= c <= 97:
                self.fg, self.fgi = ANSI16[c - 90 + 8], None
            elif 100 <= c <= 107:
                self.bg = ANSI16[c - 100 + 8]
            elif c in (38, 48) and i + 1 < len(p):
                if p[i + 1] == 5 and i + 2 < len(p):
                    col = xterm(p[i + 2])
                    i += 2
                elif p[i + 1] == 2 and i + 4 < len(p):
                    col = tuple(p[i + 2:i + 5])
                    i += 4
                else:
                    col = None
                if c == 38:
                    self.fg, self.fgi = col, None
                else:
                    self.bg = col
                i += 1
            i += 1


def parse(text):
    """-> list of rows, each a list of (char, St)."""
    rows, cur = [], St()
    for line in text.split("\n"):
        row, pos = [], 0
        for m in SGR.finditer(line):
            row += [(ch, cur.copy()) for ch in line[pos:m.start()]]
            cur.apply(m.group(1))
            pos = m.end()
        row += [(ch, cur.copy()) for ch in line[pos:]]
        rows.append(row)
    while rows and not rows[-1]:
        rows.pop()
    return rows


# ---------------------------------------------------------------- fonts
def find_font(names):
    dirs = []
    if platform.system() == "Windows":
        dirs += [Path(os.environ.get("WINDIR", r"C:\Windows")) / "Fonts",
                 Path(os.environ.get("LOCALAPPDATA", "")) / "Microsoft" / "Windows" / "Fonts"]
    elif platform.system() == "Darwin":
        dirs += [Path("/Library/Fonts"), Path.home() / "Library/Fonts", Path("/System/Library/Fonts")]
    else:
        dirs += [Path("/usr/share/fonts"), Path("/usr/local/share/fonts"), Path.home() / ".local/share/fonts"]
    for n in names:
        for d in dirs:
            if d.is_dir():
                hit = next(d.rglob(n), None)
                if hit:
                    return str(hit)
    return None


MAIN = [("JetBrainsMono-Regular.ttf", "JetBrainsMono-Bold.ttf"),
        ("CascadiaMono.ttf", "CascadiaMono.ttf"),
        ("consola.ttf", "consolab.ttf"),
        ("DejaVuSansMono.ttf", "DejaVuSansMono-Bold.ttf")]


class Fonts:
    def __init__(self, path, size):
        if path:
            reg = path
            bold = path.replace("Regular", "Bold") if "Regular" in path else path
            bold = bold if Path(bold).exists() else path
        else:
            reg = bold = None
            for r, b in MAIN:
                reg = find_font([r])
                if reg:
                    bold = find_font([b]) or reg
                    break
        if not reg:
            sys.exit("no monospace font found, pass --font")
        self.reg = ImageFont.truetype(reg, size)
        self.bold = ImageFont.truetype(bold, size)
        fb = find_font(["DejaVuSansMono.ttf"])
        self.fallback = ImageFont.truetype(fb, size) if fb else None
        self.cw = self.reg.getlength("M")
        self._cache = {}
        self.name = Path(reg).name

    def pick(self, ch, bold):
        f = self.bold if bold else self.reg
        key = (ch, bold)
        if key not in self._cache:
            self._cache[key] = self.fallback if self.fallback and ord(ch) > 126 and _missing(f, ch) else f
        return self._cache[key]


def _glyph(font, ch):
    im = Image.new("L", (font.size * 2, font.size * 2))
    ImageDraw.Draw(im).text((0, 0), ch, font=font, fill=255)
    return im.tobytes()


def _missing(font, ch):
    """A missing glyph renders as .notdef, same as a surely-unassigned codepoint."""
    return _glyph(font, ch) == _glyph(font, "\U000F0000")


# ---------------------------------------------------------------- drawing
BOX = {  # char -> (left, right, up, down, heavy, rounded)
    "─": (1, 1, 0, 0, 0, 0), "│": (0, 0, 1, 1, 0, 0), "┃": (0, 0, 1, 1, 1, 0),
    "╭": (0, 1, 0, 1, 0, 1), "╮": (1, 0, 0, 1, 0, 1), "╰": (0, 1, 1, 0, 0, 1), "╯": (1, 0, 1, 0, 0, 1),
    "┌": (0, 1, 0, 1, 0, 0), "┐": (1, 0, 0, 1, 0, 0), "└": (0, 1, 1, 0, 0, 0), "┘": (1, 0, 1, 0, 0, 0),
}


def draw_box(d, ch, x, y, cw, lh, col, s):
    l, r, u, dn, heavy, rounded = BOX[ch]
    w = max(1, round((2.2 if heavy else 1.15) * s))
    cx, cy = x + cw / 2, y + lh / 2
    if rounded:
        rad = min(cw, lh) / 2
        # arc from the horizontal arm to the vertical arm
        hx = x + cw if r else x
        vy = y + lh if dn else y
        bx0, bx1 = (cx, cx + 2 * rad) if r else (cx - 2 * rad, cx)
        by0, by1 = (cy, cy + 2 * rad) if dn else (cy - 2 * rad, cy)
        start = {(1, 1): 180, (0, 1): 270, (1, 0): 90, (0, 0): 0}[(r, dn)]
        d.arc([bx0, by0, bx1, by1], start, start + 90, fill=col, width=w)
        if r:
            d.line([(cx + rad, cy), (hx, cy)], fill=col, width=w)
        else:
            d.line([(hx, cy), (cx - rad, cy)], fill=col, width=w)
        if dn:
            d.line([(cx, cy + rad), (cx, vy)], fill=col, width=w)
        else:
            d.line([(cx, vy), (cx, cy - rad)], fill=col, width=w)
        return
    if l or r:
        d.line([(x if l else cx, cy), (x + cw if r else cx, cy)], fill=col, width=w)
    if u or dn:
        d.line([(cx, y if u else cy), (cx, y + lh if dn else cy)], fill=col, width=w)


def background(w, h):
    """Dark base with two soft glows, close to the original screenshots."""
    small = Image.new("RGB", (64, 64), BG)
    px = small.load()
    for j in range(64):
        for i in range(64):
            t1 = max(0.0, 1 - (((i - 58) / 40) ** 2 + ((j - 60) / 34) ** 2))
            t2 = max(0.0, 1 - (((i - 8) / 46) ** 2 + ((j - 4) / 40) ** 2))
            c = blend(BG, (92, 46, 26), 0.55 * t1)
            c = blend(c, (34, 44, 46), 0.6 * t2)
            px[i, j] = c
    return small.resize((w, h), Image.BICUBIC)


def render(rows, fonts, scale, pad):
    cw = fonts.cw
    lh = round(fonts.reg.size * 1.42)
    cols = max(sum(1 for _ in r) for r in rows)
    W = round(cols * cw + 2 * pad)
    H = round(len(rows) * lh + 2 * pad)
    img = background(W, H)
    d = ImageDraw.Draw(img)
    asc, _ = fonts.reg.getmetrics()
    for ri, row in enumerate(rows):
        y = pad + ri * lh
        for ci, (ch, st) in enumerate(row):
            x = pad + ci * cw
            fg = st.fg or FG
            if st.bold and st.fgi is not None:  # "bold is bright", like most terminals
                fg = ANSI16[st.fgi + 8]
            bg = st.bg
            if st.rev:
                fg, bg = (bg or BG), fg
            if st.dim:
                fg = blend(fg, BG, 0.45)
            if bg:
                d.rectangle([x, y, x + cw + 0.5, y + lh], fill=bg)
            if ch == " ":
                continue
            if ch in BOX:
                draw_box(d, ch, x, y, cw, lh, fg, scale)
                continue
            f = fonts.pick(ch, st.bold)
            ty = y + (lh - (asc + fonts.reg.getmetrics()[1])) / 2
            d.text((x, ty), ch, font=f, fill=fg)
            if st.under:
                d.line([(x, y + lh - 3 * scale), (x + cw, y + lh - 3 * scale)], fill=fg, width=max(1, round(scale)))
    return img


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("sheets", nargs="*", help="sheet names (default: all)")
    ap.add_argument("--bin", default=None, help="cheatbox binary (default: target/release/cheatbox)")
    ap.add_argument("--font", default=None)
    ap.add_argument("--size", type=float, default=14, help="font size in px at 1x")
    ap.add_argument("--scale", type=float, default=2, help="pixel density (2 = crisp on HiDPI)")
    ap.add_argument("--out", default=str(ROOT / "images"))
    a = ap.parse_args()

    exe = a.bin or str(ROOT / "target" / "release" / ("cheatbox.exe" if os.name == "nt" else "cheatbox"))
    if not Path(exe).exists():
        exe = shutil.which("cheatbox") or sys.exit("build first: cargo build --release")
    names = a.sheets or [l.split()[0] for l in subprocess.run(
        [exe, "list"], capture_output=True, text=True, check=True).stdout.splitlines() if l.strip()]

    fonts = Fonts(a.font, round(a.size * a.scale))
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    for n in names:
        txt = subprocess.run([exe, "show", n, "--color", "always"], capture_output=True,
                             text=True, encoding="utf-8", check=True).stdout
        img = render(parse(txt), fonts, a.scale, round(18 * a.scale))
        p = out / f"{n}.png"
        img.save(p, optimize=True)
        print(f"{p.relative_to(ROOT) if p.is_relative_to(ROOT) else p}  {img.width}x{img.height}  ({fonts.name})")


if __name__ == "__main__":
    main()
