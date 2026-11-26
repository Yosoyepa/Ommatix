"""The README media kit: tokens, type, primitives and the parts every figure is built from.

Standard library only. Every figure is a pure function of a theme (`light`, `dark`) and a
layout (`desktop`, `mobile`), so the same call always writes the same bytes on any machine.

Figures are drawn at the size GitHub shows them: 840 units wide for the desktop README column
and 360 units for a phone. One unit is then about one CSS pixel, and the type scale below is
what the reader sees.
"""
from __future__ import annotations

import html

DESKTOP = 840
MOBILE = 360

# GitHub's own stacks, so text inside a figure matches the text around it.
SANS = '-apple-system,BlinkMacSystemFont,"Segoe UI","Noto Sans",Helvetica,Arial,sans-serif'
MONO = 'ui-monospace,SFMono-Regular,"SF Mono",Menlo,Consolas,"Liberation Mono",monospace'

# Colour roles. Neutrals are GitHub's canvas, border and text colours, so a figure sits in the
# page like a code block does. Three hues carry meaning and nothing else does:
#   accent (teal)  the engine: what Ommatix perceives, publishes and allows
#   agent  (iris)  the agent: the MCP client that reasons and asks
#   warn   (amber) attention: changed pixels, stale references, refusals, preliminary evidence
# `accent_soft` is the first step of a two-step teal ramp for data marks (the results figure): an
# ordered pair told apart by lightness, so it survives colour-vision deficiency.
THEMES = {
    "light": {
        "bg": "#f6f8fa", "surface": "#ffffff", "inset": "#eaeef2", "line": "#d1d9e0", "rule": "#afb8c1",
        "ink": "#1f2328", "muted": "#59636e",
        "accent": "#0a6b62", "accent_tint": "#dcf3ee", "accent_line": "#7fc8bc", "accent_soft": "#5ab8aa",
        "agent": "#5a45c4", "agent_tint": "#ece9fb", "agent_line": "#b3a8ea",
        "warn": "#8a5300", "warn_tint": "#fff1cf", "warn_line": "#dcb25a",
        "on_fill": "#ffffff",
    },
    "dark": {
        "bg": "#151b23", "surface": "#0d1117", "inset": "#212830", "line": "#3d444d", "rule": "#656c76",
        "ink": "#f0f6fc", "muted": "#9198a1",
        "accent": "#4fd1bd", "accent_tint": "#0f2e2c", "accent_line": "#1f7a70", "accent_soft": "#23867b",
        "agent": "#b3a6ff", "agent_tint": "#231f45", "agent_line": "#5a4fa8",
        "warn": "#e3b341", "warn_tint": "#32270a", "warn_line": "#7d6220",
        "on_fill": "#0d1117",
    },
}

# Type scale, in units (about CSS pixels at the README's width). Nothing is set below 12.
OVERLINE, SMALL, BODY, LABEL, TITLE = 12, 13, 14, 15, 20

# Helvetica advance widths per 1000 units of the em: a conservative stand-in for the system
# fonts a reader may have (Segoe UI, SF, Roboto). Used to size pills and to refuse a label
# that would not fit its box on any of them.
_W = {
    " ": 278, "!": 278, '"': 355, "#": 556, "$": 556, "%": 889, "&": 667, "'": 191, "(": 333, ")": 333,
    "*": 389, "+": 584, ",": 278, "-": 333, ".": 278, "/": 278, ":": 278, ";": 278, "<": 584, "=": 584,
    ">": 584, "?": 556, "@": 1015, "[": 278, "\\": 278, "]": 278, "^": 469, "_": 556, "`": 333,
    "{": 334, "|": 260, "}": 334, "~": 584, "·": 278, "→": 1000, "←": 1000, "↓": 1000, "↑": 1000, "—": 1000,
    "–": 556, "…": 1000, "“": 333, "”": 333, "‘": 222, "’": 222, "×": 584, "✓": 800, "•": 350,
    "A": 667, "B": 667, "C": 722, "D": 722, "E": 667, "F": 611, "G": 778, "H": 722, "I": 278, "J": 500,
    "K": 667, "L": 556, "M": 833, "N": 722, "O": 778, "P": 667, "Q": 778, "R": 722, "S": 667, "T": 611,
    "U": 722, "V": 667, "W": 944, "X": 667, "Y": 667, "Z": 611,
    "a": 556, "b": 556, "c": 500, "d": 556, "e": 556, "f": 278, "g": 556, "h": 556, "i": 222, "j": 222,
    "k": 500, "l": 222, "m": 833, "n": 556, "o": 556, "p": 556, "q": 556, "r": 333, "s": 500, "t": 278,
    "u": 556, "v": 500, "w": 722, "x": 500, "y": 500, "z": 500,
    "á": 556, "é": 556, "í": 278, "ó": 556, "ú": 556, "ñ": 556, "ü": 556,
}
MONO_ADVANCE = 0.61  # of the em; the widest of the stack's faces


