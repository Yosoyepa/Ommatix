"""The kit's promises: legible colours, labels that fit, short numbers, and one honest timeline."""
import unittest

import kit
from timeline import Scene, Track, eased


class Tokens(unittest.TestCase):
    def test_text_colours_meet_contrast_on_every_surface_they_sit_on(self):
        for theme, p in kit.THEMES.items():
            for role in ("ink", "muted", "accent", "agent", "warn"):
                for surface in ("bg", "surface", "inset"):
                    self.assertGreaterEqual(kit.contrast(p[role], p[surface]), 4.5, f"{theme}: {role} on {surface}")
            for role in ("accent", "agent", "warn"):
                self.assertGreaterEqual(kit.contrast(p[role], p[role + "_tint"]), 4.5, f"{theme}: {role} on its tint")
                self.assertGreaterEqual(kit.contrast(p["on_fill"], p[role]), 4.5, f"{theme}: text on a {role} fill")

    def test_both_themes_define_the_same_roles(self):
        self.assertEqual(set(kit.THEMES["light"]), set(kit.THEMES["dark"]))


class Text(unittest.TestCase):
    def test_a_label_that_would_overflow_is_refused(self):
        with self.assertRaises(kit.FitError):
            kit.text(0, 0, "Every delivery spends the snapshot", 13, fit=120)
        self.assertIn(">Capture<", kit.text(0, 0, "Capture", 15, weight=600, fit=120))

    def test_type_below_the_smallest_size_is_refused(self):
        with self.assertRaises(kit.FitError):
            kit.text(0, 0, "too small", 11)

    def test_width_grows_with_size_weight_and_length(self):
        self.assertLess(kit.width("policy", 13), kit.width("policy", 15))
        self.assertLess(kit.width("policy", 13), kit.width("policy", 13, 600))
        self.assertLess(kit.width("policy", 13), kit.width("policy + freshness", 13))
        self.assertAlmostEqual(kit.width("e7", 12, mono=True), 2 * 12 * kit.MONO_ADVANCE)

    def test_text_is_escaped_and_numbers_are_short(self):
        self.assertIn("&lt;b&gt; &amp; &quot;", kit.text(0, 0, '<b> & "'))
        self.assertEqual([kit.n(v) for v in (1.0, 0.5, 12.3456, -0.0, 0.004)], ["1", "0.5", "12.35", "0", "0"])

    def test_a_document_is_an_accessible_image(self):
        svg = kit.document(10, 10, "A title", "A description", "")
        self.assertIn('role="img"', svg)
        self.assertIn('<title id="title">A title</title><desc id="desc">A description</desc>', svg)


class Timeline(unittest.TestCase):
    def setUp(self):
        self.scene = Scene(10.0, 9.0)

    def test_a_track_spans_the_whole_loop_and_returns_its_keyframes(self):
        track = self.scene.visible((2.0, 6.0), fade=0.5)
        self.scene.add("<rect/>", track)
        self.assertEqual((track.keys[0][0], track.keys[-1][0]), (0.0, 10.0))
        self.assertEqual([track.at(t) for t in (0.0, 2.0, 2.5, 4.0, 6.0, 6.5, 10.0)], [0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0])
        self.assertTrue(0.0 < track.at(6.25) < 1.0)

    def test_css_keyframes_and_frozen_frames_come_from_the_same_keys(self):
        track = Track("move", [(0.0, (8.0, 0.0), "linear"), (5.0, (0.0, 0.0), "linear")])
        self.scene.add("<rect/>", track)
        self.assertEqual(track.keyframes(10.0), "0%{transform:translate(8px,0px)}50%{transform:translate(0px,0px)}"
                                               "100%{transform:translate(0px,0px)}")
        self.assertEqual(self.scene.frame(2.5), '<g transform="translate(4 0)"><rect/></g>')

    def test_without_animation_a_layer_shows_its_value_at_the_poster_time(self):
        self.scene.add("<rect/>", self.scene.visible((1.0, None)))
        self.scene.add("<circle/>", self.scene.visible((1.0, 3.0)))
        body, css = self.scene.animated()
        self.assertIn('<g class="k0" opacity="1"><rect/></g>', body)
        self.assertIn('<g class="k1" opacity="0"><circle/></g>', body)
        self.assertIn("@media (prefers-reduced-motion:no-preference){.k0{animation:k0 10s linear infinite}", css)

    def test_identical_keyframes_are_written_once(self):
        for _ in range(3):
            self.scene.add("<rect/>", self.scene.visible((1.0, 3.0)))
        _, css = self.scene.animated()
        self.assertEqual(css.count("@keyframes"), 1)

    def test_a_keyframe_outside_the_loop_is_an_error(self):
        with self.assertRaises(ValueError):
            self.scene.add("<rect/>", Track("opacity", [(0.0, 0.0, "linear"), (11.0, 1.0, "linear")]))

    def test_easing_is_monotone_and_holds_step_at_the_end(self):
        for name in ("linear", "out", "inout"):
            values = [eased(name, step / 20) for step in range(21)]
            self.assertEqual(values, sorted(values), name)
            self.assertAlmostEqual(values[0], 0.0, places=3)
            self.assertAlmostEqual(values[-1], 1.0, places=3)
        self.assertEqual([eased("hold", p) for p in (0.0, 0.99, 1.0)], [0.0, 0.0, 1.0])

    def test_a_scale_keeps_its_origin_in_both_outputs(self):
        track = Track("scale", [(0.0, (0.0, 1.0), "linear"), (10.0, (1.0, 1.0), "linear")], origin=(24.0, 0.0))
        self.assertEqual(track.attribute(track.at(5.0)), 'transform="translate(24 0) scale(0.5 1) translate(-24 0)"')
        self.assertEqual(track.declaration(track.at(5.0)), "transform:translate(24px,0px) scale(0.5,1) translate(-24px,0px)")


if __name__ == "__main__":
    unittest.main()
