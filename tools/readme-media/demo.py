"""The protocol demo: one observe -> act round trip on a simulated desktop, as an animated SVG.

An illustration, not a recording. The window is drawn here; the element labels, identifiers,
revisions, snapshot names and results are read from the dossier's contract examples
(docs/thesis/contratos/ejemplos). The refusal shown is the age bound of ADR-0025: a dry run
ends before any capture, so its STALE_SNAPSHOT comes from the snapshot's age, never from pixels.

The loop ends on a summary that holds every exchange; that frame is the poster a reader with
reduced motion sees.
"""
from __future__ import annotations

import json
from pathlib import Path

from kit import (DESKTOP, MOBILE, SMALL, THEMES, card, document, icon, mark, n, path, rect, tag, text, width)
from timeline import Scene, Track

DURATION = 24.0
POSTER = 23.0
BEATS = [  # start, end, label, caption (desktop), caption (mobile, two lines)
    (0.0, 4.6, "Observe", "Only the regions that changed are analysed again.",
     ("Only the regions that changed", "are analysed again.")),
    (4.6, 8.6, "Identify", "Each control gets a session ID and a revision.",
     ("Each control gets a session ID", "and a revision.")),
    (8.6, 12.6, "Read", "The agent reads elements: no pixels, no coordinates.",
     ("The agent reads elements:", "no pixels, no coordinates.")),
    (12.6, 17.4, "Refuse", "An observation older than the limit is refused. No input is sent.",
     ("An observation older than the limit", "is refused. No input is sent.")),
    (17.4, 22.0, "Act", "Observe again, then act: the dry run reports what happened.",
     ("Observe again, then act: the dry", "run reports what happened.")),
]
SUMMARY = ("By ID and revision: a stale reference is refused, a dry run sends nothing.",
           ("By ID and revision: a stale reference", "is refused, a dry run sends nothing."))


def examples(root: Path) -> dict:
    """The fields of the contract examples the scene shows; nothing else is invented."""
    def read(name: str) -> dict:
        return json.loads((root / "docs/thesis/contratos/ejemplos" / name).read_text(encoding="utf-8"))
    observe, click, stale, simulated = (read(f"{name}.json") for name in
                                        ("observe_output", "click_input", "stale_output", "simulated_output"))
    by_id = {element["target_id"]: element for element in observe["elements"]}
    button, label = by_id[click["target_id"]], next(e for e in observe["elements"] if e["target_id"] != click["target_id"])
    if button["revision"] != click["expected_revision"] or simulated["snapshot_id"] != click["snapshot_id"]:
        raise ValueError("the contract examples no longer describe one round trip")
    return {"button": button, "label": label, "click": click, "stale": stale, "simulated": simulated,
            "elements": len(observe["elements"])}


def log_lines(d: dict, mobile: bool) -> list[tuple[float, str, str, bool]]:
    """Time, text, colour role and whether it opens an exchange."""
    b, t, click, stale, sim = d["button"], d["label"], d["click"], d["stale"], d["simulated"]
    old, new = stale["snapshot_id"], click["snapshot_id"]
    act = "→ execute_action {id} rev {rev} {snap} dry_run" if mobile else "→ execute_action {id} · rev {rev} · {snap} · dry_run"
    second = f'  {t["target_id"]} {t["role"]} rev {t["revision"]}' if mobile else \
        f'  {t["target_id"]} {t["role"]} "{t["label"]}" rev {t["revision"]}'
    return [
        (8.9, "→ observe_screen", "agent", True),
        (9.5, f'← snapshot {old} · {d["elements"]} elements', "ink", False),
        (9.8, f'  {b["target_id"]} {b["role"]} "{b["label"]}" rev {b["revision"]}', "muted", False),
        (10.1, second, "muted", False),
        (14.6, act.format(id=stale["target_id"], rev=b["revision"], snap=old), "agent", True),
        (15.9, f'← {stale["status"]} · {stale["error"]["code"]}', "warn", False),
        (16.2, f'  no input · next: {stale["error"]["next_step"]}', "muted", False),
        (17.6, "→ observe_screen", "agent", True),
        (18.6, f'← snapshot {new} · {b["target_id"]} rev {b["revision"]}', "ink", False),
        (19.2, act.format(id=click["target_id"], rev=click["expected_revision"], snap=new), "agent", True),
        (20.6, f'← {sim["status"]} · input_delivered: {str(sim["input_delivered"]).lower()}', "accent", False),
    ]