class FitError(ValueError):
    """A label would overflow its box."""


def width(value: str, size: float, weight: int = 400, mono: bool = False, spacing: float = 0.0) -> float:
    """The estimated advance of `value`, in units, with a margin for wider system fonts."""
    if mono:
        return len(value) * (size * MONO_ADVANCE + spacing)
    units = sum(_W.get(ch, 556) for ch in value) / 1000 * size
    return (units * (1.07 if weight >= 600 else 1.0) + len(value) * spacing) * 1.04


def n(value: float) -> str:
    """A number as short as SVG allows: at most two decimals, no trailing zeros."""
    text = f"{round(float(value) + 0.0, 2):.2f}".rstrip("0").rstrip(".")
    return "0" if text in ("-0", "") else text


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def text(x: float, y: float, value: str, size: float = BODY, fill: str = "#000", weight: int = 400,
         anchor: str = "start", mono: bool = False, spacing: float = 0.0, fit: float | None = None,
         extra: str = "", smallest: float = 12) -> str:
    """One line of text. `fit` is the widest it may be; a label that would not fit is an error."""
    if size < smallest:
        raise FitError(f"{value!r}: {size} is below the smallest type size")
    if fit is not None and width(value, size, weight, mono, spacing) > fit:
        raise FitError(f"{value!r} at {size} needs {width(value, size, weight, mono, spacing):.0f}, has {fit:.0f}")
    parts = [f'<text x="{n(x)}" y="{n(y)}" font-size="{n(size)}" fill="{fill}"']
    if weight != 400:
        parts.append(f' font-weight="{weight}"')
    if anchor != "start":
        parts.append(f' text-anchor="{anchor}"')
    if mono:
        parts.append(' class="m"')
    if spacing:
        parts.append(f' letter-spacing="{n(spacing)}"')
    if extra:
        parts.append(" " + extra)
    return "".join(parts) + f">{esc(value)}</text>"


def rect(x: float, y: float, w: float, h: float, fill: str = "none", stroke: str | None = None, r: float = 6,
         sw: float = 1, extra: str = "") -> str:
    out = f'<rect x="{n(x)}" y="{n(y)}" width="{n(w)}" height="{n(h)}"'
    if r:
        out += f' rx="{n(r)}"'
    out += f' fill="{fill}"'
    if stroke:
        out += f' stroke="{stroke}"' + (f' stroke-width="{n(sw)}"' if sw != 1 else "")
    return out + (" " + extra if extra else "") + "/>"


def path(d: str, stroke: str, sw: float = 1.5, fill: str = "none", extra: str = "") -> str:
    return (f'<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{n(sw)}" stroke-linecap="round" '
            f'stroke-linejoin="round"' + (" " + extra if extra else "") + "/>")


def group(body: str, extra: str = "") -> str:
    return f"<g{(' ' + extra) if extra else ''}>{body}</g>"


def arrow(x1: float, y1: float, x2: float, y2: float, stroke: str, sw: float = 1.5, head: float = 4.5) -> str:
    """A straight arrow along an axis, with an open head at (x2, y2)."""
    if abs(x2 - x1) >= abs(y2 - y1):
        s = 1 if x2 > x1 else -1
        wings = f"M{n(x2 - s * head)} {n(y2 - head)}L{n(x2)} {n(y2)}L{n(x2 - s * head)} {n(y2 + head)}"
    else:
        s = 1 if y2 > y1 else -1
        wings = f"M{n(x2 - head)} {n(y2 - s * head)}L{n(x2)} {n(y2)}L{n(x2 + head)} {n(y2 - s * head)}"
    return path(f"M{n(x1)} {n(y1)}L{n(x2)} {n(y2)}{wings}", stroke, sw)


