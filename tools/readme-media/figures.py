"""The still figures of the README: badges, hero, pipeline, gate, results and social preview.

Each function returns one SVG document for a theme and a layout. Labels are English in both
READMEs (AGENTS.md, language policy); the alt text beside each image is translated.
Facts drawn here are the repository's own: the contract examples for identifiers; AGENTS.md
rule 6 and the order of `src/application/src/engine/act.rs` for the gate; chapter 09 §9.11 for
the scores.

A figure carries only the words its drawing needs: the README counts the words drawn in its
figures with the words of its prose (`check.py` prints both).
"""
from __future__ import annotations

from kit import (BODY, DESKTOP, LABEL, MOBILE, SMALL, THEMES, TITLE, arrow, card, document, elbow, group, heading,
                 icon, mark, n, overline, path, pill, rect, tag, text, width)

# ---------------------------------------------------------------- badges

BADGES = {
    # name: (label, value, role of the value)
    "code": ("code", "MIT OR Apache-2.0", "accent"),
    "docs": ("docs", "CC BY 4.0", "accent"),
    "rust": ("rust", "{rust}", "accent"),
    "mcp": ("MCP", "stdio", "accent"),
    "status": ("status", "research prototype", "warn"),
    "platform": ("platform", "Windows 11 x64 first", "accent"),
}


def badge(label: str, value: str, role: str) -> str:
    """A two-part badge with the geometry of the CI badge beside it: 20 high, 11-unit type."""
    p = THEMES["light"]
    lw, vw = round(width(label, 11, 600) + 12), round(width(value, 11, 600) + 12)
    body = (f'<clipPath id="r"><rect width="{lw + vw}" height="20" rx="3"/></clipPath><g clip-path="url(#r)">'
            + rect(0, 0, lw, 20, "#3d4752", r=0) + rect(lw, 0, vw, 20, p[role], r=0) + "</g>"
            + text(lw / 2, 14, label, 11, "#ffffff", 600, "middle", smallest=11)
            + text(lw + vw / 2, 14, value, 11, "#ffffff", 600, "middle", smallest=11))
    return document(lw + vw, 20, f"{label}: {value}", f"{label}: {value}", body)


# ---------------------------------------------------------------- shared drawings

