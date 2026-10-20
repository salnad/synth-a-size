#!/usr/bin/env python3
"""Regression tests for run_corpus.py (stdlib unittest; no reference build).

  python3 -m unittest discover -s migration/scripts -p 'test_*.py'

The resolver cases mirror port/crates/parity/src/cases.rs; both must accept
and reject the same inputs.
"""

import contextlib
import io
import json
import os
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_corpus as rc  # noqa: E402

LEDGER = """# Ledger

## 2026-10-21: approve looser filters/test/a

Owner approved residual_db -70 for filters/test/a.

## 2026-10-22: unrelated

Nothing.
"""

MODULE_DEFAULTS = {"sample_rate": 44100, "block_size": 64, "oversample": 1,
                   "tolerances": {"max_abs_error": 1e-4, "residual_db": -80.0}}
RENDER_TOLERANCES = {"rms_diff_db": 0.01, "residual_db": -50.0, "lsd_db": 0.1}
IDENTITY = {"upstream_sha": "abc", "compiler": "13.3.0", "glibc": "2.39", "fast_math": True,
            "opt_flags": "-O3", "arch": "x86_64", "libm_variants": "baseline"}


def approved(tolerances):
    return {"approved_in": "2026-10-21: approve looser filters/test/a", "tolerances": tolerances}


def resolve(data, defaults=None):
    return rc.resolve_module_file("filters", "test", "x.json", data, defaults or MODULE_DEFAULTS, LEDGER)


def cases_json(render_tolerances=None, render=None):
    return {"defaults": {"sample_rate": 44100, "block_size": 64, "tail": 2.0, "bpm": 120, "seed": 0,
                         "tolerances": render_tolerances or RENDER_TOLERANCES},
            "renders": [render or {"name": "r", "preset": "p.vital", "midi": "m.mid"}],
            "module_defaults": MODULE_DEFAULTS}