def elbow(points: list[tuple[float, float]], stroke: str, sw: float = 1.5, head: float = 4.5, r: float = 6) -> str:
    """A rounded polyline through `points`, ending in an arrow head. Segments are axis-aligned."""
    d = f"M{n(points[0][0])} {n(points[0][1])}"
    for i in range(1, len(points) - 1):
        (px, py), (cx, cy), (nx, ny) = points[i - 1], points[i], points[i + 1]
        ax = cx - r * (1 if cx > px else -1 if cx < px else 0)
        ay = cy - r * (1 if cy > py else -1 if cy < py else 0)
        bx = cx + r * (1 if nx > cx else -1 if nx < cx else 0)
        by = cy + r * (1 if ny > cy else -1 if ny < cy else 0)
        d += f"L{n(ax)} {n(ay)}Q{n(cx)} {n(cy)} {n(bx)} {n(by)}"
    (px, py), (x2, y2) = points[-2], points[-1]
    d += f"L{n(x2)} {n(y2)}"
    if abs(x2 - px) >= abs(y2 - py):
        s = 1 if x2 > px else -1
        d += f"M{n(x2 - s * head)} {n(y2 - head)}L{n(x2)} {n(y2)}L{n(x2 - s * head)} {n(y2 + head)}"
    else:
        s = 1 if y2 > py else -1
        d += f"M{n(x2 - head)} {n(y2 - s * head)}L{n(x2)} {n(y2)}L{n(x2 + head)} {n(y2 - s * head)}"
    return path(d, stroke, sw)


def document(w: float, h: float, title: str, description: str, body: str, style: str = "") -> str:
    """The SVG file. Every figure carries a title, a description and the image role."""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {n(w)} {n(h)}" width="{n(w)}" height="{n(h)}" '
            f'role="img" aria-labelledby="title desc"><title id="title">{esc(title)}</title>'
            f'<desc id="desc">{esc(description)}</desc><style>text{{font-family:{SANS}}}.m{{font-family:{MONO}}}'
            f"{style}</style>{body}</svg>\n")


def card(w: float, h: float, p: dict) -> str:
    """The frame every figure sits in: its own background and border, so it reads on any page."""
    return rect(0.5, 0.5, w - 1, h - 1, p["bg"], p["line"], r=10)


def overline(x: float, y: float, value: str, p: dict, role: str = "accent", anchor: str = "start",
             fit: float | None = None) -> str:
    """Small capitals above a title or a lane."""
    return text(x, y, value.upper(), OVERLINE, p[role], 600, anchor, spacing=0.9, fit=fit)


def heading(x: float, y: float, title: str, subtitle: str, p: dict, fit: float) -> str:
    """A figure's title and the one sentence under it."""
    return (text(x, y, title, TITLE, p["ink"], 700, fit=fit) +
            text(x, y + 23, subtitle, BODY, p["muted"], fit=fit))


def pill(x: float, y: float, value: str, p: dict, role: str = "accent", size: float = SMALL, mono: bool = False,
         pad: float = 9, h: float = 24, filled: bool = False, anchor: str = "start") -> tuple[str, float]:
    """A rounded label sized to its text. Returns the markup and its width."""
    w = round(width(value, size, 600, mono) + 2 * pad)
    if anchor == "end":
        x -= w
    elif anchor == "middle":
        x -= w / 2
    fill, ink = (p[role], p["on_fill"]) if filled else (p[role + "_tint"], p[role])
    return (rect(x, y, w, h, fill, None if filled else p[role + "_line"], r=h / 2) +
            text(x + w / 2, y + h / 2 + size * 0.35, value, size, ink, 600, "middle", mono)), w


def tag(x: float, y: float, value: str, p: dict, role: str = "accent", size: float = SMALL) -> tuple[str, float]:
    """A square-cornered identifier, set in the monospace face: `e7`, `s17`, a status."""
    w = round(width(value, size, 600, True) + 14)
    return (rect(x, y, w, 22, p[role + "_tint"], p[role + "_line"], r=4) +
            text(x + w / 2, y + 15.5, value, size, p[role], 600, "middle", True)), w


def mark(cx: float, cy: float, r: float, p: dict) -> str:
    """The project's mark: seven facets of a compound eye, the centre one lit."""
    out = ""
    for i, (dx, dy) in enumerate([(0, 0), (1, 0), (0.5, -0.866), (-0.5, -0.866), (-1, 0), (-0.5, 0.866), (0.5, 0.866)]):
        x, y = cx + dx * r * 1.86, cy + dy * r * 1.86
        points = " ".join(f"{n(x + r * c)},{n(y + r * s)}" for c, s in
                          [(0.866, 0.5), (0, 1), (-0.866, 0.5), (-0.866, -0.5), (0, -1), (0.866, -0.5)])
        out += (f'<polygon points="{points}" fill="{p["accent"] if i == 0 else p["accent_tint"]}" '
                f'stroke="{p["accent"]}" stroke-width="{n(max(1, r / 6))}" stroke-linejoin="round"/>')
    return out


