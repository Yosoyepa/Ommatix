"""A small timeline for animated SVG: one model, two outputs.

A scene is a list of layers. Each layer is SVG markup plus tracks, and a track is the keyframes
of one property (opacity, an offset, a scale about a point). From the same tracks this module
writes

* CSS `@keyframes`, so a browser plays the scene inside an `<img>` with no script, and
* the scene frozen at any time, as plain attributes, for stills, contact sheets and the GIF.

Every animation lasts the whole loop and starts at zero, so the two outputs cannot drift apart.
The value a layer has at the poster time is also written as its plain attribute: a viewer that
plays no animation (reduced motion, or a renderer without CSS animation) shows the poster.
"""
from __future__ import annotations

from dataclasses import dataclass, field

from kit import n

EASE = {  # name: (CSS timing function, control points of its cubic Bézier or None)
    "linear": ("linear", None),
    "out": ("cubic-bezier(.2,.7,.2,1)", (0.2, 0.7, 0.2, 1.0)),
    "inout": ("cubic-bezier(.45,0,.2,1)", (0.45, 0.0, 0.2, 1.0)),
    "hold": ("step-end", None),
}


def eased(name: str, progress: float) -> float:
    """The eased progress of a segment, as a browser computes it."""
    if name == "hold":
        return 0.0 if progress < 1 else 1.0
    points = EASE[name][1]
    if points is None:
        return progress
    x1, y1, x2, y2 = points

    def bezier(a: float, b: float, u: float) -> float:
        return 3 * (1 - u) ** 2 * u * a + 3 * (1 - u) * u * u * b + u ** 3

    lo, hi = 0.0, 1.0
    for _ in range(40):
        mid = (lo + hi) / 2
        if bezier(x1, x2, mid) < progress:
            lo = mid
        else:
            hi = mid
    return bezier(y1, y2, (lo + hi) / 2)


@dataclass
class Track:
    """Keyframes `(time, value, ease)` of one property. `ease` shapes the segment that follows."""
    kind: str  # "opacity", "move" (dx, dy) or "scale" (sx, sy) about `origin`
    keys: list[tuple[float, object, str]]
    origin: tuple[float, float] = (0.0, 0.0)

    def at(self, time: float):
        keys = self.keys
        if time <= keys[0][0]:
            return keys[0][1]
        for (t0, v0, ease), (t1, v1, _) in zip(keys, keys[1:]):
            if time <= t1:
                p = eased(ease, (time - t0) / (t1 - t0)) if t1 > t0 else 1.0
                if isinstance(v0, tuple):
                    return tuple(a + (b - a) * p for a, b in zip(v0, v1))
                return v0 + (v1 - v0) * p
        return keys[-1][1]

    def attribute(self, value) -> str:
        if self.kind == "opacity":
            return f'opacity="{n(value)}"'
        if self.kind == "move":
            return f'transform="translate({n(value[0])} {n(value[1])})"'
        ox, oy = self.origin
        return f'transform="translate({n(ox)} {n(oy)}) scale({n(value[0])} {n(value[1])}) translate({n(-ox)} {n(-oy)})"'

    def declaration(self, value) -> str:
        if self.kind == "opacity":
            return f"opacity:{n(value)}"
        if self.kind == "move":
            return f"transform:translate({n(value[0])}px,{n(value[1])}px)"
        ox, oy = self.origin
        return (f"transform:translate({n(ox)}px,{n(oy)}px) scale({n(value[0])},{n(value[1])}) "
                f"translate({n(-ox)}px,{n(-oy)}px)")

    def keyframes(self, duration: float) -> str:
        """The body of the `@keyframes` rule. Times become percentages of the loop."""
        steps = []
        for index, (time, value, ease) in enumerate(self.keys):
            css = self.declaration(value)
            if ease != "linear" and index < len(self.keys) - 1:
                css += f";animation-timing-function:{EASE[ease][0]}"
            steps.append(f"{n(time / duration * 100)}%{{{css}}}")
        return "".join(steps)


@dataclass
class Layer:
    body: str
    tracks: list[Track] = field(default_factory=list)


