#!/usr/bin/env python3
"""Tests for gui_reference.py's layout path list."""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gui_reference  # noqa: E402


class LayoutPathsTest(unittest.TestCase):
    def paths(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            listing = Path(tmp) / "layout_paths.txt"
            listing.write_text(text)
            original = gui_reference.LAYOUT_PATHS
            gui_reference.LAYOUT_PATHS = listing
            try:
                return gui_reference.layout_paths()
            finally:
                gui_reference.LAYOUT_PATHS = original

    def test_preset_applies_to_the_paths_after_it(self):
        preset = "migration/corpus/gui/presets/all_effects.vital"
        self.assertEqual(
            self.paths(f"# comment\nheader_section\n\n@preset {preset}\neffects/container/CHORUS\n"),
            [("header_section", None), ("effects/container/CHORUS", preset)],
        )

    def test_a_missing_preset_fails(self):
        with self.assertRaises(SystemExit):
            self.paths("@preset migration/corpus/gui/presets/missing.vital\nheader_section\n")


if __name__ == "__main__":
    unittest.main()
