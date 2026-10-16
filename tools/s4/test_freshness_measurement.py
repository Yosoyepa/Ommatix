"""M2 tooling tests: no process, window read, screen capture or input."""

import importlib.util
import tempfile
import tomllib
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("m2_driver", HERE / "e2e_driver.py")
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
spec = importlib.util.spec_from_file_location("m2_dossier", HERE / "e2e_dossier.py")
dossier = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dossier)


class FreshnessMeasurementTest(unittest.TestCase):
    def test_pin_component_spellings_refuse_before_copying_in_both_clients(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            source.parent.mkdir()
            bad = ["pin\\", "pin//x", "pin.", "pin ", r".\pin", r"a\.\b", "pin:stream",
                   r"a\b:c", "pin?", "pin*", "pin<", "pin>", "pin|", 'pin"']
            for index, value in enumerate(bad):
                source.write_text("[safety]\nwindow_pin_file = '" + value + "'\n", encoding="utf-8")
                for client in (driver, dossier.driver):
                    with self.subTest(client=client.__name__, value=value):
                        folder = root / f"run-{index}-{client.__name__}"
                        args = SimpleNamespace(config=str(source), results=folder / "results.jsonl",
                                               target_window_freshness=True)
                        with self.assertRaises(client.Abort):
                            client.freshness_config(args)
                        self.assertFalse(folder.exists())

    def test_plain_relative_copy_keeps_raw_config_directory_and_relative_separators(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            source.parent.mkdir()
            source.write_text("[safety]\nwindow_pin_file = 'nested/pin'\n", encoding="utf-8")
            raw_source = str(root) + "/operator/live.toml"
            expected = str(root) + "/operator\\nested/pin"
            for client in (driver, dossier.driver):
                args = SimpleNamespace(config=raw_source, results=root / "run" / "results.jsonl",
                                       target_window_freshness=True)
                client.freshness_config(args)
                copied = tomllib.loads(args.config.read_text(encoding="utf-8"))
                self.assertEqual(copied["safety"]["window_pin_file"], expected)

    def test_both_clis_validate_raw_config_before_path_objects_can_remove_separators(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            source.parent.mkdir()
            source.write_text("[engine]\n", encoding="utf-8")
            raw_source = str(source.parent) + "\\\\" + source.name
            for program, client, app in ((driver, driver, "notepad"), (dossier, dossier.driver, "decoy")):
                results = root / program.__name__ / "results.jsonl"
                argv = ["client.py", "--exe", "ommatix.exe", "--config", raw_source,
                        "--app", app, "--ledger", str(root / "ledger"), "--results", str(results),
                        "--target-window-freshness"]
                with mock.patch.object(program.sys, "argv", argv), \
                     mock.patch.object(client, "log"), mock.patch.object(client, "use_utf8_stdout"), \
                     mock.patch.object(client.s3, "Server") as server:
                    self.assertEqual(program.main(), 2)
                    server.assert_not_called()
                self.assertFalse(results.parent.exists())

    def test_ambiguous_windows_operator_paths_refuse_before_writes_in_both_clients(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            source.parent.mkdir()
            bad = [source.drive + "STOP", r"\STOP", r"..\STOP", "",
                   r"\\?\C:\STOP", r"\\.\C:\STOP"]
            resolve = Path.resolve

            def no_stop_lookup(path, *args, **kwargs):
                return path if path.name == "STOP" else resolve(path, *args, **kwargs)

            for index, value in enumerate(bad):
                source.write_text("[safety]\nstop_file = '" + value + "'\n", encoding="utf-8")
                for client in (driver, dossier.driver):
                    with self.subTest(client=client.__name__, value=value):
                        folder = root / f"run-{index}-{client.__name__}"
                        args = SimpleNamespace(config=str(source), results=folder / "results.jsonl",
                                               target_window_freshness=True)
                        with mock.patch.object(Path, "resolve", no_stop_lookup):
                            with self.assertRaises(client.Abort):
                                client.freshness_config(args)
                        self.assertFalse(folder.exists())
                        self.assertEqual(args.config, str(source))

    def test_ambiguous_config_paths_are_refused_before_reading_or_writing(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            bad = [r"\\?" + "\\" + str(source), r"\\." + "\\" + str(source),
                   source.drive + "live.toml", r"\live.toml", "",
                   str(source.parent / "nested" / ".." / source.name)]
            for index, value in enumerate(bad):
                for client in (driver, dossier.driver):
                    with self.subTest(client=client.__name__, value=value):
                        folder = root / f"run-{index}-{client.__name__}"
                        args = SimpleNamespace(config=value, results=folder / "results.jsonl",
                                               target_window_freshness=True)
                        with mock.patch.object(Path, "is_symlink", return_value=False), \
                             mock.patch.object(Path, "read_text", return_value="[engine]\n") as read:
                            with self.assertRaises(client.Abort):
                                client.freshness_config(args)
                            read.assert_not_called()
                        self.assertFalse(folder.exists())

    def test_absolute_operator_paths_are_preserved_byte_for_byte_in_both_clients(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "operator" / "live.toml"
            # Mock resolution to avoid contacting the UNC host; these are pure path tests.
            accepted = [source.drive + r"\operator/STOP", r"\\server\share\folder\STOP"]
            for client in (driver, dossier.driver):
                for value in accepted:
                    config = {"safety": {"stop_file": value}}
                    with mock.patch.object(Path, "resolve", lambda path: path):
                        client.rebase_operator_paths(config, str(source))
                    self.assertEqual(config["safety"]["stop_file"], value)

    def test_rejected_path_explanation_reaches_both_clients_without_starting_a_server(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "live.toml"
            source.write_text("[safety]\nstop_file = '" + source.drive + "STOP'\n", encoding="utf-8")
            for program, client in ((driver, driver), (dossier, dossier.driver)):
                args = SimpleNamespace(config=str(source), results=root / "run" / "results.jsonl",
                                       target_window_freshness=True)
                with mock.patch.object(program.argparse.ArgumentParser, "parse_args", return_value=args), \
                     mock.patch.object(client, "log") as log, \
                     mock.patch.object(client, "use_utf8_stdout"), \
                     mock.patch.object(client.s3, "Server") as server:
                    self.assertEqual(program.main(), 2)
                    self.assertIn("drive-relative", log.call_args.kwargs["reason"])
                    server.assert_not_called()
                self.assertFalse(args.results.parent.exists())

    def test_symlinked_configuration_is_refused_before_copying_in_both_clients(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            source.parent.mkdir()
            source.write_text('[safety]\nstop_file = "STOP"\n', encoding="utf-8")
            for client in (driver, dossier.driver):
                with self.subTest(client=client.__name__):
                    args = SimpleNamespace(config=str(source), results=root / "run" / "results.jsonl",
                                           target_window_freshness=True)
                    # Model a file symlink without requiring Windows symlink privileges.
                    with mock.patch.object(Path, "is_symlink", lambda path: path == source):
                        with self.assertRaises(client.Abort):
                            client.freshness_config(args)
                    self.assertEqual(args.config, str(source))
                    self.assertFalse(args.results.parent.exists())

    def test_relative_stop_and_pin_keep_the_supplied_config_directory_in_both_clients(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            source.parent.mkdir()
            (source.parent / "STOP").write_text("engaged", encoding="utf-8")
            (source.parent / "pin").write_text("123", encoding="utf-8")
            source.write_text('[safety]\nstop_file = "STOP"\nwindow_pin_file = "pin"\n',
                              encoding="utf-8")
            for client in (driver, dossier.driver):
                args = SimpleNamespace(config=str(source), results=root / "run" / "results.jsonl",
                                       target_window_freshness=True)
                client.freshness_config(args)
                copied = tomllib.loads(args.config.read_text(encoding="utf-8"))
                for key, filename in (("stop_file", "STOP"), ("window_pin_file", "pin")):
                    self.assertEqual(Path(copied["safety"][key]), source.parent / filename)
                    self.assertTrue(Path(copied["safety"][key]).samefile(source.parent / filename))
                self.assertEqual((source.parent / "STOP").read_text(), "engaged")

    def test_operator_path_rebasing_is_pure_text_without_path_library_calls(self):
        source = r"C:\operator/mixed\live.toml"
        for client in (driver, dossier.driver):
            config = {"safety": {"stop_file": "STOP", "window_pin_file": "nested/pin"}}
            with mock.patch.object(client, "Path", side_effect=AssertionError("operator paths are raw text")):
                client.rebase_operator_paths(config, source)
            self.assertEqual(config["safety"], {"stop_file": r"C:\operator/mixed\STOP",
                             "window_pin_file": r"C:\operator/mixed\nested/pin"})

    def test_a_normalized_config_path_object_is_refused_before_copying(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "live.toml"
            source.write_text("[engine]\n", encoding="utf-8")
            for client in (driver, dossier.driver):
                args = SimpleNamespace(config=source, results=root / "run" / "results.jsonl",
                                       target_window_freshness=True)
                with self.assertRaises(client.Abort):
                    client.freshness_config(args)
                self.assertFalse(args.results.parent.exists())

    def test_component_whitelist_applies_to_drive_unc_and_relative_forms(self):
        accepted = [(r"C:\operator\STOP", True), (r"\\server\share\operator\STOP", True),
                    (r"sub-folder\pin_1.dat", False), ("sub folder/pin-2", False)]
        rejected = [r"C:\operator\pin.", r"\\server.\share\pin", r"\\server\share.\pin",
                    r"C:\operator\pin:stream", "C:\\operator\\pin\\", r"C:/operator\pin"]
        for client in (driver, dossier.driver):
            for value, absolute in accepted:
                self.assertEqual(client.m2_path_is_absolute(value), absolute)
            for value in rejected:
                with self.assertRaises(client.Abort):
                    client.m2_path_is_absolute(value)

    def test_all_current_path_keys_rebase_plain_relatives_against_the_supplied_base(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "operator").mkdir()
            source = root / "operator" / "live.toml"
            original = {
                "safety": {"stop_file": "STOP", "window_pin_file": "pin", "purge_file": "purge"},
                "models": {"detector_manifest": "manifest.json", "onnxruntime_path": "ort.dll",
                           "directml_path": "dml.dll", "detector_path": "weights", "ocr_path": "ocr"},
                "cache": {"prototypes": "icons.json"}}
            for client in (driver, dossier.driver):
                config = {section: dict(values) for section, values in original.items()}
                client.rebase_operator_paths(config, str(source))
                for section, values in original.items():
                    for key, value in values.items():
                        self.assertEqual(Path(config[section][key]), source.parent / value)

    def test_unmapped_or_invalid_operator_paths_are_refused_before_copying(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "live.toml"
            for raw in ('[safety]\nnew_stop_file = "STOP"\n', '[safety]\nstop_file = 4\n'):
                source.write_text(raw, encoding="utf-8")
                for client in (driver, dossier.driver):
                    args = SimpleNamespace(config=str(source), results=root / "run" / "results.jsonl",
                                           target_window_freshness=True)
                    with self.assertRaises(client.Abort):
                        client.freshness_config(args)
                    self.assertFalse(args.results.parent.exists())

    def test_d0_nested_counts_do_not_break_timing_conversion(self):
        meta = {driver.ACTION_TIMINGS_KEY: {"total_us": 1234, "freshness_diff": {
            "total_tiles": 5, "group_tiles": 0, "touches_target": False}}}
        self.assertEqual(driver.stage_ms(meta), {"action_total": 1.234})
        self.assertEqual(driver.freshness_fields({"status": "rejected"}, meta), {
            "freshness_diff": {"total_tiles": 5, "group_tiles": 0, "touches_target": False}})

    def test_unattested_group_is_absent_and_screen_data_is_discarded(self):
        meta = {driver.ACTION_TIMINGS_KEY: {"freshness_diff": {
            "total_tiles": 0, "touches_target": False, "label": "private", "x": 5}}}
        self.assertEqual(driver.freshness_fields({"status": "rejected"}, meta), {
            "freshness_diff": {"total_tiles": 0, "touches_target": False}})
        self.assertEqual(driver.freshness_fields({"status": "input_delivered"}, meta), {})
        self.assertEqual(driver.freshness_fields({"status": "rejected"}, {}), {})

    def test_diagnostic_types_are_checked_without_coercion(self):
        meta = {driver.ACTION_TIMINGS_KEY: {"freshness_diff": {
            "total_tiles": True, "group_tiles": -1, "touches_target": "private"}}}
        self.assertEqual(driver.freshness_fields({"status": "rejected"}, meta), {})

    def test_variant_is_a_separate_copy_and_preserves_operator_paths_and_permissions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "operator" / "live.toml"
            source.parent.mkdir()
            raw = ('[engine]\naction_freshness = "screen"\n'
                   '[safety]\nactuation_enabled = true\nstop_file = "STOP"\n'
                   'window_pin_file = "pin"\npurge_file = "purge"\n'
                   'allowed_applications = ["explorer.exe"]\n'
                   '[safety.allowed_window_classes]\n"explorer.exe" = ["CabinetWClass"]\n'
                   '[models]\ndetector_manifest = "model/manifest.json"\n'
                   'onnxruntime_path = "runtime.dll"\n[cache]\nprototypes = "icons.json"\n')
            source.write_text(raw, encoding="utf-8")
            args = SimpleNamespace(config=str(source), results=root / "run" / "results.jsonl",
                                   target_window_freshness=True)
            driver.freshness_config(args)
            self.assertNotEqual(args.config, source)
            self.assertTrue(args.config.is_relative_to(args.results.parent))
            self.assertEqual(source.read_text(encoding="utf-8"), raw)
            variant = tomllib.loads(args.config.read_text(encoding="utf-8"))
            self.assertEqual(variant["engine"]["action_freshness"], "target_window")
            self.assertTrue(variant["safety"]["actuation_enabled"])
            self.assertEqual(variant["safety"]["allowed_window_classes"],
                             {"explorer.exe": ["CabinetWClass"]})
            for key in ("stop_file", "window_pin_file", "purge_file"):
                expected = {"stop_file": "STOP", "window_pin_file": "pin", "purge_file": "purge"}[key]
                self.assertEqual(Path(variant["safety"][key]), source.parent / expected)
            self.assertEqual(Path(variant["models"]["detector_manifest"]),
                             source.parent / "model/manifest.json")
            first = args.config
            args.config = str(source)
            driver.freshness_config(args)
            self.assertNotEqual(args.config, first)

    def test_without_opt_in_no_copy_or_config_read_occurs(self):
        args = SimpleNamespace(config=Path("absent.toml"))
        driver.freshness_config(args)
        self.assertEqual(args.config, Path("absent.toml"))

    def test_repository_run_folder_is_refused_before_writing(self):
        args = SimpleNamespace(config=Path("absent.toml"), results=HERE / "results.jsonl",
                               target_window_freshness=True)
        with self.assertRaises(driver.Abort):
            driver.freshness_config(args)


if __name__ == "__main__":
    unittest.main()