class ToleranceRules(unittest.TestCase):
    def assertCaseError(self, fragment, function, *args):
        with self.assertRaises(rc.CaseError) as raised:
            function(*args)
        self.assertIn(fragment, str(raised.exception))

    def test_layering_and_defaults(self):
        cases = resolve({"module": "filter", "block_size": 32,
                         "cases": {"a": {"num_samples": 10, "block_size": 16}, "b": {"num_samples": 5}}})
        self.assertEqual(cases[0]["spec"]["block_size"], 16)
        self.assertEqual(cases[1]["spec"]["block_size"], 32)
        self.assertEqual(cases[1]["tolerances"], {"max_abs_error": 1e-4, "residual_db": -80.0})

    def test_looser_without_exception(self):
        self.assertCaseError("looser", resolve, {"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerances": {"residual_db": -70}}}})

    # B1: the reviewer's free-text "x" unlocked residual_db -10 / max_abs_error 1.0.
    def test_free_text_exception_rejected(self):
        self.assertCaseError("tolerance_exception must be", resolve, {"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerances": {"residual_db": -10, "max_abs_error": 1.0},
            "tolerance_exception": "x"}}})

    # B1: a file-level exception covered every (future) case.
    def test_file_level_exception_rejected(self):
        self.assertCaseError("case-level only", resolve, {
            "module": "filter", "tolerance_exception": approved({"residual_db": 0}),
            "cases": {"a": {"num_samples": 1, "tolerances": {"residual_db": 0}}}})

    # B1: an approved case could be loosened further.
    def test_loosening_past_approval_rejected(self):
        self.assertCaseError("differs from the approved value", resolve, {"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerances": {"residual_db": -60},
            "tolerance_exception": approved({"residual_db": -70})}}})

    def test_exact_approval_accepted(self):
        cases = resolve({"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerances": {"residual_db": -70},
            "tolerance_exception": approved({"residual_db": -70})}}})
        self.assertEqual(cases[0]["tolerances"]["residual_db"], -70.0)

    # B1: nothing checked that the approval exists in LEDGER.md.
    def test_ledger_heading_must_exist_and_name_case(self):
        self.assertCaseError("does not exist", resolve, {"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerances": {"residual_db": -70},
            "tolerance_exception": {"approved_in": "2026-10-21: no such heading",
                                    "tolerances": {"residual_db": -70}}}}})
        self.assertCaseError("does not name the case", resolve, {"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerances": {"residual_db": -70},
            "tolerance_exception": {"approved_in": "2026-10-22: unrelated",
                                    "tolerances": {"residual_db": -70}}}}})
        self.assertCaseError("does not name the case", resolve, {"module": "filter", "cases": {"b": {
            "num_samples": 1, "tolerances": {"residual_db": -70},
            "tolerance_exception": approved({"residual_db": -70})}}})

    # Reviewer note 2: the case id must match on token boundaries.
    def test_ledger_names_case_as_whole_token(self):
        cases = [
            ("Owner approved filters/test/a.", True),
            ("approved `filters/test/a` today", True),
            ("filters/test/a", True),
            ("see filters/test/a:value", True),
            ("(filters/test/a)", True),
            ("only filters/test/ab is approved", False),
            ("xfilters/test/a", False),
            ("filters/test/a_2", False),
            ("filters/test/a-b", False),
            ("filters/test/a/extra", False),
            ("other/filters/test/a", False),
            ("filters/test/ab and later filters/test/a", True),
        ]
        for text, expected in cases:
            self.assertEqual(rc.names_case(text, "filters/test/a"), expected, text)
        ledger = "## 2026-10-23: approve\n\nOwner approved filters/test/ab only.\n"
        with self.assertRaises(rc.CaseError) as raised:
            rc.resolve_module_file("filters", "test", "x.json", {"module": "filter", "cases": {"a": {
                "num_samples": 1, "tolerances": {"residual_db": -70},
                "tolerance_exception": {"approved_in": "2026-10-23: approve",
                                        "tolerances": {"residual_db": -70}}}}}, MODULE_DEFAULTS, ledger)
        self.assertIn("does not name the case", str(raised.exception))

    # Reviewer nit: "a.b" or "a:x" would contain an approval for "a" across a
    # token boundary, so names are restricted at load.
    def test_names_restricted_to_id_characters(self):
        for name in ["a.b", "a:x", "a b", "a/b", "", "\u00e4"]:
            self.assertCaseError("must match [A-Za-z0-9_-]+", resolve,
                                 {"module": "filter", "cases": {name: {"num_samples": 1}}})
        resolve({"module": "filter", "cases": {"Case_2-b": {"num_samples": 1}}})
        ok = {"module": "filter", "cases": {"a": {"num_samples": 1}}}
        for area, module in [("filters", "dig.svf"), ("filters", "svf:x"), ("fil.ters", "svf"), ("", "svf")]:
            self.assertCaseError("must match [A-Za-z0-9_-]+", rc.resolve_module_file,
                                 area, module, "x.json", ok, MODULE_DEFAULTS, LEDGER)
        for name in ["init.saw", "init:saw", ""]:
            self.assertCaseError("must match [A-Za-z0-9_-]+", rc.resolve_renders,
                                 cases_json(render={"name": name, "preset": "p", "midi": "m"}), LEDGER)

    def test_existing_case_files_comply(self):
        rc.load_modules()
        rc.load_renders()

    def test_exception_covers_exactly_loosened_metrics(self):
        self.assertCaseError("does not loosen it", resolve, {"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerance_exception": approved({"residual_db": -70})}}})
        self.assertCaseError("loosened without approval", resolve, {"module": "filter", "cases": {"a": {
            "num_samples": 1, "tolerances": {"residual_db": -70, "max_abs_error": 1e-3},
            "tolerance_exception": approved({"residual_db": -70})}}})

    # B2: loosening module_defaults or dropping a metric passed silently.
    def test_module_floor(self):
        data = {"module": "filter", "cases": {"a": {"num_samples": 1}}}
        self.assertCaseError("missing max_abs_error", resolve, data,
                             dict(MODULE_DEFAULTS, tolerances={"residual_db": -20}))
        self.assertCaseError("looser than the approved floor", resolve, data,
                             dict(MODULE_DEFAULTS, tolerances={"max_abs_error": 1e-4, "residual_db": -20}))
        stricter = resolve(data, dict(MODULE_DEFAULTS, tolerances={"max_abs_error": 1e-5, "residual_db": -90}))
        self.assertEqual(stricter[0]["tolerances"]["residual_db"], -90.0)

    def test_render_floor(self):
        rc.resolve_renders(cases_json(), LEDGER)
        self.assertCaseError("lsd_db", rc.resolve_renders,
                             cases_json(dict(RENDER_TOLERANCES, lsd_db=0.5)), LEDGER)
        self.assertCaseError("missing lsd_db", rc.resolve_renders,
                             cases_json({"rms_diff_db": 0.01, "residual_db": -50.0}), LEDGER)

    # N4
    def test_render_unknown_keys_and_nan(self):
        self.assertCaseError("unknown keys", rc.resolve_renders,
                             cases_json(render={"name": "r", "preset": "p", "midi": "m", "tolerance": {}}), LEDGER)
        self.assertCaseError("NaN is not allowed", rc.loads_strict, '{"residual_db": NaN}', "x")
        self.assertCaseError("Infinity is not allowed", rc.loads_strict, '{"residual_db": Infinity}', "x")

    def test_ledger_sections_ignore_fences(self):
        sections = rc.ledger_sections("## A\nx\n```\n## not a heading\n```\n### B\ny\n## C\nz\n")
        self.assertEqual([t for t, _ in sections], ["A", "B", "C"])
        self.assertIn("## not a heading", sections[0][1])