# Pictograms: one line weight, one 24-unit box, drawn from (x, y) as the top-left corner.
def icon(name: str, x: float, y: float, p: dict, role: str = "accent", scale: float = 1.0) -> str:
    c, t, sw = p[role], p[role + "_tint"], 1.5
    shapes = {
        # a frame with a title bar
        "capture": rect(2.5, 4.5, 19, 15, t, c, 2.5, sw) + path("M2.5 9h19", c, sw),
        # tiles, the changed ones filled
        "plan": (rect(3, 3, 18, 18, "none", c, 2, sw) + path("M9 3v18M15 3v18M3 9h18M3 15h18", c, 1) +
                 rect(9, 9, 6, 6, c, None, 0) + rect(15, 9, 6, 6, c, None, 0)),
        # a detected box with text inside
        "detect": (rect(2.5, 5.5, 19, 13, "none", c, 2, sw, 'stroke-dasharray="3 2.5"') +
                   path("M7 15l2.5-6 2.5 6M8 13.2h3M14.5 12.3a1.8 1.8 0 1 0 0 2.4M16.3 10.6v4.4", c, 1.3)),
        # two boxes resolved into one
        "fuse": rect(3, 4, 12, 11, t, c, 2, sw) + rect(9, 9, 12, 11, "none", c, 2, sw),
        # a glyph looked up in a small set
        "icons": (rect(3, 3, 7.5, 7.5, t, c, 1.5, sw) + rect(13.5, 3, 7.5, 7.5, "none", c, 1.5, sw) +
                  rect(3, 13.5, 7.5, 7.5, "none", c, 1.5, sw) + path("M15.2 15.6a2.2 2.2 0 1 1 3 2.1c-.7.4-.9.8-.9 1.4M17.3 21v.1", c, 1.4)),
        # a label tied to a thing
        "identity": path("M3.5 4.5h8.2l8.8 8.8-7.2 7.2-9.8-9.8z", c, sw, t) + f'<circle cx="8" cy="9" r="1.6" fill="{c}"/>',
        # layers committed together
        "snapshot": path("M12 3.5l9 4.5-9 4.5-9-4.5z", c, sw, t) + path("M3 12.5l9 4.5 9-4.5M3 16.5l9 4.5 9-4.5", c, sw),
        # the reasoning client
        "agent": (path("M12 2.5l1.9 5.6 5.6 1.9-5.6 1.9-1.9 5.6-1.9-5.6-5.6-1.9 5.6-1.9z", c, sw, t) +
                  path("M19 16.5l.8 2.2 2.2.8-2.2.8-.8 2.2-.8-2.2-2.2-.8 2.2-.8z", c, 1.2, c)),
        # a shield
        "gate": path("M12 2.5l8 3v6.2c0 5-3.4 8.3-8 9.8-4.6-1.5-8-4.8-8-9.8V5.5z", c, sw, t) + path("M8.4 12.2l2.6 2.6 4.8-5.2", c, sw),
        # a pointer
        "actuator": path("M6 3.5l12.5 8.2-5.4 1.1 3.3 6.4-2.4 1.2-3.2-6.4-4.1 3.6z", c, sw, t),
        # an eye on the result
        "verify": (path("M2 12c2.6-4.4 6-6.5 10-6.5s7.4 2.1 10 6.5c-2.6 4.4-6 6.5-10 6.5S4.6 16.4 2 12z", c, sw, t) +
                   f'<circle cx="12" cy="12" r="3" fill="{c}"/>'),
        # a refusal
        "stop": f'<circle cx="12" cy="12" r="9" fill="{t}" stroke="{c}" stroke-width="{sw}"/>' + path("M7.5 12h9", c, 2),
        # a clock
        "age": f'<circle cx="12" cy="12" r="9" fill="{t}" stroke="{c}" stroke-width="{sw}"/>' + path("M12 7v5.2l3.4 2", c, sw),
        # once more
        "again": path("M19.8 12a7.8 7.8 0 1 1-2.6-5.8M19.8 3.6v4.6h-4.6", c, sw) + f'<circle cx="12" cy="12" r="2.2" fill="{c}"/>',
    }
    body = shapes[name]
    transform = f"translate({n(x)} {n(y)})" + (f" scale({n(scale)})" if scale != 1.0 else "")
    return f'<g transform="{transform}">{body}</g>'


def contrast(a: str, b: str) -> float:
    """WCAG 2 contrast ratio of two #rrggbb colours."""
    def luminance(colour: str) -> float:
        channels = [int(colour[i:i + 2], 16) / 255 for i in (1, 3, 5)]
        r, g, bl = (v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in channels)
        return 0.2126 * r + 0.7152 * g + 0.0722 * bl
    hi, lo = sorted((luminance(a), luminance(b)), reverse=True)
    return (hi + 0.05) / (lo + 0.05)
