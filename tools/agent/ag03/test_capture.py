"""AG-03 capture (ADR-0038 decision 5) against fake windows and fake backends. No real screen: the only
admitted backend renders the window itself; a desktop-source copy is never admitted, in any scene, and
there is no fallback between backends."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parent / "ag02")]

import fake_os  # noqa: E402
import pixel_capture as cap  # noqa: E402
import pixel_guards as g  # noqa: E402

SCENES = ("opaque", "translucent", "backdrop", "rounded", "foreign_during_acquisition")
W = fake_os.window_4x3()


def desktop_backend(scene):
    """A screen copy of the window's rectangle: what is on screen during the copy."""
    return fake_os.SceneBackend(scene, app_rendered=False)


def app_backend(scene="opaque"):
    return fake_os.SceneBackend(scene, app_rendered=True)


class DesktopSourceIsNeverAdmittedTests(unittest.TestCase):
    def test_a_desktop_source_copy_gives_no_image_in_every_scene(self):
        for scene in SCENES:
            backend = desktop_backend(scene)
            # even with a record that claims everything passed, a backend that is not app-rendered is refused
            for record in (fake_os.full_admission(), None):
                with self.subTest(scene=scene, record=record is not None):
                    with self.assertRaises(cap.CaptureRefused) as raised:
                        cap.capture_window(W, backend, record)
                    self.assertEqual(raised.exception.reason, "BACKEND_NOT_ADMITTED")
            self.assertEqual(backend.calls, 0, "a backend that is not admitted is never even asked for pixels")

    def test_the_foreign_window_present_only_during_acquisition_with_identical_states_before_and_after(self):
        # The leak the review found: the first and the second enumeration are identical, the occluder
        # exists only while the copy happens. A desktop-source backend returns its pixels; it is refused
        # before it is called, and the admission test rejects it by looking at the output.
        backend = desktop_backend("foreign_during_acquisition")
        frame = backend.acquire(W)
        self.assertIn(fake_os.OCCLUDER, [frame.bgra[i:i + 4] for i in range(0, len(frame.bgra), 4)])
        self.assertFalse(cap.scenario_passes(backend, fake_os.scene("foreign_during_acquisition")))
        with self.assertRaises(cap.CaptureRefused):
            cap.capture_window(W, desktop_backend("foreign_during_acquisition"), fake_os.full_admission())

    def test_the_admission_test_rejects_a_backend_whose_output_contains_foreign_pixels(self):
        for scene in ("translucent", "backdrop", "rounded", "foreign_during_acquisition"):
            with self.subTest(scene=scene):
                self.assertFalse(cap.scenario_passes(desktop_backend(scene), fake_os.scene(scene)))

    def test_a_desktop_copy_of_an_opaque_window_is_still_not_admitted(self):
        # Fidelity holds for an opaque window, but admission is for the backend, not the scene.
        backend = desktop_backend("opaque")
        self.assertTrue(cap.scenario_passes(backend, fake_os.scene("opaque")))
        self.assertFalse(fake_os.full_admission().admits(backend))

    def test_there_is_no_screen_copy_backend_in_the_module(self):
        names = {n for n in dir(cap) if "backend" in n.lower() or "screen" in n.lower() or "desktop" in n.lower()}
        self.assertEqual(sorted(cap.ADMITTED_BACKENDS), ["printwindow"])
        self.assertFalse({"ScreenCopyBackend", "DesktopBackend", "BitBltBackend"} & names)


