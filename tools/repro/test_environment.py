"""Tests of tools/repro/environment.py (REL-01): the parts that decide whether an artefact
is the pinned one. Standard library only; they read no real artefact and run no tool."""
import hashlib
import json
import shutil
import tempfile
import unittest
from pathlib import Path

import environment

CONFIG = '''
[models]
detector_manifest = "models/manifest.json"
detector_manifest_sha256 = "%s"
onnxruntime_path = "runtime/onnxruntime.dll"
onnxruntime_sha256 = "%s"
# directml_sha256 = "%s"
directml_path = "runtime/DirectML.dll"
directml_sha256 = "%s"
'''


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class PinsTest(unittest.TestCase):
    def test_a_pin_names_the_path_it_pins(self):
        text = CONFIG % ('a' * 64, 'b' * 64, 'c' * 64, 'd' * 64)
        found = {p['key']: (p['path'], p['sha256']) for p in environment.pins(text)}
        self.assertEqual(found, {
            'detector_manifest_sha256': ('models/manifest.json', 'a' * 64),
            'onnxruntime_sha256': ('runtime/onnxruntime.dll', 'b' * 64),
            'directml_sha256': ('runtime/DirectML.dll', 'd' * 64),
        })

    def test_a_commented_pin_and_a_short_digest_are_not_pins(self):
        text = '# onnxruntime_sha256 = "%s"\nfoo_sha256 = "abc"\n' % ('a' * 64)
        self.assertEqual(environment.pins(text), [])

    def test_the_shipped_configurations_pin_what_their_keys_name(self):
        for config in sorted((environment.REPO / 'tools' / 's0').glob('*.toml')):
            found = environment.pins(config.read_text(encoding='utf-8'))
            for pin in found:
                self.assertIsNotNone(pin['path'], f'{config.name}: {pin["key"]} pins no path')


class VerifyTest(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir, ignore_errors=True)
        (self.dir / 'models').mkdir()
        (self.dir / 'runtime').mkdir()
        self.weights = b'fake weights'
        manifest = {'status': 'SELECCIONADO', 'artifacts': [
            {'role': 'detector', 'path': 'w.onnx', 'sha256': digest(self.weights), 'license': 'MIT'},
            {'role': 'ocr', 'sha256': 'no aplica'}]}
        (self.dir / 'models' / 'manifest.json').write_text(json.dumps(manifest))
        (self.dir / 'models' / 'w.onnx').write_bytes(self.weights)
        (self.dir / 'runtime' / 'onnxruntime.dll').write_bytes(b'runtime')
        (self.dir / 'runtime' / 'DirectML.dll').write_bytes(b'directml')
        self.config = self.dir / 'c.toml'

    def write(self):
        values = (
            digest((self.dir / 'models' / 'manifest.json').read_bytes()),
            digest(b'runtime'), 'c' * 64, digest(b'directml'),
        )
        self.config.write_text(CONFIG % values)

    def statuses(self):
        return {r['key']: r['status'] for r in environment.verify(self.config, self.dir)}

    def test_every_file_with_its_digest_is_ok_and_the_weights_are_checked(self):
        self.write()
        self.assertEqual(self.statuses(), {
            'detector_manifest_sha256': 'ok',
            'weights of models/manifest.json': 'ok',
            'onnxruntime_sha256': 'ok',
            'directml_sha256': 'ok',
        })

    def test_a_tampered_runtime_is_a_mismatch(self):
        self.write()
        (self.dir / 'runtime' / 'onnxruntime.dll').write_bytes(b'patched runtime')
        self.assertEqual(self.statuses()['onnxruntime_sha256'], 'mismatch')

    def test_tampered_weights_are_a_mismatch_even_when_the_manifest_is_the_pinned_one(self):
        self.write()
        (self.dir / 'models' / 'w.onnx').write_bytes(b'other weights')
        self.assertEqual(self.statuses()['weights of models/manifest.json'], 'mismatch')

    def test_a_tampered_manifest_is_a_mismatch_and_its_weights_are_not_trusted(self):
        self.write()
        path = self.dir / 'models' / 'manifest.json'
        path.write_text(path.read_text() + ' ')
        statuses = self.statuses()
        self.assertEqual(statuses['detector_manifest_sha256'], 'mismatch')
        self.assertNotIn('weights of models/manifest.json', statuses)

    def test_a_missing_file_is_absent_not_ok(self):
        self.write()
        (self.dir / 'runtime' / 'DirectML.dll').unlink()
        self.assertEqual(self.statuses()['directml_sha256'], 'absent')


class MachineFactsTest(unittest.TestCase):
    """Two facts a clean-machine session needs to read its own failures (S4_SESIONES_PENDIENTES
    §A.2): whether Smart App Control can block a locally built binary, and which Rust host built it."""

    def test_the_smart_app_control_state_is_a_closed_name(self):
        # HKLM\\SYSTEM\\CurrentControlSet\\Control\\CI\\Policy, VerifiedAndReputablePolicyState
        self.assertEqual(environment.smart_app_control(0), 'off')
        self.assertEqual(environment.smart_app_control(1), 'on')
        self.assertEqual(environment.smart_app_control(2), 'evaluation')

    def test_a_missing_or_unknown_smart_app_control_value_is_never_off(self):
        self.assertEqual(environment.smart_app_control(None), 'not_reported')
        self.assertEqual(environment.smart_app_control(7), 'unknown')
        self.assertEqual(environment.smart_app_control('1'), 'unknown')
        self.assertEqual(environment.smart_app_control(True), 'unknown')

    def test_the_rust_host_is_read_from_rustc_verbose_version(self):
        verbose = ('rustc 1.99.0 (b940084d7 2026-09-28)\nbinary: rustc\n'
                   'commit-hash: b940084d7eb6a299eb4bfeb8e34901bc051e7ac4\n'
                   'host: x86_64-pc-windows-gnu\nrelease: 1.99.0\n')
        self.assertEqual(environment.rustc_host(verbose), 'x86_64-pc-windows-gnu')

    def test_no_rustc_output_is_no_host(self):
        self.assertIsNone(environment.rustc_host(None))
        self.assertIsNone(environment.rustc_host('rustc 1.99.0\n'))


if __name__ == '__main__':
    unittest.main()
