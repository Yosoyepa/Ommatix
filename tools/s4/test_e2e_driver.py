"""Regression tests for the S4 driver's activation target (no window is activated here, and no
input is sent). Run with: python -m unittest discover -s tools/s4 -p "test_*.py"
"""

import importlib.util
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("s4_e2e_driver", HERE / "e2e_driver.py")
driver = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(driver)


class PickMainWindowTest(unittest.TestCase):
    def test_the_event_target_helper_is_not_the_main_window(self):
        # winit's hidden event target is listed first and is larger than the window itself
        windows = [(1, "Winit Thread Event Target", 1920 * 1080),
                   (2, "Window Class", 1936 * 1048)]
        self.assertEqual(driver.pick_main_window(windows), 2)

    def test_the_largest_non_helper_window_is_the_main_one(self):
        # a UWP application is hosted by an ApplicationFrameWindow; the IME window is a helper
        windows = [(7, "IME", 4000), (8, "ApplicationFrameWindow", 1000), (9, "Other", 500)]
        self.assertEqual(driver.pick_main_window(windows), 8)

    def test_no_main_window_means_no_activation_target(self):
        self.assertIsNone(driver.pick_main_window([(3, "MSCTFIME UI", 10)]))
        self.assertIsNone(driver.pick_main_window([]))


if __name__ == "__main__":
    unittest.main()