class AppRenderedBackendTests(unittest.TestCase):
    def test_an_app_rendered_backend_with_an_isolated_output_gives_the_image(self):
        for scene in SCENES:
            with self.subTest(scene=scene):
                backend = app_backend(scene)
                self.assertTrue(cap.scenario_passes(backend, fake_os.scene(scene)))
                captured = cap.capture_window(W, backend, fake_os.full_admission())
                self.assertEqual((captured.width, captured.height), (4, 3))
                self.assertEqual(cap.png_info(captured.png), (4, 3))

    def test_the_image_is_the_applications_own_pixels_converted_to_rgb(self):
        captured = cap.capture_window(W, app_backend(), fake_os.full_admission())
        import struct
        import zlib
        pos, idat = 8, b""
        while pos < len(captured.png):
            length, kind = struct.unpack(">I4s", captured.png[pos:pos + 8])
            if kind == b"IDAT":
                idat += captured.png[pos + 8:pos + 8 + length]
            pos += 12 + length
        raw = zlib.decompress(idat)
        first_row = raw[1:1 + 12]
        own = fake_os.OWN
        self.assertEqual(first_row, bytes([own[0][2], own[0][1], own[0][0], own[1][2], own[1][1], own[1][0],
                                           own[2][2], own[2][1], own[2][0], own[3][2], own[3][1], own[3][0]]))

    def test_the_record_must_cover_every_required_scenario(self):
        backend = app_backend()
        for missing in cap.REQUIRED_SCENARIOS:
            scenarios = {name: name != missing for name in cap.REQUIRED_SCENARIOS}
            record = cap.AdmissionRecord("printwindow", scenarios)
            with self.subTest(missing=missing):
                self.assertFalse(record.admits(backend))
                with self.assertRaises(cap.CaptureRefused):
                    cap.capture_window(W, backend, record)

    def test_a_record_for_another_backend_name_does_not_admit(self):
        self.assertFalse(cap.AdmissionRecord("bitblt", {n: True for n in cap.REQUIRED_SCENARIOS}).admits(app_backend()))

    def test_a_backend_that_calls_itself_by_another_name_is_not_admitted(self):
        backend = fake_os.SceneBackend("opaque", app_rendered=True, name="bitblt")
        self.assertFalse(fake_os.full_admission("bitblt").admits(backend))

    def test_no_record_means_no_capture(self):
        with self.assertRaises(cap.CaptureRefused) as raised:
            cap.capture_window(W, app_backend(), None)
        self.assertEqual(raised.exception.reason, "BACKEND_NOT_ADMITTED")


