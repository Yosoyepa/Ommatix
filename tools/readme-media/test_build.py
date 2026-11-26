"""The generator is deterministic, says only what its sources say, and the committed files are its output."""
import json
import re
import unittest
import xml.etree.ElementTree as ET

import build
import demo
import figures


class Generator(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.assets = build.vector_assets()

    def test_the_same_call_writes_the_same_bytes(self):
        self.assertEqual(self.assets, build.vector_assets())

    def test_every_figure_has_its_four_variants_and_is_an_accessible_image(self):
        self.assertEqual(len(self.assets), 6 + 5 * 4)
        for name, (_, text) in self.assets.items():
            root = ET.fromstring(text)
            self.assertEqual(root.attrib["role"], "img", name)
            self.assertTrue(text.endswith("</svg>\n"), name)
            self.assertNotRegex(text, r"<script|<foreignObject|<image|https?://(?!www\.w3\.org/2000/svg)", name)

    def test_no_label_is_set_below_twelve_units_outside_the_badges(self):
        for name, (_, text) in self.assets.items():
            sizes = {float(size) for size in re.findall(r'font-size="([\d.]+)"', text)}
            self.assertGreaterEqual(min(sizes), 11 if name.startswith("badge-") else 12, name)

    def test_the_committed_files_are_what_the_generator_writes(self):
        for name, (_, text) in self.assets.items():
            self.assertEqual((build.ASSETS / name).read_bytes(), text.encode("utf-8"), f"{name}: run build.py")
        self.assertEqual(build.SOCIAL_SOURCE.read_bytes(), figures.social().encode("utf-8"))
        manifest = json.loads((build.ASSETS / "manifest.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest, build.manifest(self.assets, manifest))


class Facts(unittest.TestCase):
    def test_the_demo_reads_its_identifiers_and_results_from_the_contract_examples(self):
        data = demo.examples(build.ROOT)
        lines = " ".join(line for _, line, _, _ in demo.log_lines(data, mobile=False))
        self.assertEqual((data["button"]["target_id"], data["button"]["revision"], data["button"]["label"]), ("e7", 3, "Guardar ejemplo"))
        for expected in ("snapshot s16 · 2 elements", 'e7 button "Guardar ejemplo" rev 3', "rejected · STALE_SNAPSHOT",
                         "next: observe_again", "snapshot s17", "simulated · input_delivered: false"):
            self.assertIn(expected, lines)
        self.assertNotIn("verified: true", lines)

    def test_the_exchange_is_observe_refuse_observe_again_then_simulate(self):
        for mobile in (False, True):
            lines = sorted(demo.log_lines(demo.examples(build.ROOT), mobile))
            order = [next(word for word in ("observe_screen", "rejected", "simulated", "execute_action", "")
                          if word in line) for _, line, _, _ in lines]
            self.assertEqual([word for word in order if word],
                             ["observe_screen", "execute_action", "rejected", "observe_screen", "execute_action", "simulated"])
            beats = {label: (start, end) for start, end, label, *_ in demo.BEATS}
            refused = next(at for at, line, _, _ in lines if "rejected" in line)
            simulated = next(at for at, line, _, _ in lines if "simulated" in line)
            self.assertTrue(beats["Refuse"][0] <= refused < beats["Refuse"][1])
            self.assertTrue(beats["Act"][0] <= simulated < beats["Act"][1])

    def test_the_poster_holds_the_whole_exchange_and_says_it_is_simulated(self):
        still = demo.demo("light", "desktop", build.ROOT, at=demo.POSTER)
        shown = re.sub(r'<g opacity="0">.*?</g>', "", still)
        for expected in ("Simulated desktop", "STALE_SNAPSHOT", "simulated · input_delivered: false", "e7 · rev 3", "s17"):
            self.assertIn(expected, shown)
        self.assertLess(demo.POSTER, demo.DURATION)
        self.assertEqual(demo.BEATS[-1][1], 22.0)

    def test_the_scores_are_those_of_chapter_09(self):
        chapter = (build.ROOT / "docs/thesis/09_EVALUACION_EXPERIMENTAL.md").read_text(encoding="utf-8")
        self.assertEqual([(dry, live) for _, dry, live in figures.SCORES], [(10, 6), (11, 7), (10, 7)])
        self.assertIn("| Éxitos de la fase 1 (implementada / estricta) | 10 de 11 / 9 de 11 | 10 de 11 / 9 de 11 | 11 de 11 / 10 de 11 |", chapter)
        self.assertIn("| Éxito (puntuación de la fase 2) | 6 de 11 | 7 de 11 | 7 de 11 |", chapter)

    def test_the_gate_names_every_status_of_the_contract(self):
        gate = figures.gate("light", "desktop")
        schema = json.loads((build.ROOT / "docs/thesis/contratos/esquemas/execute_action.output.schema.json").read_text(encoding="utf-8"))
        statuses = schema["properties"]["status"]["enum"]
        for status in statuses:
            self.assertIn(f">{status}<", gate)

    def test_the_rust_badge_follows_the_pinned_toolchain(self):
        channel = re.search(r'channel = "([^"]+)"', (build.ROOT / "rust-toolchain.toml").read_text(encoding="utf-8")).group(1)
        self.assertIn(f">{channel}<", self.badge("badge-rust.svg"))

    def badge(self, name: str) -> str:
        return build.vector_assets()[name][1]


if __name__ == "__main__":
    unittest.main()