def mini_screen(x: float, y: float, w: float, h: float, p: dict, tiles: int = 6) -> str:
    """A simulated window seen as the engine sees it: tiles, the changed ones marked, one control found."""
    cell = w / tiles
    rows = int((h - 18) // cell)
    out = rect(x, y, w, h, p["surface"], p["line"], r=6) + path(f"M{n(x)} {n(y + 18)}h{n(w)}", p["line"], 1)
    out += "".join(f'<circle cx="{n(x + 10 + i * 9)}" cy="{n(y + 9)}" r="2.5" fill="{p["line"]}"/>' for i in range(3))
    top = y + 18 + (h - 18 - rows * cell) / 2
    # content: a heading, two lines, a field and a button
    out += rect(x + cell * 0.5, top + cell * 0.35, cell * 2.2, 6, p["rule"], r=3)
    out += rect(x + cell * 0.5, top + cell * 1.25, cell * 3.6, cell * 0.62, p["inset"], p["line"], r=3)
    out += rect(x + cell * 0.5, top + cell * 2.3, cell * 2.8, 5, p["inset"], r=2.5)
    out += rect(x + cell * 0.5, top + cell * 2.3 + 10, cell * 2.0, 5, p["inset"], r=2.5)
    # the changed tiles: the row of the field
    for col in range(4):
        out += rect(x + col * cell + 1, top + cell + 1, cell - 2, cell - 2, p["warn_tint"], p["warn_line"], r=2,
                    extra='fill-opacity="0.75"')
    out += rect(x + cell * 0.5, top + cell * 1.25, cell * 3.6, cell * 0.62, "none", p["warn"], r=3)
    # the grid itself
    grid = "".join(f"M{n(x + i * cell)} {n(top)}v{n(rows * cell)}" for i in range(1, tiles))
    grid += "".join(f"M{n(x)} {n(top + j * cell)}h{n(w)}" for j in range(0, rows + 1))
    out += path(grid, p["line"], 0.75)
    # the control the engine found, with its identifier
    bx, by, bw, bh = x + w - cell * 2.45, top + cell * (rows - 1) + cell * 0.18, cell * 2.1, cell * 0.64
    out += rect(bx, by, bw, bh, p["accent_tint"], p["accent"], r=4, sw=1.5)
    out += rect(bx + bw * 0.22, by + bh / 2 - 2.5, bw * 0.56, 5, p["accent"], r=2.5)
    return out


def element_rows(x: float, y: float, w: float, h: float, p: dict, size: float = SMALL) -> str:
    """What the agent receives: identifiers, roles and revisions; never a position."""
    out = rect(x, y, w, h, p["surface"], p["line"], r=6)
    step = h / 3
    for i, (ident, role, rev) in enumerate([("e7", "button", "rev 3"), ("e8", "text_node", "rev 1")]):
        ry = y + i * step + step / 2
        t, tw = tag(x + 10, ry - 11, ident, p, size=size)
        out += t + text(x + 10 + tw + 8, ry + size * 0.35, role, size, p["ink"], mono=True)
        out += text(x + w - 10, ry + size * 0.35, rev, size, p["muted"], anchor="end", mono=True,
                    fit=w - 28 - tw - width(role, size, mono=True))
        out += path(f"M{n(x)} {n(y + (i + 1) * step)}h{n(w)}", p["line"], 1)
    cy = y + 2.5 * step
    label = "(412, 388)"
    out += text(x + 10, cy + size * 0.35, label, size, p["muted"], mono=True)
    out += path(f"M{n(x + 8)} {n(cy)}h{n(width(label, size, mono=True) + 4)}", p["warn"], 1.5)
    out += text(x + w - 10, cy + size * 0.35, "never sent", size, p["warn"], 600, "end",
                fit=w - 26 - width(label, size, mono=True))
    return out


# ---------------------------------------------------------------- hero

# Chapter 04 §4.5: an element whose continuity is not proven gets a new ID, never a neighbour's.
IDENTITY = "IDs hold while continuity is proven."


def hero(theme: str, layout: str) -> str:
    p = THEMES[theme]
    title = "Ommatix: a stable interface to a changing desktop"
    desc = ("Left, a simulated window divided into tiles; the tiles that changed are marked and one control is "
            "outlined. Right, what the agent receives: element e7, a button at revision 3, and e8, a text node "
            "at revision 1; a coordinate pair is struck out as never sent. An element keeps its identifier while "
            "its continuity is proven.")
    if layout == "desktop":
        w, h = DESKTOP, 206
        body = card(w, h, p) + mark(46, 46, 5.2, p) + overline(70, 50, "Perception · identity · control", p)
        body += text(32, 100, "A stable interface", 31, p["ink"], 700, fit=330)
        body += text(32, 138, "to a changing desktop.", 31, p["ink"], 700, fit=350)
        body += text(32, 170, IDENTITY, BODY, p["muted"], fit=350)
        sx, sy = 392, 46
        body += overline(sx, 36, "Pixels stay local", p, "muted") + mini_screen(sx, sy, 168, 136, p)
        body += arrow(sx + 178, sy + 68, sx + 204, sy + 68, p["accent"], 2, 5.5)
        body += overline(sx + 214, 36, "The agent gets", p, "agent") + element_rows(sx + 214, sy, 210, 136, p)
        return document(w, h, title, desc, body)
    w, h = MOBILE, 300
    body = card(w, h, p) + mark(30, 37, 4.6, p) + overline(50, 41, "Perception · identity · control", p, fit=290)
    body += text(20, 78, "A stable interface", 24, p["ink"], 700, fit=320)
    body += text(20, 108, "to a changing desktop.", 24, p["ink"], 700, fit=320)
    body += text(20, 134, IDENTITY, SMALL, p["muted"], fit=320)
    body += overline(20, 168, "Pixels stay local", p, "muted") + mini_screen(20, 178, 118, 102, p)
    body += arrow(142, 229, 156, 229, p["accent"], 2, 4.5)
    body += overline(162, 168, "The agent gets", p, "agent") + element_rows(162, 178, 178, 102, p, size=12)
    return document(w, h, title, desc, body)


# ---------------------------------------------------------------- pipeline

STAGES = [
    # number, pictogram, title, detail, who does it
    ("01", "capture", "Capture", "frame + dirty rectangles", "accent"),
    ("02", "plan", "Region plan", "committed base + halo", "accent"),
    ("03", "detect", "Detector + OCR", "planned regions only", "accent"),
    ("04", "fuse", "Spatial fusion", "visual + optional UIA", "accent"),
    ("05", "icons", "Icon cache", "curated; may abstain", "accent"),
    ("06", "identity", "Identity", "session IDs + revisions", "accent"),
    ("07", "snapshot", "Atomic snapshot", "publish together", "accent"),
    ("08", "agent", "LLM agent", "reasoning via MCP", "agent"),
    ("09", "gate", "Action gate", "policy + freshness", "accent"),
    ("10", "actuator", "OS actuator", "authorised input", "accent"),
    ("11", "verify", "Effect verifier", "report uncertainty", "accent"),
]


def pipeline(theme: str, layout: str) -> str:
    p = THEMES[theme]
    title = "Ommatix observation and action pipeline"
    desc = ("Eleven numbered stages. The engine captures a frame, plans the changed regions, runs the detector and "
            "OCR on them, fuses the evidence, resolves icons, tracks identity and publishes an atomic snapshot. "
            "The LLM agent, drawn in its own colour, reasons over MCP. The engine then gates the action, delivers "
            "authorised input and verifies the effect; a loop mark after the last stage leads back to a new observation.")
    if layout == "desktop":
        w, bw, bh, gap, top, pitch = DESKTOP, 186, 74, 16, 92, 100
        h = top + 3 * pitch - (pitch - bh) + 24
        body = card(w, h, p) + heading(24, 42, "From a frame to an accountable action",
                                       "Perception stays local. Reasoning stays with the agent.", p, 520)
        for i, (number, pic, name, detail, role) in enumerate(STAGES):
            row, col = divmod(i, 4)
            x, y = 24 + col * (bw + gap), top + row * pitch
            body += rect(x, y, bw, bh, p["surface"], p[role + "_line"] if role == "agent" else p["line"], r=8)
            body += text(x + 14, y + 23, number, SMALL, p[role], 700, mono=True)
            body += icon(pic, x + bw - 14 - 24, y + 10, p, role)
            body += text(x + 14, y + 46, name, LABEL, p["ink"], 600, fit=bw - 28)
            body += text(x + 14, y + 63, detail, SMALL, p["muted"], fit=bw - 28)
            if i == len(STAGES) - 1:
                break
            if col < 3:
                body += arrow(x + bw + 2.5, y + bh / 2, x + bw + gap - 2.5, y + bh / 2, p["accent"], 1.5, 4)
            else:  # the carriage return to the next row
                gutter = y + bh + (pitch - bh) / 2
                body += elbow([(x + bw / 2, y + bh + 2), (x + bw / 2, gutter), (24 + bw / 2, gutter),
                               (24 + bw / 2, y + pitch - 2.5)], p["accent"], 1.5, 4)
        # after the last stage, back to a new observation: a mark, not a sentence
        lx, ly = 24 + 3 * (bw + gap), top + 2 * pitch
        body += arrow(lx - gap + 2.5, ly + bh / 2, lx - 2.5, ly + bh / 2, p["accent"], 1.5, 4)
        body += icon("again", lx + 6, ly + bh / 2 - 15, p, "accent", 1.25)
        return document(w, h, title, desc, body)
    w, rh, top = MOBILE, 50, 118
    h = top + len(STAGES) * rh + 40
    body = card(w, h, p) + text(20, 38, "From a frame to an", 19, p["ink"], 700) + text(20, 62, "accountable action", 19, p["ink"], 700)
    body += text(20, 84, "Perception stays local.", SMALL, p["muted"]) + text(20, 101, "Reasoning stays with the agent.", SMALL, p["muted"])
    body += path(f"M38 {n(top + 20)}V{n(top + len(STAGES) * rh + 4)}", p["line"], 1.5)
    for i, (number, pic, name, detail, role) in enumerate(STAGES):
        y = top + i * rh
        body += f'<circle cx="38" cy="{n(y + 20)}" r="17" fill="{p["surface"]}" stroke="{p[role + "_line"]}"/>'
        body += icon(pic, 26, y + 8, p, role)
        body += text(68, y + 17, number, SMALL, p[role], 700, mono=True)
        body += text(92, y + 17, name, LABEL, p["ink"], 600, fit=250)
        body += text(68, y + 35, detail, SMALL, p["muted"], fit=272)
    body += f'<circle cx="38" cy="{n(top + len(STAGES) * rh + 14)}" r="12" fill="{p["bg"]}"/>'
    body += icon("again", 28, top + len(STAGES) * rh + 4, p, "accent", 20 / 24)
    return document(w, h, title, desc, body)


# ---------------------------------------------------------------- gate

# The order of `Engine::act` and `gate_and_perform` (src/application/src/engine/act.rs).
BEFORE = ("Request", "valid arguments · new request_id")  # checked before the gate; a repeated id replays its result
REFERENCE = ("Reference", "snapshot · target · revision · age")
CHECKS = [
    # pictogram, title, detail
    ("gate", "Policy", "operator allows"),
    ("capture", "Foreground", "allowed app"),
    ("age", "Freshness", "new capture"),  # compared with the committed base; the scope is the operator's (ADR-0035)
    ("again", "Last look", "checked again"),
    ("actuator", "Deliver", "authorised input"),
    ("verify", "Verify", "visual change"),
]
DRY, LIVE = "dry run · default", "live · dry_run: false"


def gate(theme: str, layout: str) -> str:
    p = THEMES[theme]
    title = "Fail-closed action gate"
    desc = ("A request with valid arguments and a new request_id reaches the reference check: snapshot, target, "
            "revision and age. A repeated request_id returns its stored result instead. A dry run, the default, then "
            "returns simulated. Live input must also pass the operator's policy, the allowed foreground application, "
            "a comparison of a new capture with the committed base, and a last look. A failed check is rejected and "
            "nothing is delivered. A delivery reports input_delivered; a verified visual change reports "
            "effect_observed, and an effect that cannot be confirmed reports indeterminate.")
    if layout == "desktop":
        w, h = DESKTOP, 330
        body = card(w, h, p) + text(24, 42, title, TITLE, p["ink"], 700)
        # row A: what comes before the gate, the reference check, the dry-run exit
        y, qw, rwid = 62, 262, 266
        body += rect(24, y, qw, 52, p["surface"], p["agent_line"], r=8) + icon("agent", 36, y + 14, p, "agent")
        body += text(70, y + 22, BEFORE[0], LABEL, p["ink"], 600) + text(70, y + 40, BEFORE[1], SMALL, p["muted"], fit=qw - 56)
        rx = 24 + qw + 18
        body += arrow(24 + qw + 2.5, y + 26, rx - 2.5, y + 26, p["accent"], 1.5, 4)
        body += rect(rx, y, rwid, 52, p["surface"], p["accent_line"], r=8) + icon("identity", rx + 12, y + 14, p)
        body += text(rx + 46, y + 22, REFERENCE[0], LABEL, p["ink"], 600)
        body += text(rx + 46, y + 40, REFERENCE[1], SMALL, p["muted"], fit=rwid - 56)
        dx = rx + rwid
        body += arrow(dx + 4, y + 26, dx + 122, y + 26, p["accent"], 1.5, 4)
        body += text(dx + 62, y + 18, DRY, SMALL, p["accent"], 600, "middle", fit=112)
        sim, sw_ = tag(dx + 128, y + 15, "simulated", p)
        if dx + 128 + sw_ > w - 24:
            raise ValueError("the dry-run exit does not fit")
        body += sim
        # row B: the live path
        y2, bw, gap, bh = y + 52 + 44, 122, 12, 80
        turn = y2 - 14
        body += elbow([(rx + 24, y + 54), (rx + 24, turn), (24 + bw / 2, turn), (24 + bw / 2, y2 - 2.5)], p["warn"], 1.5, 4)
        body += text(rx + 36, turn - 8, LIVE, SMALL, p["warn"], 600, fit=300)
        for i, (pic, name, detail) in enumerate(CHECKS):
            x = 24 + i * (bw + gap)
            body += rect(x, y2, bw, bh, p["surface"], p["accent_line"] if i >= 4 else p["line"], r=8)
            body += icon(pic, x + 12, y2 + 10, p, "accent", 22 / 24)
            body += text(x + 12, y2 + 52, name, LABEL, p["ink"], 600, fit=bw - 24)
            body += text(x + 12, y2 + 69, detail, SMALL, p["muted"], fit=bw - 20)
            if i < len(CHECKS) - 1:
                body += arrow(x + bw + 1.5, y2 + bh / 2, x + bw + gap - 1.5, y2 + bh / 2, p["accent"], 1.5, 3.5)
        # row C: a failed check, and what a delivery and its verification report
        y3 = y2 + bh + 10
        span = 4 * bw + 3 * gap
        body += path(f"M24 {n(y3)}v6h{n(span)}v-6", p["warn"], 1.5)
        rej, rjw = tag(24, y3 + 16, "rejected", p, "warn", 12)
        body += rej + text(24 + rjw + 10, y3 + 31.5, "nothing delivered", SMALL, p["muted"], fit=span - rjw - 10)
        for column, status in ((4, "input_delivered"), (5, "effect_observed")):
            x = 24 + column * (bw + gap)
            chip, cw = tag(x, y3 + 16, status, p, "accent", 12, pad=6)
            if cw > bw:
                raise ValueError(f"{status} is wider than its box")
            body += path(f"M{n(x + 16)} {n(y3 - 8)}v{n(22)}", p["accent"], 1.5) + chip
        # a delivery or a verification that cannot be confirmed: under both boxes, not under one
        ox, ow = 24 + 4 * (bw + gap), 2 * bw + gap
        doubt, dw = tag(0, y3 + 44, "indeterminate", p, "warn", 12, pad=6)
        body += group(doubt, f'transform="translate({n(ox + (ow - dw) / 2)} 0)"')
        return document(w, h, title, desc, body)
    w, rh, top = MOBILE, 46, 230
    h = top + len(CHECKS) * rh + 112
    body = card(w, h, p) + text(20, 38, title, 19, p["ink"], 700, fit=320)
    body += rect(20, 54, 320, 48, p["surface"], p["agent_line"], r=8) + icon("agent", 32, 66, p, "agent")
    body += text(66, 74, BEFORE[0], LABEL, p["ink"], 600) + text(66, 92, BEFORE[1], SMALL, p["muted"], fit=264)
    body += arrow(38, 104, 38, 116, p["accent"], 1.5, 4)
    body += rect(20, 120, 320, 48, p["surface"], p["accent_line"], r=8) + icon("identity", 32, 132, p)
    body += text(66, 140, REFERENCE[0], LABEL, p["ink"], 600) + text(66, 158, REFERENCE[1], SMALL, p["muted"], fit=264)
    # the two exits of the reference check
    body += arrow(38, 170, 38, 184, p["accent"], 1.5, 4)
    sim, sw_ = tag(58, 176, "simulated", p)
    body += sim + text(58 + sw_ + 8, 191.5, DRY, SMALL, p["accent"], 600, fit=320 - 46 - sw_)
    body += text(20, top - 8, LIVE, SMALL, p["warn"], 600, fit=320)
    body += path(f"M38 {n(top + 22)}V{n(top + (len(CHECKS) - 1) * rh + 22)}", p["line"], 1.5)
    for i, (pic, name, detail) in enumerate(CHECKS):
        y = top + 4 + i * rh
        body += f'<circle cx="38" cy="{n(y + 18)}" r="16" fill="{p["surface"]}" stroke="{p["accent_line"] if i >= 4 else p["line"]}"/>'
        body += icon(pic, 27, y + 7, p, "accent", 22 / 24)
        body += text(66, y + 15, name, LABEL, p["ink"], 600) + text(66, y + 32, detail, SMALL, p["muted"])
    by = top + len(CHECKS) * rh + 6
    body += path(f"M20 {n(by)}h320", p["line"], 1)
    rej, rjw = tag(20, by + 12, "rejected", p, "warn", 12)
    body += rej + text(20 + rjw + 8, by + 27.5, "nothing delivered", SMALL, p["muted"], fit=320 - rjw - 8)
    t1, w1 = tag(20, by + 42, "input_delivered", p, size=12)
    t2, _ = tag(20 + w1 + 6, by + 42, "effect_observed", p, size=12)
    t3, _ = tag(20, by + 70, "indeterminate", p, "warn", 12)
    body += t1 + t2 + t3
    return document(w, h, title, desc, body)


# ---------------------------------------------------------------- results

SCORES = [
    # model, phase 1 (dry run), phase 2 (live); out of 11 goals (chapter 09 §9.11.1-3)
    ("Claude Haiku 5.5", 10, 6),
    ("DeepSeek V4.1 Flash*", 11, 7),
    ("MiMo V2.6 Flash", 10, 7),
]
GOALS = 11
NOTES = [
    "One series per model and phase · two synthetic apps · one machine.",
    "Phase 1 scores choices; its decoy-goal verdict is exempt.",
    "* Version attributed to provider documentation; the run records an alias.",
]


def meter(x: float, y: float, total: float, value: int, fill: str, track: str, thick: float = 12, gap: float = 3) -> str:
    """Eleven goals as eleven cells: a ratio whose unit stays visible."""
    cell = (total - (GOALS - 1) * gap) / GOALS
    return "".join(rect(x + i * (cell + gap), y, cell, thick, fill if i < value else track, r=2.5) for i in range(GOALS))


def results(theme: str, layout: str) -> str:
    p = THEMES[theme]
    title = "Preliminary AG-02 scores, not a model ranking"
    desc = ("Goals scored out of 11, correct abstentions included. Phase 1, dry run: Claude Haiku 5.5 10, DeepSeek "
            "V4.1 Flash 11, MiMo V2.6 Flash 10. Phase 2, live: 6, 7 and 7. One series per model and phase; phase 1 "
            "exempts the decoy-goal verdict; no VLM baseline.")
    # The two phases are an ordered pair: one hue, two steps, the live phase the stronger one.
    series = [("Phase 1 · dry run", p["accent_soft"], p["accent_tint"]), ("Phase 2 · live", p["accent"], p["accent_tint"])]
    if layout == "desktop":
        w, top, rh = DESKTOP, 104, 58
        h = top + len(SCORES) * rh + 86
        body = card(w, h, p) + heading(24, 42, "AG-02 · preliminary", "Goals scored out of 11, correct abstentions included.", p, 480)
        lx = w - 24
        for label, fill, _ in reversed(series):
            lw = 18 + width(label, SMALL)
            lx -= lw
            body += rect(lx, 31, 12, 12, fill, r=2.5) + text(lx + 18, 42, label, SMALL, p["muted"])
            lx -= 20
        mx, mw = 262, 484
        for i, (name, dry, live) in enumerate(SCORES):
            y = top + i * rh
            if i:
                body += path(f"M24 {n(y - 12)}h{n(w - 48)}", p["line"], 1)
            body += text(24, y + 21, name, 16, p["ink"], 600, fit=mx - 40)
            for j, (value, (_, fill, track)) in enumerate(zip((dry, live), series)):
                body += meter(mx, y + j * 20, mw, value, fill, track)
                body += text(w - 24, y + 11 + j * 20, f"{value}/{GOALS}", BODY, p["ink"], 600, "end")
        ny = top + len(SCORES) * rh
        body += path(f"M24 {n(ny - 6)}h{n(w - 48)}", p["line"], 1)
        for k, note in enumerate(NOTES):
            body += text(24, ny + 20 + k * 19, note, SMALL, p["muted"], fit=w - 48)
        return document(w, h, title, desc, body)
    w, top, rh = MOBILE, 142, 82
    notes = ["One series per model and phase ·", "two synthetic apps · one machine.",
             "Phase 1 scores choices; its decoy-goal", "verdict is exempt.",
             "* Version attributed to provider", "documentation; the run records an alias."]
    h = top + len(SCORES) * rh + 14 + len(notes) * 18 + 12
    body = card(w, h, p) + text(20, 40, "AG-02 · preliminary", 19, p["ink"], 700)
    body += text(20, 62, "Goals scored out of 11,", SMALL, p["muted"]) + text(20, 79, "correct abstentions included.", SMALL, p["muted"])
    lx = 20
    for label, fill, _ in series:
        body += rect(lx, 97, 12, 12, fill, r=2.5) + text(lx + 18, 108, label, SMALL, p["muted"])
        lx += 18 + width(label, SMALL) + 18
    for i, (name, dry, live) in enumerate(SCORES):
        y = top + i * rh
        body += path(f"M20 {n(y - 14)}h320", p["line"], 1)
        body += text(20, y + 8, name, 16, p["ink"], 600, fit=320)
        for j, (value, (_, fill, track)) in enumerate(zip((dry, live), series)):
            body += meter(20, y + 22 + j * 20, 264, value, fill, track)
            body += text(340, y + 33 + j * 20, f"{value}/{GOALS}", BODY, p["ink"], 600, "end")
    ny = top + len(SCORES) * rh
    body += path(f"M20 {n(ny - 14)}h320", p["line"], 1)
    for k, note in enumerate(notes):
        body += text(20, ny + 8 + k * 18, note, SMALL, p["muted"], fit=320)
    return document(w, h, title, desc, body)


# ---------------------------------------------------------------- social preview

def social() -> str:
    """1280 x 640, dark: the picture GitHub shows when the repository is shared."""
    p = THEMES["dark"]
    body = rect(0, 0, 1280, 640, p["bg"], r=0)
    body += group(mark(52, 44, 15, p), 'transform="translate(40 48)"')
    body += text(152, 112, "Ommatix", 64, p["ink"], 700)
    body += text(64, 250, "A stable interface", 62, p["ink"], 700)
    body += text(64, 326, "to a changing desktop.", 62, p["ink"], 700)
    body += text(64, 392, "Spatial indexing and incremental perception for GUI agents", 25, p["muted"])
    x = 64
    for label, role in (("element IDs, not coordinates", "accent"), ("incremental perception", "accent"),
                        ("fail-closed actuation", "warn")):
        chip, cw = pill(x, 452, label, p, role, size=20, pad=18, h=44)
        body += chip
        x += cw + 14
    body += text(64, 578, "Rust · MCP over stdio · research prototype", 22, p["muted"])
    body += group(mini_screen(0, 0, 168, 138, p) + arrow(178, 69, 204, 69, p["accent"], 2, 5.5) + element_rows(214, 0, 210, 138, p),
                  'transform="translate(760 78) scale(1.12)"')
    return document(1280, 640, "Ommatix social preview",
                    "Ommatix: a stable interface to a changing desktop. A research prototype for incremental desktop "
                    "perception and identifier-based control over MCP.", body)


VARIANTS = [(theme, layout) for theme in ("light", "dark") for layout in ("desktop", "mobile")]


def suffix(theme: str, layout: str) -> str:
    return f"{'mobile-' if layout == 'mobile' else ''}{theme}.svg"
