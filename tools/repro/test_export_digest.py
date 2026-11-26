"""Tests of tools/repro/export_digest.py (REL-01, P4.4): the comparison of two export fingerprints and
the reading of a kernel probe. Standard library only: no torch, no onnx, no weights, no file but
temporary ones."""
import json
import tempfile
import unittest
from pathlib import Path

import export_digest


def fingerprint(**changes):
    base = {
        "format": export_digest.FORMAT,
        "file_sha256": "f" * 64,
        "header": {"ir_version": 8, "producer": ["pytorch", "2.14.1"], "opsets": [["", 17]]},
        "nodes": {"count": 3, "order_sha256": "n" * 64},
        "initializers": [["onnx::Conv_1", 1, [4, 3, 3, 3], "a" * 64],
                         ["onnx::Conv_2", 1, [4], "b" * 64],
                         ["cx8", 1, [160], "c" * 64]],
    }
    base.update(changes)
    return base


class CompareTest(unittest.TestCase):
    def test_two_identical_exports_have_no_difference(self):
        result = export_digest.compare(fingerprint(), fingerprint())
        self.assertTrue(result["file_equal"])
        self.assertTrue(result["same_structure"])
        self.assertEqual(result["differing_initializers"], [])

    def test_same_structure_with_other_values_names_the_initializers_that_differ(self):
        other = fingerprint(file_sha256="e" * 64)
        other["initializers"] = [["onnx::Conv_1", 1, [4, 3, 3, 3], "z" * 64],
                                 ["onnx::Conv_2", 1, [4], "y" * 64],
                                 ["cx8", 1, [160], "c" * 64]]
        result = export_digest.compare(fingerprint(), other)
        self.assertFalse(result["file_equal"])
        self.assertTrue(result["same_structure"])
        self.assertEqual(result["differing_initializers"], ["onnx::Conv_1", "onnx::Conv_2"])
        self.assertEqual(result["differing_initializer_count"], 2)

    def test_another_graph_is_not_the_same_structure(self):
        for other in (fingerprint(nodes={"count": 3, "order_sha256": "m" * 64}),
                      fingerprint(header={"ir_version": 9, "producer": ["pytorch", "2.14.1"],
                                          "opsets": [["", 17]]}),
                      fingerprint(initializers=fingerprint()["initializers"][::-1])):
            self.assertFalse(export_digest.compare(fingerprint(), other)["same_structure"])

    def test_a_fingerprint_of_another_format_is_refused(self):
        with self.assertRaises(ValueError):
            export_digest.compare(fingerprint(), fingerprint(format="other/1"))

    def test_compare_reads_two_files_and_never_needs_onnx(self):
        scratch = tempfile.TemporaryDirectory()
        self.addCleanup(scratch.cleanup)
        a, b = Path(scratch.name) / "a.json", Path(scratch.name) / "b.json"
        a.write_text(json.dumps(fingerprint()), encoding="utf-8")
        b.write_text(json.dumps(fingerprint()), encoding="utf-8")
        out = Path(scratch.name) / "out.json"
        self.assertEqual(export_digest.main(["compare", str(a), str(b), "--out", str(out)]), 0)
        self.assertTrue(json.loads(out.read_text(encoding="utf-8"))["file_equal"])


class KernelReadingTest(unittest.TestCase):
    def test_the_recorded_kernel_hashes_are_named(self):
        self.assertEqual(export_digest.kernel_path("fe3367ed85f7"), "correctly_rounded")
        self.assertEqual(export_digest.kernel_path("b5a552aa4d18"), "mkl_sse4_2_or_compatible")
        self.assertEqual(export_digest.kernel_path("0123456789ab"), "not_seen_on_2026_11_15")


if __name__ == "__main__":
    unittest.main()