class Scene:
    def __init__(self, duration: float, poster: float):
        self.duration, self.poster, self.layers = duration, poster, []

    def add(self, body: str, *tracks: Track) -> None:
        for track in tracks:
            keys = sorted(track.keys, key=lambda k: k[0])
            if keys[0][0] > 0:
                keys.insert(0, (0.0, keys[0][1], "linear"))
            if keys[-1][0] < self.duration:
                keys.append((self.duration, keys[-1][1], "linear"))
            if keys[0][0] < 0 or keys[-1][0] > self.duration:
                raise ValueError("a keyframe lies outside the loop")
            track.keys = keys
        self.layers.append(Layer(body, list(tracks)))

    # -- helpers that build tracks -------------------------------------------------------

    def visible(self, *windows: tuple[float, float | None], fade: float = 0.25) -> Track:
        """Opaque inside each `(start, end)` window; an open end lasts until the loop restarts."""
        keys: list[tuple[float, object, str]] = []
        for start, end in windows:
            end = self.duration - 0.35 if end is None else end
            if start <= 0:
                keys.append((0.0, 1.0, "linear"))
            else:
                keys += [(start, 0.0, "out"), (min(start + fade, end), 1.0, "linear")]
            keys += [(end, 1.0, "linear"), (min(end + fade, self.duration), 0.0, "linear")]
        if keys[0][0] > 0:
            keys.insert(0, (0.0, 0.0, "linear"))
        return Track("opacity", keys)

    def slide(self, start: float, dx: float, dy: float, span: float = 0.45) -> Track:
        """Arrive from an offset, easing out."""
        return Track("move", [(0.0, (dx, dy), "linear"), (start, (dx, dy), "out"), (start + span, (0.0, 0.0), "linear")])

    def pop(self, start: float, origin: tuple[float, float], scale: float = 1.18, span: float = 0.35) -> Track:
        return Track("scale", [(0.0, (scale, scale), "linear"), (start, (scale, scale), "out"),
                               (start + span, (1.0, 1.0), "linear")], origin)

    def grow(self, start: float, end: float, x: float, hold_until: float | None = None) -> Track:
        """Fill from the left edge `x` between two times, then stay full until the loop restarts."""
        last = self.duration - 0.35 if hold_until is None else hold_until
        return Track("scale", [(0.0, (0.0, 1.0), "linear"), (start, (0.0, 1.0), "linear"), (end, (1.0, 1.0), "linear"),
                               (last, (1.0, 1.0), "hold"), (min(last + 0.01, self.duration), (0.0, 1.0), "linear")], (x, 0.0))

    def travel(self, points: list[tuple[float, float, float]]) -> Track:
        """Move through `(time, x, y)` points, relative to the first one."""
        _, x0, y0 = points[0]
        return Track("move", [(t, (x - x0, y - y0), "inout") for t, x, y in points])

    # -- the two outputs -----------------------------------------------------------------

    def frame(self, time: float) -> str:
        """The scene frozen at `time`: plain attributes, nothing a renderer has to animate."""
        out = ""
        for layer in self.layers:
            body = layer.body
            for track in reversed(layer.tracks):
                body = f"<g {track.attribute(track.at(time))}>{body}</g>"
            out += body
        return out

    def animated(self) -> tuple[str, str]:
        """Markup and CSS. Without the animation each layer shows its value at the poster time."""
        rules: dict[str, str] = {}
        out = ""
        for layer in self.layers:
            body = layer.body
            for track in reversed(layer.tracks):
                frames = track.keyframes(self.duration)
                name = rules.setdefault(frames, f"k{len(rules)}")
                body = f'<g class="{name}" {track.attribute(track.at(self.poster))}>{body}</g>'
            out += body
        css = "".join(f"@keyframes {name}{{{frames}}}" for frames, name in rules.items())
        css += "@media (prefers-reduced-motion:no-preference){" + "".join(
            f".{name}{{animation:{name} {n(self.duration)}s linear infinite}}" for name in rules.values()) + "}"
        return out, css
