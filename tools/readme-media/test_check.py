"""Boundary cases of the offline documentation checker."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import check


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


if __name__ == "__main__":
    unittest.main()