def scene(theme: str, layout: str, d: dict) -> tuple[Scene, float, float, str, str]:
    """Builds the layers. Returns the scene, its size, and the title and description."""
    p = THEMES[theme]
    mobile = layout == "mobile"
    s = Scene(DURATION, POSTER)
    b, t, old, new = d["button"], d["label"], d["stale"]["snapshot_id"], d["click"]["snapshot_id"]
    if mobile:
        W, wx, wy, ww, bar, cell, rows = MOBILE, 20, 40, 320, 28, 32, 5
        px, py, pw, lh, mono = 20, 266, 320, 17, 12
        H = 626
    else:
        W, wx, wy, ww, bar, cell, rows = DESKTOP, 24, 50, 360, 32, 36, 7
        px, py, pw, lh, mono = 440, 50, 376, 18, 12.5
        H = 454
    cx, cy, ch = wx, wy + bar, rows * cell
    wh = bar + ch
    ph = wh if not mobile else 256

    # ---- what never moves: the card, the window, the agent's panel
    still = card(W, H, p)
    still += mark(wx + (12 if not mobile else 9), wy - (20 if not mobile else 16), 4.2 if not mobile else 3.8, p)
    still += text(wx + (30 if not mobile else 24), wy - (15.5 if not mobile else 11.5),
                  "Simulated desktop · illustration, not a screen recording" if not mobile else
                  "Simulated desktop · illustration, not a recording", SMALL if not mobile else 12, p["muted"],
                  fit=W - 2 * wx - 30)
    still += rect(wx, wy, ww, wh, p["surface"], p["line"], r=8) + path(f"M{n(wx)} {n(cy)}h{n(ww)}", p["line"], 1)
    still += "".join(f'<circle cx="{n(wx + ww - 16 - i * 12)}" cy="{n(wy + bar / 2)}" r="3" fill="{p["line"]}"/>' for i in range(3))
    title_size = 13 if not mobile else 12.5
    still += text(wx + 14, wy + bar / 2 + 4.5, t["label"], title_size, p["ink"], 600)
    still += text(cx + (20 if not mobile else 14), cy + (26 if not mobile else 22), "Documento", 16 if not mobile else 14, p["ink"], 700)
    fx, fy, fw, fh = cx + (19 if not mobile else 13), cy + cell + (6 if not mobile else 4), cell * 6.5 - 0, cell - (12 if not mobile else 8)
    still += rect(fx, fy, fw, fh, p["bg"], p["line"], r=5)
    for lw, dy in [(0.5, 0.42), (0.36, 0.8), (0.44, 1.32), (0.5, 2.5), (0.3, 2.88)][:2 if mobile else 5]:
        still += rect(cx + (20 if not mobile else 14), cy + 2 * cell + dy * cell - (0 if not mobile else 2), ww * lw,
                      6 if not mobile else 5, p["inset"], r=3)
    side_x, side_w = cx + 7 * cell + (10 if not mobile else 8), ww - 7 * cell - (26 if not mobile else 20)
    if not mobile:  # a phone's window has no room for it beside the tag of the button
        still += rect(side_x, cy + 2 * cell + 6, side_w, cell * 1.6, p["bg"], p["line"], r=5)
        still += rect(side_x + 9, cy + 2 * cell + 16, side_w - 18, cell * 0.62, p["inset"], r=3)
        still += rect(side_x + 9, cy + 2 * cell + 24 + cell * 0.62, side_w * 0.5, 5, p["inset"], r=2.5)
    bw, bh = (136, 34) if not mobile else (118, 30)
    bx, by = cx + ww - (20 if not mobile else 14) - bw, cy + ch - (16 if not mobile else 12) - bh
    still += rect(bx, by, bw, bh, p["accent_tint"], p["accent_line"], r=6)
    still += text(bx + bw / 2, by + bh / 2 + 4.5, b["label"], 13 if not mobile else 12.5, p["accent"], 600, "middle", fit=bw - 12)
    still += rect(px, py, pw, ph, p["surface"], p["agent_line"], r=8) + path(f"M{n(px)} {n(py + bar)}h{n(pw)}", p["line"], 1)
    still += icon("agent", px + 12, py + bar / 2 - 9, p, "agent", 0.75)
    still += text(px + 36, py + bar / 2 + 4.5, "LLM agent · MCP client", title_size, p["ink"], 600)
    s.add(still)

    # ---- the field's text: empty, typed, then edited
    typed, edited = "Synthetic document", "Synthetic document v2"
    ty = fy + fh / 2 + 4.5
    s.add(text(fx + 10, ty, typed, 13 if not mobile else 12.5, p["ink"]), s.visible((0.9, 13.4), fade=0.2))
    s.add(text(fx + 10, ty, edited, 13 if not mobile else 12.5, p["ink"]), s.visible((13.4, None), fade=0.2))

    # ---- tiles: the grid, and the row that changed (first observation, then the second)
    grid = "".join(f"M{n(cx + i * cell)} {n(cy)}v{n(ch)}" for i in range(1, round(ww / cell)))
    grid += "".join(f"M{n(cx)} {n(cy + j * cell)}h{n(ww)}" for j in range(1, rows))
    changed = "".join(rect(cx + i * cell + 1.5, cy + cell + 1.5, cell - 3, cell - 3, p["warn_tint"], p["warn_line"], r=3,
                           extra='fill-opacity="0.72"') for i in range(7))
    s.add(path(grid, p["rule"], 0.75), s.visible((1.5, 4.5), (17.8, 19.0), fade=0.35))
    s.add(changed, s.visible((2.1, 4.5), (18.0, 19.0), fade=0.3))
    note, nw = tag(cx + 7 * cell + 8, cy + cell + cell / 2 - 11, "changed", p, "warn", 12)
    s.add(note, s.visible((2.5, 4.5), (18.1, 19.0)), s.slide(2.5, -6, 0))

    # ---- identity: an outline and a tag for each control
    tw = width(t["label"], title_size, 600)
    label_box = rect(wx + 8, wy + bar / 2 - 10, tw + 12, 20, "none", p["accent"], r=4, sw=1.5)
    label_tag, _ = tag(wx + 8 + tw + 12 + 6, wy + bar / 2 - 11, f'{t["target_id"]} · rev {t["revision"]}', p, size=12)
    button_box = rect(bx - 4, by - 4, bw + 8, bh + 8, "none", p["accent"], r=8, sw=1.5)
    button_tag, _ = tag(bx - 4, by - 31, f'{b["target_id"]} · rev {b["revision"]}', p, size=12)
    s.add(label_box + label_tag, s.visible((5.0, None)), s.slide(5.0, 0, 4))
    s.add(button_box + button_tag, s.visible((5.7, None)), s.slide(5.7, 0, 4))

    # ---- the snapshot the agent holds, and its age against the bound of ADR-0025
    kx, ky = cx + (16 if not mobile else 12), by + bh / 2 - 11
    old_tag, kw = tag(kx, ky, old, p, "accent", 12)
    mw = 54 if not mobile else 46
    meter_x, meter_y = kx + kw + 8, ky + 8
    s.add(old_tag + rect(meter_x, meter_y, mw, 6, p["inset"], r=3), s.visible((9.5, 18.3)))
    s.add(rect(meter_x, meter_y, mw, 6, p["accent"], r=3), s.visible((9.5, 14.3)), s.grow(9.5, 14.3, meter_x, 14.5))
    s.add(rect(meter_x, meter_y, mw, 6, p["warn"], r=3) + rect(kx, ky, kw, 22, p["warn_tint"], p["warn_line"], r=4) +
          text(kx + kw / 2, ky + 15.5, old, 12, p["warn"], 600, "middle", True), s.visible((14.3, 18.3), fade=0.15))
    limit = "age" if mobile else "age"
    s.add(text(meter_x + mw + 7, ky + 15.5, limit, 12, p["muted"]), s.visible((9.5, 14.3)))
    s.add(text(meter_x + mw + 7, ky + 15.5, "over 10 s", 12, p["warn"], 600), s.visible((14.3, 18.3), fade=0.15))
    new_tag, nkw = tag(kx, ky, new, p, "accent", 12)
    s.add(new_tag + text(kx + nkw + 8, ky + 15.5, "new", 12, p["accent"], 600), s.visible((18.4, None)), s.slide(18.4, 0, 4))

    # ---- the agent's side of the exchange
    lx, ly, gap = px + 14, py + bar + (22 if not mobile else 20), 0.0
    line_y: dict[float, float] = {}
    for at, value, role, opens in log_lines(d, mobile):
        if opens and line_y:
            gap += 6
        y = ly + len(line_y) * lh + gap
        line_y[at] = y
        indent = (len(value) - len(value.lstrip())) * mono * 0.61
        s.add(text(lx + indent, y, value.lstrip(), mono, p[role], 600 if opens or role in ("warn", "accent") else 400,
                   mono=True, fit=pw - 24 - indent), s.visible((at, None)), s.slide(at, 8, 0, 0.35))

    # ---- the gate, the request that reaches it, and what happens next
    if mobile:
        gx, gy = bx + bw / 2, (wy + wh + py) / 2
        refused = [(14.8, gx, line_y[14.6] - 4), (15.6, gx, gy)]
        allowed = [(19.4, gx, line_y[19.2] - 4), (20.1, gx, gy), (20.5, gx, by + bh + 6)]
        boundary = path(f"M{n(wx)} {n(gy)}H{n(wx + ww)}", p["rule"], 1, extra='stroke-dasharray="3 4"')
        boundary += text(gx - 22, gy + 4.5, "gate", 12, p["muted"], anchor="end")
    else:
        gx, gy = (wx + ww + px) / 2, by + bh / 2
        refused = [(14.8, px + 4, line_y[14.6] - 4), (15.1, gx, line_y[14.6] - 4), (15.6, gx, gy)]
        allowed = [(19.4, px + 4, line_y[19.2] - 4), (19.7, gx, line_y[19.2] - 4), (20.1, gx, gy), (20.5, bx + bw + 8, gy)]
        boundary = path(f"M{n(gx)} {n(wy)}V{n(wy + wh)}", p["rule"], 1, extra='stroke-dasharray="3 4"')
        boundary += text(gx, gy + 32, "gate", 12, p["muted"], anchor="middle")
    shield = f'<circle cx="{n(gx)}" cy="{n(gy)}" r="16" fill="{p["bg"]}"/>'
    s.add(boundary + shield + icon("gate", gx - 12, gy - 12, p, "accent"))
    s.add(shield + icon("stop", gx - 12, gy - 12, p, "warn"), s.visible((15.6, 17.2), fade=0.15))

    def dot(role: str) -> str:
        x, y = refused[0][1], refused[0][2]
        return (f'<circle cx="{n(x)}" cy="{n(y)}" r="9" fill="{p[role]}" opacity="0.25"/>'
                f'<circle cx="{n(x)}" cy="{n(y)}" r="4.5" fill="{p[role]}"/>')
    s.add(dot("agent"), Track("opacity", [(14.7, 0.0, "linear"), (14.8, 1.0, "linear"), (15.6, 1.0, "linear"), (15.85, 0.0, "linear")]),
          s.travel(refused))
    shift = (allowed[0][1] - refused[0][1], allowed[0][2] - refused[0][2])
    s.add(f'<g transform="translate({n(shift[0])} {n(shift[1])})">{dot("agent")}</g>',
          Track("opacity", [(19.3, 0.0, "linear"), (19.4, 1.0, "linear"), (20.5, 1.0, "linear"), (20.8, 0.0, "linear")]),
          s.travel(allowed))

    # ---- the refusal, over the window
    sw_, sh = (268, 62) if not mobile else (236, 56)
    sx, sy = cx + (ww - sw_) / 2, (cy + (ch - sh) / 2 - 10) if not mobile else cy + 24  # clear of the button's tag
    code = d["stale"]["error"]["code"]
    stamp = rect(sx, sy, sw_, sh, p["warn_tint"], p["warn"], r=8, sw=1.5)
    stamp += text(sx + sw_ / 2, sy + 27, code, 17 if not mobile else 16, p["warn"], 700, "middle", True, fit=sw_ - 20)
    stamp += text(sx + sw_ / 2, sy + 47, f"{old} is over the 10 s limit · no input sent", 12, p["warn"], anchor="middle", fit=sw_ - 16)
    s.add(stamp, s.visible((15.7, 17.2), fade=0.15), s.pop(15.7, (sx + sw_ / 2, sy + sh / 2)))

    # ---- the dry run on the fresh snapshot: a press that is reported, not delivered
    ring = rect(bx - 9, by - 9, bw + 18, bh + 18, "none", p["accent"], r=12, sw=1.5, extra='stroke-dasharray="5 4"')
    s.add(ring, s.visible((20.5, 21.6), fade=0.2), s.pop(20.5, (bx + bw / 2, by + bh / 2), 0.92, 0.5))

    # ---- captions and progress
    seg_gap = 6
    seg_w = (W - 2 * wx - (len(BEATS) - 1) * seg_gap) / len(BEATS)
    cap_y = (wy + wh + 40) if not mobile else (py + ph + 26)
    bar_y = cap_y + (26 if not mobile else 40)
    for i, (start, end, label, caption, caption_mobile) in enumerate(BEATS):
        lines = caption_mobile if mobile else (caption,)
        body = "".join(text(wx, cap_y + k * 20, line, 17 if not mobile else 15, p["ink"], 600, fit=W - 2 * wx) for k, line in enumerate(lines))
        s.add(body, s.visible((max(start, 0.01) + 0.05, end - 0.2), fade=0.2))
        x = wx + i * (seg_w + seg_gap)
        s.add(rect(x, bar_y, seg_w, 5, p["inset"], r=2.5) +
              text(x, bar_y + 22, label if mobile else f"{i + 1} · {label}", 12, p["muted"], fit=seg_w))
        s.add(rect(x, bar_y, seg_w, 5, p["accent"], r=2.5), s.grow(max(start, 0.01), end, x))
    lines = SUMMARY[1] if mobile else (SUMMARY[0],)
    s.add("".join(text(wx, cap_y + k * 20, line, 17 if not mobile else 15, p["ink"], 600, fit=W - 2 * wx) for k, line in enumerate(lines)),
          s.visible((22.0, None), fade=0.2))

    title = "Ommatix protocol demo: observe, refuse a stale reference, observe again, act"
    desc = (f'On a simulated desktop, only the changed tiles are analysed; the controls get identifiers and revisions '
            f'({b["target_id"]} revision {b["revision"]}, {t["target_id"]} revision {t["revision"]}). The agent calls '
            f'observe_screen and execute_action. A dry-run action on snapshot {old}, older than the 10-second limit, is '
            f'{d["stale"]["status"]} with {code} and no input. A new observation publishes {new}, and the dry-run action '
            f'on it returns {d["simulated"]["status"]} with input_delivered false.')
    return s, W, H, title, desc


def demo(theme: str, layout: str, root: Path, at: float | None = None) -> str:
    """The animated SVG, or the scene frozen at `at` seconds."""
    s, w, h, title, desc = scene(theme, layout, examples(root))
    if at is not None:
        return document(w, h, title, desc, s.frame(at))
    body, css = s.animated()
    return document(w, h, title, desc, body, css)
