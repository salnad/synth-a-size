#!/usr/bin/env python3
"""Tests for gui_reference.py's layout path list and layout dump reading."""

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

    def test_a_path_listed_twice_fails(self):
        preset = "migration/corpus/gui/presets/all_effects.vital"
        with self.assertRaises(SystemExit):
            self.paths(f"effects\n@preset {preset}\neffects\n")

    def test_presets_come_from_the_gui_preset_folder(self):
        with self.assertRaises(SystemExit):
            self.paths("@preset migration/corpus/presets/init_saw.vital\nheader_section\n")
        with self.assertRaises(SystemExit):
            self.paths("@preset migration/corpus/gui/presets/../../presets/init_saw.vital\neffects\n")

    def test_a_missing_preset_fails(self):
        with self.assertRaises(SystemExit):
            self.paths("@preset migration/corpus/gui/presets/missing.vital\nheader_section\n")


class LayoutForTest(unittest.TestCase):
    def test_repeated_names_are_numbered_in_tree_order(self):
        dump = [
            '{"path": "macro/macro_control_1", "visible": true, "bounds": [0, 0, 68, 68]}',
            '{"path": "macro/Label", "visible": true, "bounds": [0, 69, 68, 18]}',
            '{"path": "macro/macro_control_1", "visible": false, "bounds": [0, 68, 68, 69]}',
        ]

        def run_ref_gui(args):
            Path(args[args.index("--layout") + 1]).write_text("\n".join(dump) + "\n")

        original = gui_reference.run_ref_gui
        gui_reference.run_ref_gui = run_ref_gui
        try:
            laid_out = gui_reference.layout_for(
                1400, 820, ["macro/macro_control_1", "macro/macro_control_1#2"])
        finally:
            gui_reference.run_ref_gui = original
        self.assertEqual(laid_out, {
            "macro/macro_control_1": {"bounds": [0, 0, 68, 68], "visible": True},
            "macro/macro_control_1#2": {"bounds": [0, 68, 68, 69], "visible": False},
        })


class LiveArgsTest(unittest.TestCase):
    MIDI = "migration/corpus/midi/held_pad.mid"

    def test_a_view_without_an_engine_adds_nothing(self):
        self.assertEqual(gui_reference.live_args({"name": "v", "tab": "voice"}), [])

    def test_midi_seconds_and_animate(self):
        self.assertEqual(
            gui_reference.live_args({"name": "v", "midi": self.MIDI, "seconds": 1.5, "animate": True}),
            ["--midi", str(gui_reference.REPO / self.MIDI), "--seconds", "1.5", "--animate", "on"],
        )
        self.assertEqual(gui_reference.live_args({"name": "v", "seconds": 0, "animate": False}),
                         ["--seconds", "0.0"])

    def test_animate_needs_a_running_engine(self):
        with self.assertRaises(SystemExit):
            gui_reference.live_args({"name": "v", "animate": True})

    def test_midi_comes_from_the_corpus_midi_folder(self):
        for midi in ("migration/corpus/gui/presets/all_effects.vital",
                     "migration/corpus/midi/../gui/presets/all_effects.vital",
                     "migration/corpus/midi/missing.mid"):
            with self.assertRaises(SystemExit):
                gui_reference.live_args({"name": "v", "midi": midi})

    def test_seconds_must_be_a_non_negative_number(self):
        for seconds in (-1, "2", True):
            with self.assertRaises(SystemExit):
                gui_reference.live_args({"name": "v", "seconds": seconds})


if __name__ == "__main__":
    unittest.main()