class FailureTests(unittest.TestCase):
    def refused(self, backend):
        with self.assertRaises(cap.CaptureRefused) as raised:
            cap.capture_window(W, backend, fake_os.full_admission())
        return raised.exception.reason

    def test_a_failing_printwindow_is_refused_without_an_image(self):
        self.assertEqual(self.refused(fake_os.FailingBackend("raise")), "BACKEND_FAILED")
        self.assertEqual(self.refused(fake_os.FailingBackend("none")), "BACKEND_FAILED")

    def test_a_result_of_the_wrong_size_is_refused(self):
        self.assertEqual(self.refused(fake_os.FailingBackend("wrong_size")), "WRONG_SIZE")
        self.assertEqual(self.refused(fake_os.FailingBackend("short_buffer")), "WRONG_SIZE")

    def test_there_is_no_fallback_between_backends(self):
        first = fake_os.FailingBackend("raise")
        second = app_backend()
        with self.assertRaises(cap.CaptureRefused):
            cap.capture_window(W, first, fake_os.full_admission())
        self.assertEqual(second.calls, 0)  # capture_window takes one backend: nothing else is tried

    def test_an_oversized_image_is_refused(self):
        original = cap.MAX_IMAGE_BYTES
        cap.MAX_IMAGE_BYTES = 10
        try:
            self.assertEqual(self.refused(app_backend()), "IMAGE_TOO_LARGE")
        finally:
            cap.MAX_IMAGE_BYTES = original

    def test_the_admission_record_must_be_exactly_the_expected_shape(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "a.json"
            self.assertIsNone(cap.load_admission(path))
            path.write_text("not json", encoding="utf-8")
            self.assertIsNone(cap.load_admission(path))
            path.write_text(json.dumps({"schema": "other", "backend": "printwindow", "scenarios": {}}), encoding="utf-8")
            self.assertIsNone(cap.load_admission(path))
            path.write_text(json.dumps(cap.build_admission(app_backend(), [fake_os.scene(n) for n in SCENES])),
                            encoding="utf-8")
            record = cap.load_admission(path)
            self.assertTrue(record.admits(app_backend()))

    def test_a_missing_scene_makes_the_built_record_refuse(self):
        body = cap.build_admission(app_backend(), [fake_os.scene(n) for n in SCENES[:-1]])
        self.assertFalse(body["scenarios"]["foreign_during_acquisition"])
        self.assertFalse(cap.AdmissionRecord(body["backend"], body["scenarios"]).admits(app_backend()))


class ImageHelperTests(unittest.TestCase):
    def test_png_round_trip_and_header(self):
        png = cap.png_encode(3, 2, bytes(range(18)))
        self.assertEqual(cap.png_info(png), (3, 2))
        self.assertIsNone(cap.png_info(b"not a png"))
        with self.assertRaises(ValueError):
            cap.png_encode(3, 2, bytes(5))

    def test_large_windows_are_scaled_down_to_the_edge_limit(self):
        self.assertEqual(cap.fit_size(1280, 720), (1280, 720))
        self.assertEqual(cap.fit_size(1568, 1568), (1568, 1568))
        w, h = cap.fit_size(2560, 1440)
        self.assertEqual(w, cap.MAX_IMAGE_EDGE)
        self.assertEqual(h, round(1440 * cap.MAX_IMAGE_EDGE / 2560))

    def test_nearest_neighbour_scaling_keeps_the_colours_and_the_size(self):
        rgb = bytes([255, 0, 0] * 4 + [0, 0, 255] * 4)  # 4 x 2: red row, blue row
        small = cap.scale_rgb(rgb, 4, 2, 2, 1)
        self.assertEqual(len(small), 6)
        self.assertEqual(small[:3], bytes([255, 0, 0]))

    def test_synthetic_images_are_drawn_in_memory_and_contrast(self):
        seen = {}
        for i in range(len(cap.SYNTHETIC_SCENES)):
            png, word, shape = cap.synthetic_png(i, 320, 200)
            self.assertEqual(cap.png_info(png), (320, 200))
            seen[(word, shape)] = png
        self.assertEqual(len(seen), len(cap.SYNTHETIC_SCENES))
        self.assertEqual(len(set(seen.values())), len(cap.SYNTHETIC_SCENES))

    def test_nothing_in_the_module_writes_a_file(self):
        source = (HERE / "pixel_capture.py").read_text(encoding="utf-8")
        for needle in ("write_bytes", "write_text", "open(", "NamedTemporaryFile", ".save("):
            if needle == "open(":
                self.assertEqual(source.count("open("), 0)
            else:
                self.assertNotIn(needle, source)


class SyntheticModeIsNeverACaptureTests(unittest.TestCase):
    def test_a_real_capture_never_happens_in_synthetic_mode(self):
        import pixel_server as ps
        calls = []

        class Boom(cap.CaptureBackend):
            name, app_rendered = "printwindow", True

            def acquire(self, window):
                calls.append(window)
                raise AssertionError("captured in synthetic mode")

        class BoomOs:
            def __getattr__(self, name):
                raise AssertionError(f"read the operating system ({name}) in synthetic mode")

        clock = fake_os.FakeClock()
        server = ps.PixelServer(ps.ServerConfig(synthetic=True, synthetic_size=(64, 48)),
                                ps.Deps(clock=clock, os=BoomOs(), backend=Boom(), admission=fake_os.full_admission(),
                                        sender=fake_os.FakeSender()))
        for _ in range(3):
            result = server.call_tool("screenshot", {})
            self.assertEqual(result["content"][0]["type"], "image")
        self.assertEqual(server.call_tool("click", {"x": 1, "y": 1})["content"][0]["text"], '{"status": "simulated"}')
        self.assertEqual(server.call_tool("type_text", {"text": "hola"})["content"][0]["text"], '{"status": "simulated"}')
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
