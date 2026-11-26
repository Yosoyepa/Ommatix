"""Boundary cases of the offline documentation checker."""
import hashlib
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check

PICTURE = ('<picture><source media="(max-width: 640px) and (prefers-color-scheme: dark)" srcset="a/hero-mobile-dark.svg">'
           '<source media="(max-width: 640px)" srcset="a/hero-mobile-light.svg">'
           '<source media="(prefers-color-scheme: dark)" srcset="a/hero-dark.svg">'
           '<img src="a/hero-light.svg" alt="A stable interface to a changing desktop" width="1200"></picture>')
ANIMATED = ('<svg xmlns="http://www.w3.org/2000/svg" role="img"><title>t</title><desc>d</desc><style>'
            "@keyframes k0{0%{opacity:0}100%{opacity:1}}"
            "@media (prefers-reduced-motion:no-preference){.k0{animation:k0 24s linear infinite}}</style>"
            '<text>Simulated desktop</text><g class="k0" opacity="1"><rect/></g></svg>')


class LinkChecks(unittest.TestCase):
    def test_fenced_examples_do_not_create_links_or_headings(self):
        source = "# Real\n\n```markdown\n# Fake\n[missing](absent.md)\n```\n[ok](present.md)"
        self.assertEqual(check.destinations(source), ["present.md"])
        self.assertEqual(check.anchors(source), {"real"})

    def test_picture_sources_and_nested_poster_links_are_checked(self):
        source = '[![poster](poster.png)](movie.mp4)<source srcset="dark.svg"><img src="light.svg">'
        self.assertEqual(check.destinations(source), ["poster.png", "movie.mp4", "light.svg", "dark.svg"])

    def test_unicode_headings_and_duplicate_anchors(self):
        self.assertEqual(check.anchors("# Cómo funciona\n## Configuración\n## Configuración"), {"cómo-funciona", "configuración", "configuración-1"})

    def test_missing_file_and_anchor_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "page.md").write_text("# Present", encoding="utf-8")
            source = root / "README.md"
            source.write_text("[gone](missing.md) [bad](page.md#absent) [ok](page.md#present)", encoding="utf-8")
            with patch.object(check, "ROOT", root):
                self.assertEqual(len(check.check_links(source)), 2)

    def test_svg_rejects_script_and_external_resource(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "bad.svg"
            file.write_text('<svg role="img"><title>Title</title><desc>Description</desc><script/><use href="https://invalid.example/a.svg"/></svg>', encoding="utf-8")
            self.assertEqual(len(check.check_svg(file)), 2)


class AnimationChecks(unittest.TestCase):
    def errors(self, svg: str) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "demo.svg"
            file.write_text(svg, encoding="utf-8")
            return check.check_animation(file)

    def test_a_guarded_css_animation_with_a_still_passes(self):
        self.assertEqual(self.errors(ANIMATED), [])

    def test_an_animation_outside_the_reduced_motion_guard_fails(self):
        bare = ANIMATED.replace("@media (prefers-reduced-motion:no-preference){.k0{animation:k0 24s linear infinite}}",
                                ".k0{animation:k0 24s linear infinite}")
        self.assertTrue(any("guard" in error for error in self.errors(bare)))

    def test_a_group_without_a_value_for_the_still_fails(self):
        self.assertTrue(any("still" in error for error in self.errors(ANIMATED.replace(' opacity="1"', ""))))

    def test_smil_a_long_loop_and_a_missing_label_fail(self):
        self.assertTrue(any("SMIL" in error for error in self.errors(ANIMATED.replace("<rect/>", "<rect><animate/></rect>"))))
        self.assertTrue(any("longer" in error for error in self.errors(ANIMATED.replace("24s", "31s"))))
        self.assertTrue(any("simulated" in error for error in self.errors(ANIMATED.replace("Simulated desktop", "Desktop"))))

    def test_unused_keyframes_fail(self):
        unused = ANIMATED.replace("@media", "@keyframes k1{0%{opacity:0}100%{opacity:1}}@media")
        self.assertTrue(self.errors(unused))


class PictureChecks(unittest.TestCase):
    def errors(self, source: str) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            file = root / "README.md"
            file.write_text(source, encoding="utf-8")
            with patch.object(check, "ROOT", root):
                return check.check_pictures(file)

    def test_four_variants_in_order_with_alt_text_pass(self):
        self.assertEqual(self.errors(PICTURE), [])

    def test_a_dark_source_before_the_phone_sources_fails(self):
        dark = '<source media="(prefers-color-scheme: dark)" srcset="a/hero-dark.svg">'
        self.assertEqual(len(self.errors(PICTURE.replace(dark, "").replace("<picture>", "<picture>" + dark))), 1)

    def test_a_source_of_another_picture_fails(self):
        self.assertEqual(len(self.errors(PICTURE.replace("a/hero-dark.svg", "a/gate-dark.svg"))), 1)

    def test_an_image_without_alt_text_fails_in_html_and_in_markdown(self):
        self.assertEqual(len(self.errors(PICTURE.replace(' alt="A stable interface to a changing desktop"', ""))), 1)
        self.assertEqual(len(self.errors("![](badge.svg)")), 1)
        self.assertEqual(self.errors("![Rust 1.99.0](badge.svg)"), [])


class TwinChecks(unittest.TestCase):
    def test_twins_share_structure_and_destinations_not_words(self):
        english = "# Title\n\nText [guide](docs/guide.md#use) and [ADR](docs/adr/1.md).\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> [!TIP]\n> x\n"
        spanish = "# Título\n\nTexto [guía](docs/guide.es.md#uso) y [ADR](docs/adr/1.md).\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> [!TIP]\n> x\n"
        self.assertEqual(check.shape(english), check.shape(spanish))

    def test_a_missing_row_link_or_heading_is_a_difference(self):
        base = "# Title\n\n[a](x.md)\n\n| a |\n|---|\n| 1 |\n| 2 |\n"
        for other in (base.replace("| 2 |\n", ""), base.replace("[a](x.md)", "a"), base + "\n## More\n"):
            self.assertNotEqual(check.shape(base), check.shape(other))


class WordCount(unittest.TestCase):
    def test_markup_targets_and_alt_text_are_not_words_a_reader_sees(self):
        text = 'Two words <img src="a.svg" alt="three alt words"> [a link](docs/long/path.md) `code`\n\n| a | b |\n|---|---|\n'
        self.assertEqual(check.words(text), (7, 3))


class ManifestChecks(unittest.TestCase):
    def run_check(self, files: dict[str, bytes], edit=None) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            listed = {}
            for name, data in files.items():
                (assets / name).write_bytes(data)
                kind = "animated-svg" if b"@keyframes" in data else {".svg": "svg", ".gif": "gif", ".png": "png"}[Path(name).suffix]
                listed[name] = {"kind": kind, "reproducible": "renderer" if kind in ("gif", "png") else "bytes",
                                "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
            manifest = {"schema": 2, "assets": listed}
            if edit:
                edit(manifest, assets)
            (assets / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
            with patch.object(check, "ASSETS", assets):
                return check.check_manifest()[0]

    def family(self) -> dict[str, bytes]:
        return {f"hero-{suffix}": b"<svg/>" for suffix in check.SUFFIXES}

    def test_a_complete_family_with_matching_digests_passes(self):
        self.assertEqual(self.run_check(self.family()), [])

    def test_a_file_the_manifest_does_not_list_fails(self):
        errors = self.run_check(self.family(), lambda manifest, assets: (assets / "stray.mp4").write_bytes(b"x"))
        self.assertEqual(errors, ["stray.mp4: in docs/assets/readme but not in the manifest"])

    def test_a_changed_file_a_missing_variant_and_an_oversized_asset_fail(self):
        self.assertEqual(len(self.run_check(self.family(), lambda m, a: (a / "hero-dark.svg").write_bytes(b"<svg> </svg>"))), 1)
        files = self.family()
        del files["hero-mobile-dark.svg"]
        self.assertEqual(self.run_check(files), ["hero-mobile-dark.svg: missing variant"])
        self.assertTrue(any("exceeds" in error for error in self.run_check({"demo.gif": b"x" * (check.BUDGET["gif"] + 1)})))

    def test_an_animated_file_listed_as_a_still_fails(self):
        def relabel(manifest, assets):
            manifest["assets"]["demo-light.svg"]["kind"] = "svg"
        files = {f"demo-{suffix}": ANIMATED.encode() for suffix in check.SUFFIXES}
        self.assertTrue(any("listed as a still" in error for error in self.run_check(files, relabel)))


if __name__ == "__main__":
    unittest.main()