class Selection(unittest.TestCase):
    # N5: --only matched substrings.
    def test_only_matches_exact_ids_or_path_prefixes(self):
        self.assertTrue(rc.selected("filters/digital_svf/sweep", ["filters/digital_svf"]))
        self.assertTrue(rc.selected("filters/digital_svf/sweep", ["filters/digital_svf/sweep"]))
        self.assertTrue(rc.selected("filters/digital_svf/sweep", ["filters/"]))
        self.assertFalse(rc.selected("filters/digital_svf_2/sweep", ["filters/digital_svf"]))
        self.assertFalse(rc.selected("filters/digital_svf/sweep", ["digital_svf"]))
        self.assertFalse(rc.selected("render/init_saw__chords", ["render/init_saw"]))
        self.assertTrue(rc.selected("anything", None))

    def test_fnv_vectors_match_sas_parity(self):
        self.assertEqual(rc.fnv1a64(b""), "14650fb0739d0383")
        self.assertEqual(rc.fnv1a64(b"a"), "44bd8ad473cd9906")


def write_npy(path, values, channels):
    header = "{'descr': '<f4', 'fortran_order': False, 'shape': (%d, %d), }" % (len(values) // channels, channels)
    header += " " * ((64 - (10 + len(header) + 1) % 64) % 64) + "\n"
    payload = struct.pack("<%df" % len(values), *values)
    with open(path, "wb") as handle:
        handle.write(b"\x93NUMPY\x01\x00" + struct.pack("<H", len(header)) + header.encode() + payload)
    return rc.fnv1a64(payload)


def write_wav(path, values):
    payload = struct.pack("<%df" % len(values), *values)
    fmt = struct.pack("<HHIIHH", 3, 2, 44100, 44100 * 8, 8, 32)
    data = b"WAVE" + b"fmt " + struct.pack("<I", len(fmt)) + fmt + b"data" + struct.pack("<I", len(payload)) + payload
    with open(path, "wb") as handle:
        handle.write(b"RIFF" + struct.pack("<I", len(data)) + data)
    return rc.fnv1a64(payload)


class ScratchRepo(unittest.TestCase):
    """A minimal repository with one render case and one module case."""

    def setUp(self):
        self.addCleanup(rc.set_repo, rc.REPO)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = self.tmp.name
        os.makedirs(os.path.join(root, "migration", "corpus", "modules", "filters"))
        with open(os.path.join(root, "migration", "LEDGER.md"), "w") as handle:
            handle.write(LEDGER)
        rc.set_repo(root)
        rc.write_json(os.path.join(rc.CORPUS, "cases.json"), cases_json())
        rc.write_json(os.path.join(rc.MODULE_CASES, "filters", "test.json"),
                      {"module": "filter", "cases": {"a": {"num_samples": 4}}})

    def run_main(self, *argv):
        output = io.StringIO()
        old = sys.argv
        sys.argv = ["run_corpus.py"] + list(argv)
        try:
            with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
                code = rc.main()
        finally:
            sys.argv = old
        return code, output.getvalue()

    def make_tree(self, identity=None):
        tree = os.path.join(self.tmp.name, "tree")
        renders = os.path.join(tree, "renders")
        module = os.path.join(tree, "modules", "filters", "test", "a")
        os.makedirs(renders, exist_ok=True)
        os.makedirs(module, exist_ok=True)
        build = dict(identity or IDENTITY)
        digest = write_wav(os.path.join(renders, "r.wav"), [0.0, 0.1, 0.2, 0.3])
        rc.write_json(os.path.join(renders, "r.wav.json"), {"build": build, "samples_fnv1a64": digest})
        digest = write_npy(os.path.join(module, "out_audio.npy"), [0.5] * 16, 4)
        with open(os.path.join(module, "artifact_saved.json"), "wb") as handle:
            handle.write(b'{"a": 1}\n')
        rc.write_json(os.path.join(module, "capture.json"), {
            "build": build,
            "outputs": {"audio": {"file": "out_audio.npy", "fnv1a64": digest}},
            "artifacts": {"saved.json": {"file": "artifact_saved.json", "fnv1a64": rc.fnv1a64(b'{"a": 1}\n')}},
        })
        return tree

    def write_baseline(self, tree):
        code, output = self.run_main("hashes", tree, "--write", "--new-identity")
        self.assertEqual(code, 0, output)

    def test_hashes_round_trip_including_artifacts(self):
        tree = self.make_tree()
        self.write_baseline(tree)
        with open(os.path.join(rc.MODULE_CASES, "filters", "test.hashes.json")) as handle:
            hashes = json.load(handle)["entries"][0]["hashes"]
        self.assertEqual(sorted(hashes), ["a:artifact/saved.json", "a:audio"])
        code, output = self.run_main("hashes", tree, "--check", "--require-known-identity")
        self.assertEqual(code, 0, output)

    # B4: a module hash file without the CI identity was skipped with exit 0.
    def test_module_hash_file_missing_ci_identity_fails(self):
        tree = self.make_tree()
        self.write_baseline(tree)
        path = os.path.join(rc.MODULE_CASES, "filters", "test.hashes.json")
        with open(path) as handle:
            data = json.load(handle)
        data["entries"][0]["identity"]["compiler"] = "14.2.0"
        rc.write_json(path, data)
        code, output = self.run_main("hashes", tree, "--check")
        self.assertEqual(code, 1, output)
        self.assertIn("has no hashes for the CI build identity", output)

    # N3: an unknown build identity passed silently.
    def test_unknown_identity(self):
        tree = self.make_tree()
        self.write_baseline(tree)
        other = self.make_tree(dict(IDENTITY, compiler="14.2.0"))
        code, output = self.run_main("hashes", other, "--check")
        self.assertEqual(code, 0, output)
        self.assertIn("skipped", output)
        code, output = self.run_main("hashes", other, "--check", "--require-known-identity")
        self.assertEqual(code, 1, output)
        code, output = self.run_main("hashes", other, "--write")
        self.assertEqual(code, 2, output)
        self.assertIn("not a known CI identity", output)

    # N1: hashes were read from capture.json instead of the files.
    def test_tampered_files_are_detected(self):
        tree = self.make_tree()
        self.write_baseline(tree)
        npy = os.path.join(tree, "modules", "filters", "test", "a", "out_audio.npy")
        with open(npy, "r+b") as handle:
            handle.seek(-4, os.SEEK_END)
            handle.write(b"\x00\x00\x00\x00")
        with open(os.path.join(tree, "modules", "filters", "test", "a", "artifact_saved.json"), "wb") as handle:
            handle.write(b'{"a": 2}\n')
        code, output = self.run_main("hashes", tree, "--check")
        self.assertEqual(code, 1, output)
        self.assertIn("out_audio.npy does not match capture.json", output)
        self.assertIn("artifact saved.json does not match capture.json", output)
        self.assertIn("MISMATCH", output)

    def make_text_only_trees(self):
        """Reference and test trees for an artifact-only module case."""
        os.makedirs(os.path.join(rc.MODULE_CASES, "harness"), exist_ok=True)
        rc.write_json(os.path.join(rc.MODULE_CASES, "harness", "textonly.json"),
                      {"module": "harness_echo", "cases": {"json": {}}})
        trees = []
        for name in ("ref", "test"):
            case_dir = os.path.join(self.tmp.name, name, "modules", "harness", "textonly", "json")
            os.makedirs(case_dir)
            with open(os.path.join(case_dir, "artifact_saved.json"), "wb") as handle:
                handle.write(b'{"version": "1.0.6"}\n')
            trees.append((os.path.join(self.tmp.name, name), case_dir))
        # audio-diff must never run for text artifacts: make it fail loudly.
        fake_diff = os.path.join(self.tmp.name, "fake-audio-diff")
        with open(fake_diff, "w") as handle:
            handle.write("#!/bin/sh\necho audio-diff should not run\nexit 2\n")
        os.chmod(fake_diff, 0o755)
        return trees, fake_diff

    # Bug from PR #4 CI: compare reported "(no reference outputs)" for
    # artifact-only modules, breaking the determinism and calibration steps.
    def test_compare_artifact_only_module(self):
        ((ref, _), (test, test_case)), fake_diff = self.make_text_only_trees()
        for extra in ([], ["--exact"]):
            code, output = self.run_main("compare", ref, test, "--audio-diff", fake_diff,
                                         "--only", "harness/textonly", *extra)
            self.assertEqual(code, 0, output)
            self.assertIn("pass  harness/textonly/json:artifact/saved.json (exact)", output)
            self.assertNotIn("no reference outputs", output)

        with open(os.path.join(test_case, "artifact_saved.json"), "wb") as handle:
            handle.write(b'{"version": "1.0.7"}\n')
        code, output = self.run_main("compare", ref, test, "--audio-diff", fake_diff, "--only", "harness/textonly")
        self.assertEqual(code, 1, output)
        self.assertIn("FAIL  harness/textonly/json:artifact/saved.json (exact)", output)

        os.remove(os.path.join(test_case, "artifact_saved.json"))
        code, output = self.run_main("compare", ref, test, "--audio-diff", fake_diff, "--only", "harness/textonly")
        self.assertEqual(code, 2, output)
        self.assertIn("missing in test tree", output)

    def test_compare_flags_extra_and_absent_artifacts(self):
        ((ref, ref_case), (test, test_case)), fake_diff = self.make_text_only_trees()
        with open(os.path.join(test_case, "artifact_extra.txt"), "wb") as handle:
            handle.write(b"extra\n")
        code, output = self.run_main("compare", ref, test, "--audio-diff", fake_diff, "--only", "harness/textonly")
        self.assertEqual(code, 2, output)
        self.assertIn("artifact/extra.txt (missing in reference tree)", output)

        os.remove(os.path.join(ref_case, "artifact_saved.json"))
        os.remove(os.path.join(test_case, "artifact_extra.txt"))
        code, output = self.run_main("compare", ref, test, "--audio-diff", fake_diff, "--only", "harness/textonly")
        self.assertEqual(code, 2, output)
        self.assertIn("(no reference outputs or artifacts)", output)

    # B3: a failed render left stale data plus a new case.json.
    def test_failed_render_leaves_no_stale_capture(self):
        tree = self.make_tree()
        renders = os.path.join(tree, "renders")
        module = os.path.join(tree, "modules", "filters", "test", "a")
        rc.write_json(os.path.join(renders, "r.case.json"), {"old": True})
        rc.write_json(os.path.join(module, "case.json"), {"old": True})
        failing = os.path.join(self.tmp.name, "failing-tool")
        with open(failing, "w") as handle:
            handle.write("#!/bin/sh\necho simulated failure\nexit 1\n")
        os.chmod(failing, 0o755)
        code, output = self.run_main("render", "--out", tree, "--ref-render", failing, "--ref-module", failing)
        self.assertEqual(code, 2, output)
        for stale in ("r.wav", "r.wav.json", "r.case.json"):
            self.assertFalse(os.path.exists(os.path.join(renders, stale)), stale)
        for stale in ("case.json", "capture.json", "out_audio.npy", "artifact_saved.json"):
            self.assertFalse(os.path.exists(os.path.join(module, stale)), stale)


if __name__ == "__main__":
    unittest.main()
