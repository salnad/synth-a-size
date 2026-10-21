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
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ledger  # noqa: E402
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
            "opt_flags": "-O3 -ffast-math -U__FAST_MATH__ -march=x86-64", "arch": "x86_64",
            "libm_variants": "baseline"}


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

    # Parity Reviewer on #50: "approved_in": "Ledger" was accepted, because the
    # "# Ledger" section spans the whole file (and then every entry file).
    def test_approved_in_must_be_a_dated_heading_within_one_file(self):
        def case(heading):
            return {"num_samples": 1, "tolerances": {"residual_db": -60},
                    "tolerance_exception": {"approved_in": heading, "tolerances": {"residual_db": -60}}}

        defaults = MODULE_DEFAULTS["tolerances"]
        frozen = "# Ledger\n\nIntro.\n\n## 2026-10-20: old\n\nHistory.\n"
        progress = "## 2026-10-22: comb filter work\n\nfilters/test/a measured -60 dB.\n"
        resolve = lambda heading, files: rc.resolve_tolerances("filters/test/a", defaults, {}, case(heading), files)  # noqa: E731
        self.assertCaseError("must cite a dated ledger heading", resolve, "Ledger", [frozen, progress])
        self.assertCaseError("must cite a dated ledger heading", resolve, "Ledger", rc.read_ledger())
        undated = "## Approval for filters/test/a\n\nfilters/test/a approved.\n"
        self.assertCaseError("must cite a dated ledger heading", resolve, "Approval for filters/test/a",
                             [frozen, undated])
        # A dated level-1 heading cannot reach into the next file.
        notes = "# 2026-10-22: notes\n\nNothing approved.\n"
        self.assertCaseError("does not name the case", resolve, "2026-10-22: notes", [frozen, notes, progress])
        approval = "## 2026-10-23: approve looser filters/test/a\n\nOwner approved filters/test/a.\n"
        tolerances, _ = resolve("2026-10-23: approve looser filters/test/a", [frozen, approval])
        self.assertEqual(tolerances["residual_db"], -60.0)

    def test_ledger_sections_ignore_fences(self):
        sections = rc.ledger_sections("## A\nx\n```\n## not a heading\n```\n### B\ny\n## C\nz\n")
        self.assertEqual([t for t, _ in sections], ["A", "B", "C"])
        self.assertIn("## not a heading", sections[0][1])


ALLOWED = "filters/test/morph"
LEGACY_DECISION = """# Owner decisions

## No-fast-math exact cases (allowlist)

- `%s` (test)

## Something else

- `filters/test/a`
""" % ALLOWED
APPROVAL_HEADING = rc.NOFASTMATH_APPROVALS["SAS-35"]
APPROVAL_ENTRY = "## %s\n\nSAS-35: approved.\n" % APPROVAL_HEADING
ALLOWLIST_FILE = "# filters/test\n\nProse naming `filters/test/other` is ignored.\n\n" \
                 "- `%s` (SAS-35; x86): noise-level phases\n" % ALLOWED


def f32(value, ulps=0):
    """value rounded to f32, moved by `ulps` representable steps."""
    bits = struct.unpack("<I", struct.pack("<f", value))[0] + ulps
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def b64_f32(values):
    import base64
    return base64.b64encode(struct.pack("<%df" % len(values), *values)).decode()


def json_problems(ref, test, keys=()):
    return rc.json_bound_problems(json.dumps(ref).encode(), json.dumps(test).encode(), rc.JSON_BOUND, keys)


class ComparisonRules(unittest.TestCase):
    """Owner decisions of 2026-10-21 (JSON bound, no-fast-math exact cases)."""

    def resolve(self, case, file_level=None, allowlist=()):
        data = dict(file_level or {}, module="filter", cases={"morph": case})
        return rc.resolve_module_file("filters", "test", "x.json", data, MODULE_DEFAULTS, LEDGER, allowlist)[0]

    def assertCaseError(self, fragment, function, *args, **kwargs):
        with self.assertRaises(rc.CaseError) as raised:
            function(*args, **kwargs)
        self.assertIn(fragment, str(raised.exception))

    def test_undeclared_cases_resolve_as_before(self):
        entry = self.resolve({"num_samples": 1})
        self.assertEqual(sorted(entry), ["file", "id", "spec", "tolerances"])

    def test_json_artifacts_get_the_pinned_bound(self):
        entry = self.resolve({"artifacts": {"saved.json": "json", "notes.txt": "exact"},
                              "base64_f32_keys": ["wave_data"]})
        self.assertEqual(entry["artifacts"], {"saved.json": "json", "notes.txt": "exact"})
        self.assertEqual(entry["json_bound"], {"ulp": 4, "abs": 1e-6})
        self.assertEqual(entry["base64_f32_keys"], ["wave_data"])
        tighter = self.resolve({"json_bound": {"ulp": 0, "abs": 0}}, {"artifacts": {"saved.json": "json"}})
        self.assertEqual(tighter["json_bound"], {"ulp": 0, "abs": 0.0})
        self.assertNotIn("json_bound", self.resolve({"artifacts": {"saved.json": "exact"}}))

    def test_json_bound_cannot_be_loosened(self):
        json_artifact = {"artifacts": {"saved.json": "json"}}
        self.assertCaseError("looser than the approved JSON bound", self.resolve,
                             dict(json_artifact, json_bound={"ulp": 5}))
        self.assertCaseError("looser than the approved JSON bound", self.resolve,
                             dict(json_artifact, json_bound={"abs": 1e-5}))
        self.assertCaseError("looser than the approved JSON bound", self.resolve,
                             {"num_samples": 1}, dict(json_artifact, json_bound={"ulp": 96}))
        self.assertCaseError("json_bound.ulp must be a non-negative integer", self.resolve,
                             dict(json_artifact, json_bound={"ulp": 2.5}))
        self.assertCaseError("json_bound must be", self.resolve, dict(json_artifact, json_bound={"rel": 1}))
        self.assertCaseError("mode must be one of", self.resolve, {"artifacts": {"saved.json": "loose"}})

    def test_nofastmath_needs_the_decision_doc_allowlist(self):
        self.assertCaseError("only allowed for case ids listed", self.resolve, {"reference": "nofastmath"})
        self.assertCaseError("only allowed for case ids listed", self.resolve, {"reference": "nofastmath"},
                             allowlist=["filters/test/morph_2", "filters/test"])
        entry = self.resolve({"reference": "nofastmath", "artifacts": {"saved.json": "json"}},
                             {"tolerances": {"residual_db": -120}}, allowlist=[ALLOWED])
        self.assertEqual(entry["reference"], "nofastmath")
        # The declared modes stay: they are the normal limits calibration uses.
        self.assertEqual(entry["json_bound"], rc.JSON_BOUND)
        # Normal limits are the module defaults; file-level tightening is ignored.
        self.assertEqual(entry["tolerances"], MODULE_DEFAULTS["tolerances"])
        self.assertCaseError("cannot set tolerance_exception", self.resolve,
                             {"reference": "nofastmath", "tolerance_exception": approved({"residual_db": -70})},
                             allowlist=[ALLOWED])
        self.assertCaseError("cannot set tolerances", self.resolve,
                             {"reference": "nofastmath", "tolerances": {"max_abs_error": 1e-12}},
                             allowlist=[ALLOWED])
        self.assertCaseError("unknown keys ['reference']", self.resolve, {"num_samples": 1},
                             {"reference": "nofastmath"}, allowlist=[ALLOWED])
        self.assertCaseError("reference must be one of", self.resolve, {"reference": "x86"})
        self.assertEqual(self.resolve({"reference": "official"}).get("reference"), None)

    # Reviewer note on #34: declaring "audio_file" let int16 PCM pass the f32
    # bound (reinterpreted words are denormals within 1e-6).
    def test_only_pinned_base64_f32_keys_can_be_declared(self):
        json_artifact = {"artifacts": {"saved.json": "json"}}
        self.assertCaseError("cannot be declared in base64_f32_keys", self.resolve,
                             dict(json_artifact, base64_f32_keys=["audio_file"]))
        self.assertCaseError("cannot be declared in base64_f32_keys", self.resolve,
                             dict(json_artifact, base64_f32_keys=["wave_data", "x"]))
        self.assertCaseError("cannot be declared in base64_f32_keys", self.resolve,
                             json_artifact, {"base64_f32_keys": ["audio_file"]})
        self.assertEqual(rc.BASE64_F32_KEYS, ("wave_data",))
        # The reviewer's repro: these PCM strings pass only if reinterpreted.
        pcm = lambda v: __import__("base64").b64encode(struct.pack("<4h", *v)).decode()  # noqa: E731
        ref, test = {"audio_file": pcm([5, 0, 1000, 0])}, {"audio_file": pcm([30000, 0, -1000, 0])}
        self.assertEqual(json_problems(ref, test, ["audio_file"]), [])
        self.assertNotEqual(json_problems(ref, test), [])

    # Reviewer note on #34: a tightened json_bound made the official
    # reference "fail" the justification on purpose.
    def test_nofastmath_cases_always_use_the_pinned_json_bound(self):
        entry = self.resolve({"reference": "nofastmath"},
                             {"artifacts": {"saved.json": "json"}, "json_bound": {"ulp": 0, "abs": 0}},
                             allowlist=[ALLOWED])
        self.assertEqual(entry["json_bound"], rc.JSON_BOUND)
        self.assertCaseError("cannot set json_bound", self.resolve, {"reference": "nofastmath",
                             "json_bound": {"ulp": 0}}, {"artifacts": {"saved.json": "json"}}, allowlist=[ALLOWED])

    def test_allowlist_additions_must_be_loaded_nofastmath_cases(self):
        check = rc.allowlist_addition_problems
        head = {ALLOWED: ("SAS-35", ("x86",)), "filters/test/comb": ("SAS-35", ("macos",))}
        self.assertEqual(check({ALLOWED}, head, {ALLOWED, "filters/test/comb"}), [])
        # Staged ahead of its case, or still parked in pending/.
        problems = check({ALLOWED}, head, {ALLOWED})
        self.assertEqual(len(problems), 1)
        self.assertIn("filters/test/comb was added", problems[0])
        # Already listed at the base: not an addition. Removals need nothing.
        self.assertEqual(check({ALLOWED, "filters/test/comb"}, head, set()), [])
        self.assertEqual(check({ALLOWED, "filters/test/comb"}, {}, set()), [])

    def test_allowlist_files_are_per_module_and_strict(self):
        parsed = rc.allowlist_from_files({"filters/test.md": ALLOWLIST_FILE, "README.md": "- anything\n"})
        self.assertEqual(parsed, {ALLOWED: ("SAS-35", ("x86",))})
        rc.check_allowlist_approvals(parsed, APPROVAL_ENTRY)
        for path, text, fragment in (
                ("filters/test.md", "- `filters/test/*` (SAS-35; x86): glob\n", "list lines must be"),
                ("filters/test.md", "- `filters/test/morph`: no approval\n", "list lines must be"),
                ("filters/test.md", "- `filters/test/morph` (SAS-35): no target\n", "list lines must be"),
                ("filters/test.md", "- `filters/test/morph` (SAS-35; x86,macos): separator\n", "list lines must be"),
                ("filters/test.md", "- `filters/test/morph` (SAS-35; arm): x\n", "targets must be distinct"),
                ("filters/test.md", "- `filters/test/morph` (SAS-35; x86, x86): x\n", "targets must be distinct"),
                ("filters/test.md", "- `filters/test/morph` (SAS-35; x86):\n", "list lines must be"),
                ("filters/test.md", "  - `filters/test/morph` (SAS-35; x86): indented\n", "list lines must be"),
                ("filters/test.md", "* `filters/test/morph` (SAS-35; x86): star\n", "list lines must be"),
                ("filters/test.md", "- `filters/test/morph/x` (SAS-35; x86): deep\n", "list lines must be"),
                ("filters/test.md", "- `filters/other/morph` (SAS-35; x86): x\n", "lists filters/test cases only"),
                ("filters/test.md", "- `filters/test/morph` (SAS-99; x86): x\n", "not an approval pinned"),
                ("filters/test.txt", "", "only <area>/<module>.md"),
                ("filters/sub/test.md", "", "only <area>/<module>.md"),
                ("test.md", "", "only <area>/<module>.md"),
                ("filters/te.st.md", "", "module name")):
            self.assertCaseError(fragment, rc.allowlist_from_files, {path: text})
        self.assertCaseError("listed twice", rc.allowlist_from_files,
                             {"filters/test.md": ALLOWLIST_FILE + "- `%s` (SAS-35; x86): again\n" % ALLOWED})
        # The cited approval's heading must exist and name the key.
        for ledger_text in (LEDGER, "## %s\n\nSAS-350 only.\n" % APPROVAL_HEADING):
            self.assertCaseError("cites SAS-35", rc.check_allowlist_approvals, parsed, ledger_text)
        self.assertEqual(rc.legacy_allowlist_from(LEGACY_DECISION), [ALLOWED])

    def test_repository_allowlist_parses(self):
        allowlist = rc.read_allowlist(rc.read_ledger())
        self.assertIn("wavetable/render/wave_linear_freq_disjoint_spectra", allowlist)
        self.assertEqual({approval for approval, _ in allowlist.values()}, {"SAS-35"})

    def test_five_ulp_drift_fails_and_four_passes(self):
        # Above |x| = 4 the ulp term is the looser one (4 ulp > 1e-6).
        for value in (1000.3, -6.5, 4.0):
            self.assertEqual(json_problems({"x": f32(value)}, {"x": f32(value, 4)}), [], value)
            problems = json_problems({"x": f32(value)}, {"x": f32(value, 5)})
            self.assertEqual(len(problems), 1, value)
            self.assertIn("$.x", problems[0])

    def test_absolute_floor_is_looser_below_four(self):
        # Below |x| = 4 the 1e-6 floor dominates: 5 ulp of 0.3 is 1.5e-7.
        self.assertEqual(json_problems({"x": f32(0.3)}, {"x": f32(0.3, 5)}), [])
        self.assertNotEqual(json_problems({"x": f32(0.3)}, {"x": f32(0.3) + 1.01e-6}), [])

    def test_negative_zero_equals_zero(self):
        self.assertEqual(json_problems({"x": [-0.0, 0.0]}, {"x": [0.0, -0.0]}), [])
        self.assertEqual(json_problems({"w": b64_f32([-0.0])}, {"w": b64_f32([0.0])}, ["w"]), [])

    def test_absolute_floor_near_zero(self):
        tiny = 1e-30
        self.assertEqual(json_problems({"x": tiny}, {"x": tiny + 7.8e-7}), [])
        self.assertNotEqual(json_problems({"x": tiny}, {"x": tiny + 1.1e-6}), [])

    def test_structure_types_integers_and_strings_are_exact(self):
        ref = {"a": 1, "b": "s", "c": [1.0, 2.0], "d": True, "e": None}
        self.assertEqual(json_problems(ref, dict(ref)), [])
        for change in ({"a": 2}, {"a": 1.0}, {"b": "t"}, {"c": [1.0]}, {"d": False}, {"e": 0}, {"f": 1}):
            self.assertNotEqual(json_problems(ref, dict(ref, **change)), [], change)
        self.assertNotEqual(json_problems(ref, {k: v for k, v in ref.items() if k != "a"}), [])

    def test_formatting_and_key_order_do_not_count(self):
        problems = rc.json_bound_problems(b'{"a": 1, "b": 0.5}', b'{\n  "b": 5e-1,\n  "a": 1\n}\n', rc.JSON_BOUND)
        self.assertEqual(problems, [])
        self.assertNotEqual(rc.json_bound_problems(b'{"a": 1}', b'{"a": NaN}', rc.JSON_BOUND), [])

    def test_base64_f32_one_ulp_passes_five_fails(self):
        values = [0.25, -6.7, 0.3]
        ref = {"wave_data": b64_f32(values)}
        self.assertEqual(json_problems(ref, {"wave_data": b64_f32([f32(v, 1) for v in values])}, ["wave_data"]), [])
        drift = [values[0], f32(values[1], 5), values[2]]
        problems = json_problems(ref, {"wave_data": b64_f32(drift)}, ["wave_data"])
        self.assertEqual(len(problems), 1)
        self.assertIn("$.wave_data[1]", problems[0])
        self.assertIn("5 ulp", problems[0])
        shorter = json_problems(ref, {"wave_data": b64_f32(values[:2])}, ["wave_data"])
        self.assertIn("3 f32 values in the reference, 2 in the port", shorter[0])
        # Undeclared, the same strings compare exactly.
        self.assertNotEqual(json_problems(ref, {"wave_data": b64_f32([f32(v, 1) for v in values])}), [])

    def test_changed_pcm_byte_fails(self):
        import base64
        pcm = struct.pack("<4h", 0, 1000, -1000, 32767)
        changed = pcm[:2] + bytes([pcm[2] ^ 1]) + pcm[3:]
        ref = {"audio_file": base64.b64encode(pcm).decode(), "wave_data": b64_f32([0.5])}
        test = dict(ref, audio_file=base64.b64encode(changed).decode())
        problems = json_problems(ref, test, ["wave_data"])
        self.assertEqual(len(problems), 1)
        self.assertIn("$.audio_file", problems[0])


class LedgerFiles(unittest.TestCase):
    """migration/LEDGER.md (frozen) plus one file per entry in migration/ledger/."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = self.tmp.name
        os.makedirs(os.path.join(self.root, "migration", "ledger"))
        with open(os.path.join(self.root, "migration", "LEDGER.md"), "w") as handle:
            handle.write("# Ledger\n\n## 2026-10-20: old\n\nHistory.\n")

    def entry(self, name, text):
        with open(os.path.join(self.root, "migration", "ledger", name), "w") as handle:
            handle.write(text)

    def test_entries_follow_the_frozen_ledger_in_date_order(self):
        self.entry("2026-10-22-b.md", "## 2026-10-22: b\n\nSecond.\n")
        self.entry("2026-10-21-a.md", "## 2026-10-21: a\n\nFirst.\n")
        self.entry("README.md", "# How to add an entry\n")
        self.assertEqual(ledger.entry_files(self.root), ["2026-10-21-a.md", "2026-10-22-b.md"])
        titles = [title for title, _ in rc.ledger_sections(ledger.read_text(self.root))]
        self.assertEqual(titles, ["Ledger", "2026-10-20: old", "2026-10-21: a", "2026-10-22: b"])
        self.assertEqual(ledger.problems(self.root), [])

    def test_entry_names_and_headings_are_checked(self):
        self.entry("2026-10-21-Bad_Name.md", "## 2026-10-21: x\n")
        self.entry("2026-10-22-wrong-date.md", "## 2026-10-21: x\n")
        self.entry("2026-10-23-no-heading.md", "Some text\n")
        found = ledger.problems(self.root)
        self.assertEqual(len(found), 3, found)
        self.assertIn("YYYY-MM-DD-<slug>.md", found[0])
        self.assertIn("must start with \"## 2026-10-22: <title>\"", found[1])

    def test_frozen_ledger_rejects_any_change_once_the_base_is_frozen(self):
        frozen = "# Ledger\n\n%s Do not edit.\n\n## 2026-10-20: old\n" % ledger.FROZEN_MARKER
        self.assertIs(ledger.frozen_violation(frozen, frozen), False)
        self.assertIs(ledger.frozen_violation(frozen, frozen + "\n## 2026-10-22: appended\n"), True)
        # The change that introduces the freeze header is allowed.
        self.assertIsNone(ledger.frozen_violation("# Ledger\n\n## 2026-10-20: old\n", frozen))

    def test_repository_ledger_carries_the_freeze_marker(self):
        with open(os.path.join(rc.REPO, "migration", "LEDGER.md")) as handle:
            self.assertIn(ledger.FROZEN_MARKER, handle.read())

    def test_repository_ledger_is_valid(self):
        self.assertEqual(ledger.problems(rc.REPO), [])


class ParkRules(unittest.TestCase):
    """Parked cases (migration/corpus/parked_arm64.json) need a recorded owner
    approval and cannot be macos-declared no-fast-math cases (Parity
    Reviewer on #56)."""

    KNOWN = {"filters/x/a", "filters/x/b"}
    FILES = ["# Ledger\n\nfilters/x/a and filters/x/b are mentioned here.\n",
             "## 2026-10-21: owner approval — park\n\nfilters/x/a is parked on macOS.\n",
             "## Park approval\n\nfilters/x/b\n"]

    def resolve(self, cases, allowlist=None, data=None):
        data = data if data is not None else {"description": "d", "cases": cases}
        return rc.resolve_parked(data, "parked_arm64.json", self.KNOWN, allowlist or {}, self.FILES)

    def park(self, heading="2026-10-21: owner approval — park", reason="noisy (A11)"):
        return {"approved_in": heading, "reason": reason}

    def assertCaseError(self, fragment, function, *args, **kwargs):
        with self.assertRaises(rc.CaseError) as raised:
            function(*args, **kwargs)
        self.assertIn(fragment, str(raised.exception))

    def test_approved_park_resolves(self):
        self.assertEqual(self.resolve({"filters/x/a": self.park()}), {"filters/x/a": "noisy (A11)"})
        self.assertEqual(self.resolve({}), {})

    def test_park_needs_a_dated_heading_that_names_the_case(self):
        self.assertCaseError("does not exist", self.resolve, {"filters/x/a": self.park("2026-10-22: nothing")})
        self.assertCaseError("does not name the case", self.resolve, {"filters/x/b": self.park()})
        self.assertCaseError("must cite a dated ledger heading", self.resolve,
                             {"filters/x/b": self.park("Park approval")})
        self.assertCaseError("must cite a dated ledger heading", self.resolve, {"filters/x/a": self.park("Ledger")})

    def test_park_file_shape(self):
        for entry in ("noisy", {"approved_in": "2026-10-21: owner approval — park"},
                      dict(self.park(), extra=1), self.park(reason=""), self.park(reason=3), self.park(heading=None)):
            self.assertCaseError("must be {", self.resolve, {"filters/x/a": entry})
        for data in ({"cases": [], "description": "d"}, {"description": "d"}, {"cases": {}, "extra": 1},
                     {"cases": {}, "description": 3}, []):
            self.assertCaseError("must be {", self.resolve, None, data=data)
        self.assertCaseError("is not a loaded case", self.resolve, {"filters/x/gone": self.park()})

    def test_macos_declared_case_cannot_be_parked(self):
        self.assertCaseError("declared for macos", self.resolve, {"filters/x/a": self.park()},
                             {"filters/x/a": ("SAS-35", ("x86", "macos"))})
        self.assertEqual(self.resolve({"filters/x/a": self.park()}, {"filters/x/a": ("SAS-35", ("x86",))}),
                         {"filters/x/a": "noisy (A11)"})

    def test_repository_park_file_resolves(self):
        ledger_files = rc.read_ledger()
        known = {c["id"] for c in rc.load_renders()} | {c["id"] for c in rc.load_modules()}
        rc.resolve_parked(rc.read_json_strict(os.path.join(rc.CORPUS, "parked_arm64.json")), "parked_arm64.json",
                          known, rc.read_allowlist(ledger_files), ledger_files)


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

    def make_tree(self, identity=None, name="tree"):
        tree = os.path.join(self.tmp.name, name)
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

    def test_compare_flags_array_missing_in_reference(self):
        ((ref, _), (test, test_case)), fake_diff = self.make_text_only_trees()
        write_npy(os.path.join(test_case, "out_extra.npy"), [0.5] * 4, 4)
        code, output = self.run_main("compare", ref, test, "--audio-diff", fake_diff, "--only", "harness/textonly")
        self.assertEqual(code, 2, output)
        self.assertIn("ERROR harness/textonly/json:extra (missing in reference tree)", output)
        self.assertIn("pass  harness/textonly/json:artifact/saved.json (exact)", output)
        self.assertNotIn("audio-diff should not run", output)

    def make_json_trees(self, test_document):
        rc.write_json(os.path.join(rc.MODULE_CASES, "filters", "test.json"), {
            "module": "filter", "artifacts": {"saved.json": "json"}, "base64_f32_keys": ["wave_data"],
            "cases": {"a": {"num_samples": 4}}})
        reference = {"gain": f32(0.3), "wave_data": b64_f32([0.25, -6.7, 0.0])}
        trees = []
        for name, document in (("ref", reference), ("test", test_document)):
            case_dir = os.path.join(self.tmp.name, name, "modules", "filters", "test", "a")
            os.makedirs(case_dir, exist_ok=True)
            with open(os.path.join(case_dir, "artifact_saved.json"), "w") as handle:
                json.dump(document, handle, indent=None if name == "ref" else 4)
            trees.append(os.path.join(self.tmp.name, name))
        fake_diff = os.path.join(self.tmp.name, "fake-audio-diff")
        with open(fake_diff, "w") as handle:
            handle.write("#!/bin/sh\necho audio-diff should not run\nexit 2\n")
        os.chmod(fake_diff, 0o755)
        return trees, fake_diff

    def test_compare_declared_json_artifact_uses_the_bound(self):
        drift = {"gain": f32(0.3, 3), "wave_data": b64_f32([f32(0.25, 1), f32(-6.7, 4), -0.0])}
        (ref, test), fake_diff = self.make_json_trees(drift)
        only = ["--audio-diff", fake_diff, "--only", "filters/test"]
        code, output = self.run_main("compare", ref, test, *only)
        self.assertEqual(code, 0, output)
        self.assertIn("pass  filters/test/a:artifact/saved.json (json bound)", output)
        # Determinism compares stay byte-exact.
        code, output = self.run_main("compare", ref, test, "--exact", *only)
        self.assertEqual(code, 1, output)
        self.assertIn("FAIL  filters/test/a:artifact/saved.json (exact)", output)

        (ref, test), fake_diff = self.make_json_trees(
            {"gain": f32(0.3), "wave_data": b64_f32([0.25, f32(-6.7, 5), 0.0])})
        code, output = self.run_main("compare", ref, test, *only)
        self.assertEqual(code, 1, output)
        self.assertIn("$.wave_data[1]", output)
        self.assertIn("FAIL  filters/test/a:artifact/saved.json (json bound)", output)

    # Preset Registry #7: "Tolerances are achievable" failed on every preset
    # state artifact because the proxy's default-wavetable wave_data differs
    # by 1 ulp in 4 samples and text artifacts were compared byte for byte.
    def test_calibration_compare_applies_the_declared_bound_to_nested_wave_data(self):
        saw = [f32((i - 1024) / 1024.0) for i in range(2048)]
        proxy_saw = list(saw)
        for index in (3, 700, 1500, 2047):
            proxy_saw[index] = f32(saw[index], 1)

        def preset(samples):
            keyframe = {"position": 0, "wave_data": b64_f32(samples)}
            group = {"components": [{"type": "Wave Source", "keyframes": [keyframe]}]}
            return {"settings": {"osc_1_level": 0.7071067690849304,
                                 "wavetables": [{"name": "Init", "groups": [group]}]}, "synth_version": "1.0.6"}

        case = {"module": "preset_state", "cases": {"init": {}}}
        os.makedirs(os.path.join(rc.MODULE_CASES, "preset"), exist_ok=True)
        trees = {}
        for name, samples in (("ref", saw), ("proxy", proxy_saw)):
            case_dir = os.path.join(self.tmp.name, name, "modules", "preset", "state", "init")
            os.makedirs(case_dir)
            with open(os.path.join(case_dir, "artifact_saved.vital"), "w") as handle:
                json.dump(preset(samples), handle)
            trees[name] = os.path.join(self.tmp.name, name)
        fake_diff = os.path.join(self.tmp.name, "fake-audio-diff")
        with open(fake_diff, "w") as handle:
            handle.write("#!/bin/sh\necho audio-diff should not run\nexit 2\n")
        os.chmod(fake_diff, 0o755)
        compare = ["compare", trees["ref"], trees["proxy"], "--audio-diff", fake_diff, "--only", "preset/state"]

        # Undeclared: byte-exact, which is the failure #7 hit.
        rc.write_json(os.path.join(rc.MODULE_CASES, "preset", "state.json"), case)
        code, output = self.run_main(*compare)
        self.assertEqual(code, 1, output)
        self.assertIn("FAIL  preset/state/init:artifact/saved.vital (exact)", output)

        # Declared JSON state with base64-f32 wave_data: the calibration
        # (cross-build) compare passes under the bound...
        rc.write_json(os.path.join(rc.MODULE_CASES, "preset", "state.json"),
                      dict(case, artifacts={"saved.vital": "json"}, base64_f32_keys=["wave_data"]))
        code, output = self.run_main(*compare)
        self.assertEqual(code, 0, output)
        self.assertIn("pass  preset/state/init:artifact/saved.vital (json bound)", output)
        # ...while the determinism compare (same build twice) stays byte-exact.
        code, output = self.run_main(*(compare + ["--exact"]))
        self.assertEqual(code, 1, output)
        self.assertIn("FAIL  preset/state/init:artifact/saved.vital (exact)", output)

        # 5 ulp on a sample of magnitude >= 4 would still fail; samples in
        # [-1, 1] are held to the 1e-6 absolute floor.
        proxy_saw[700] = saw[700] + 1.5e-6
        with open(os.path.join(trees["proxy"], "modules", "preset", "state", "init", "artifact_saved.vital"),
                  "w") as handle:
            json.dump(preset(proxy_saw), handle)
        code, output = self.run_main(*compare)
        self.assertEqual(code, 1, output)
        self.assertIn("$.settings.wavetables[0].groups[0].components[0].keyframes[0].wave_data[700]", output)

    def write_file(self, rel, text, mode="w"):
        path = os.path.join(self.tmp.name, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, mode) as handle:
            handle.write(text)

    def write_morph_case(self):
        self.write_file("migration/decisions/nofastmath/filters/test.md", ALLOWLIST_FILE)
        self.write_file("migration/ledger/2026-10-21-owner-approval-sas-35-sas-36.md", APPROVAL_ENTRY)
        rc.write_json(os.path.join(rc.MODULE_CASES, "filters", "test.json"), {
            "module": "filter", "cases": {"a": {"num_samples": 4}, "morph": {"reference": "nofastmath"}}})

    def add_morph_capture(self, tree, identity, values):
        case_dir = os.path.join(tree, "modules", "filters", "test", "morph")
        os.makedirs(case_dir, exist_ok=True)
        digest = write_npy(os.path.join(case_dir, "out_audio.npy"), values, 4)
        rc.write_json(os.path.join(case_dir, "capture.json"), {
            "build": dict(identity), "outputs": {"audio": {"file": "out_audio.npy", "fnv1a64": digest}}})
        return tree

    def test_compare_nofastmath_case_is_exact_against_the_nofastmath_tree(self):
        self.write_morph_case()
        values = [0.5, -0.25, 0.125, 0.0]
        official = self.add_morph_capture(os.path.join(self.tmp.name, "ref"), IDENTITY, [0.5, -0.25, 0.126, 0.0])
        proxy = self.add_morph_capture(os.path.join(self.tmp.name, "proxy"), IDENTITY, values)
        port = self.add_morph_capture(os.path.join(self.tmp.name, "port"), IDENTITY, values)
        fake_diff = os.path.join(self.tmp.name, "fake-audio-diff")
        with open(fake_diff, "w") as handle:
            handle.write("#!/bin/sh\necho audio-diff should not run\nexit 2\n")
        os.chmod(fake_diff, 0o755)
        only = ["--only", "filters/test/morph", "--audio-diff", fake_diff]

        code, output = self.run_main("compare", official, port, *only)
        self.assertEqual(code, 2, output)
        self.assertIn("pass --reference-nofastmath DIR", output)

        code, output = self.run_main("compare", official, port, "--reference-nofastmath", proxy, *only)
        self.assertEqual(code, 0, output)
        self.assertIn("pass  filters/test/morph:audio (nofastmath, bit-exact)", output)

        self.add_morph_capture(port, IDENTITY, [0.5, -0.25, f32(0.125, 1), 0.0])
        code, output = self.run_main("compare", official, port, "--reference-nofastmath", proxy, *only)
        self.assertEqual(code, 1, output)
        self.assertIn("FAIL  filters/test/morph:audio (nofastmath, bit-exact)", output)

    def test_nofastmath_case_must_be_justified_in_calibration(self):
        self.write_morph_case()
        official = self.add_morph_capture(os.path.join(self.tmp.name, "ref"), IDENTITY, [0.5, -0.25, 0.126, 0.0])
        proxy = self.add_morph_capture(os.path.join(self.tmp.name, "proxy"), IDENTITY, [0.5, -0.25, 0.125, 0.0])
        cmp_diff = os.path.join(self.tmp.name, "cmp-audio-diff")
        with open(cmp_diff, "w") as handle:
            handle.write('#!/bin/sh\ncmp -s "$1" "$2" && exit 0\nexit 1\n')
        os.chmod(cmp_diff, 0o755)
        only = ["--only", "filters/test/morph", "--audio-diff", cmp_diff, "--justify-nofastmath"]

        code, output = self.run_main("compare", official, proxy, *only)
        self.assertEqual(code, 0, output)
        self.assertIn("pass  filters/test/morph (reference \"nofastmath\" justified", output)
        self.assertIn("in filters/test/morph:audio", output)

        # The official reference passes its limits: the case must use it.
        self.add_morph_capture(official, IDENTITY, [0.5, -0.25, 0.125, 0.0])
        code, output = self.run_main("compare", official, proxy, *only)
        self.assertEqual(code, 2, output)
        self.assertIn("ERROR filters/test/morph (reference \"nofastmath\" NOT justified on x86", output)

        # Declared for macOS only: the x86 job does not gate it...
        allowlist = os.path.join(rc.NOFASTMATH_DIR, "filters", "test.md")
        self.write_file(os.path.relpath(allowlist, self.tmp.name), ALLOWLIST_FILE.replace("(SAS-35; x86)",
                                                                                           "(SAS-35; macos)"))
        code, output = self.run_main("compare", official, proxy, *only)
        self.assertEqual(code, 0, output)
        self.assertIn("declared for macos, not gated on x86", output)
        # ...the macOS job does, on its own official and no-fast-math trees.
        macos = dict(IDENTITY, arch="aarch64", compiler="Apple LLVM 17.0.0", glibc=None)
        mac_official = self.add_morph_capture(os.path.join(self.tmp.name, "mac-ref"), macos, [0.5, -0.25, 0.126, 0.0])
        mac_proxy = self.add_morph_capture(os.path.join(self.tmp.name, "mac-proxy"), macos, [0.5, -0.25, 0.125, 0.0])
        code, output = self.run_main("compare", mac_official, mac_proxy, *only)
        self.assertEqual(code, 0, output)
        self.assertIn("justified on macos", output)
        self.add_morph_capture(mac_official, macos, [0.5, -0.25, 0.125, 0.0])
        code, output = self.run_main("compare", mac_official, mac_proxy, *only)
        self.assertEqual(code, 2, output)
        self.assertIn("NOT justified on macos", output)
        # A build that is neither target cannot justify anything.
        proxy_linux = dict(IDENTITY, arch="aarch64", compiler="13.3.0")
        linux = self.add_morph_capture(os.path.join(self.tmp.name, "arm-linux"), proxy_linux, [0.5, -0.25, 0.126, 0.0])
        code, output = self.run_main("compare", linux, mac_proxy, *only)
        self.assertEqual(code, 2, output)
        self.assertIn("is not one of ['x86', 'macos']", output)
        self.assertEqual(rc.justify_target(IDENTITY), "x86")
        self.assertEqual(rc.justify_target(macos), "macos")

    def test_tightened_json_bound_alone_does_not_justify(self):
        self.write_morph_case()
        rc.write_json(os.path.join(rc.MODULE_CASES, "filters", "test.json"), {
            "module": "filter", "artifacts": {"saved.json": "json"}, "json_bound": {"ulp": 0, "abs": 0},
            "cases": {"a": {"num_samples": 4}, "morph": {"reference": "nofastmath"}}})
        trees = []
        for name, value in (("ref", f32(0.3)), ("proxy", f32(0.3, 1))):
            case_dir = os.path.join(self.tmp.name, name, "modules", "filters", "test", "morph")
            os.makedirs(case_dir)
            with open(os.path.join(case_dir, "artifact_saved.json"), "w") as handle:
                json.dump({"gain": value}, handle)
            rc.write_json(os.path.join(case_dir, "capture.json"), {"build": dict(IDENTITY), "outputs": {}})
            trees.append(os.path.join(self.tmp.name, name))
        fake_diff = os.path.join(self.tmp.name, "fake-audio-diff")
        with open(fake_diff, "w") as handle:
            handle.write("#!/bin/sh\nexit 2\n")
        os.chmod(fake_diff, 0o755)
        code, output = self.run_main("compare", trees[0], trees[1], "--audio-diff", fake_diff,
                                     "--only", "filters/test/morph", "--justify-nofastmath")
        self.assertEqual(code, 2, output)
        self.assertIn("NOT justified", output)

    def test_nofastmath_hashes_use_their_own_identity(self):
        self.write_morph_case()
        nofastmath = dict(IDENTITY, fast_math=False, opt_flags="-O3 -march=x86-64")
        official = self.add_morph_capture(self.make_tree(), IDENTITY, [0.5] * 4)
        self.write_baseline(official)
        proxy = self.add_morph_capture(self.make_tree(nofastmath, "proxy"), nofastmath, [0.25] * 4)

        code, output = self.run_main("hashes", official, "--check", "--reference", "nofastmath")
        self.assertEqual(code, 2, output)
        self.assertIn("not rendered by a REF_FAST_MATH=OFF build", output)

        code, output = self.run_main("hashes", proxy, "--check", "--reference", "nofastmath",
                                     "--require-known-identity")
        self.assertEqual(code, 1, output)
        self.assertIn("has no hashes for the CI build identity", output)
        code, output = self.run_main("hashes", proxy, "--write", "--reference", "nofastmath")
        self.assertEqual(code, 0, output)
        with open(os.path.join(rc.MODULE_CASES, "filters", "test.hashes.json")) as handle:
            entries = json.load(handle)["entries"]
        self.assertEqual([sorted(e["hashes"]) for e in entries],
                         [["a:artifact/saved.json", "a:audio", "morph:audio"], ["morph:audio"]])
        self.assertEqual(entries[1]["identity"], nofastmath)
        code, output = self.run_main("hashes", proxy, "--check", "--reference", "nofastmath",
                                     "--require-known-identity")
        self.assertEqual(code, 0, output)
        code, output = self.run_main("hashes", official, "--check", "--require-known-identity")
        self.assertEqual(code, 0, output)

        self.add_morph_capture(proxy, nofastmath, [0.375] * 4)
        code, output = self.run_main("hashes", proxy, "--check", "--reference", "nofastmath")
        self.assertEqual(code, 1, output)
        self.assertIn("MISMATCH", output)

        # Only the exact counterpart is known: the CI flags minus
        # " -ffast-math -U__FAST_MATH__" (reviewer note on #34).
        for index, change in enumerate(({"compiler": "14.2.0"}, {"opt_flags": "-O3 -march=native"},
                                        {"opt_flags": "-O3 -march=x86-64 -ffp-contract=fast"},
                                        {"opt_flags": "-O3 -ffast-math -U__FAST_MATH__ -march=x86-64"})):
            other = dict(nofastmath, **change)
            stranger = self.add_morph_capture(self.make_tree(other, "stranger%d" % index), other, [0.25] * 4)
            code, output = self.run_main("hashes", stranger, "--write", "--reference", "nofastmath")
            self.assertEqual(code, 2, (change, output))
            self.assertIn("not a known CI identity", output)
        self.assertEqual(rc.nofastmath_counterpart(IDENTITY), nofastmath)
        self.assertIsNone(rc.nofastmath_counterpart(dict(IDENTITY, opt_flags="-O3")))

    def test_tolerance_exception_approved_in_a_ledger_entry_file(self):
        case = {"num_samples": 4, "tolerances": {"residual_db": -70},
                "tolerance_exception": {"approved_in": "2026-10-23: approve looser filters/test/b",
                                        "tolerances": {"residual_db": -70}}}
        rc.write_json(os.path.join(rc.MODULE_CASES, "filters", "test.json"), {"module": "filter", "cases": {"b": case}})
        with self.assertRaises(rc.CaseError) as raised:
            rc.load_modules()
        self.assertIn("does not exist (migration/LEDGER.md or migration/ledger/)", str(raised.exception))
        os.makedirs(os.path.join(self.tmp.name, "migration", "ledger"))
        with open(os.path.join(self.tmp.name, "migration", "ledger", "2026-10-23-approve-b.md"), "w") as handle:
            handle.write("## 2026-10-23: approve looser filters/test/b\n\nOwner approved filters/test/b.\n")
        self.assertEqual(rc.load_modules()[0]["tolerances"]["residual_db"], -70.0)

    def git(self, *argv):
        result = subprocess.run(["git", "-C", self.tmp.name] + list(argv), stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, text=True)
        self.assertEqual(result.returncode, 0, result.stdout)

    def commit(self, message):
        self.git("add", "-A")
        self.git("-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", message)

    def test_allowlist_approval_checks_additions_against_loaded_cases(self):
        # A base with the legacy decision-doc allowlist: moving its ids into
        # the per-module files is not an addition.
        self.write_file("migration/decisions/owner-decisions-json-and-morphs.md", LEGACY_DECISION)
        self.git("init", "-q")
        self.commit("base")
        self.write_morph_case()
        os.remove(os.path.join(self.tmp.name, "migration", "decisions", "owner-decisions-json-and-morphs.md"))
        code, output = self.run_main("allowlist-approval", "--base", "HEAD")
        self.assertEqual(code, 0, output)
        self.assertNotIn("added", output.replace("additions", ""))
        self.commit("per-module allowlist")

        # Listed, but the case is still parked: rejected.
        comb = "- `filters/test/comb` (SAS-35; x86): feedback tail\n"
        self.write_file("migration/decisions/nofastmath/filters/test.md", comb, "a")
        code, output = self.run_main("allowlist-approval", "--base", "HEAD")
        self.assertEqual(code, 1, output)
        self.assertIn("added filters/test/comb (SAS-35; x86)", output)
        self.assertIn("filters/test/comb was added", output)
        # The same change moves the case in: accepted, with no ledger entry.
        rc.write_json(os.path.join(rc.MODULE_CASES, "filters", "test.json"), {
            "module": "filter", "cases": {"a": {"num_samples": 4}, "morph": {"reference": "nofastmath"},
                                          "comb": {"reference": "nofastmath"}}})
        code, output = self.run_main("allowlist-approval", "--base", "HEAD")
        self.assertEqual(code, 0, output)

        # An uncited or unpinned approval fails at load (exit 2).
        self.write_file("migration/decisions/nofastmath/filters/test.md",
                        ALLOWLIST_FILE + "- `filters/test/comb` (SAS-99; x86): feedback tail\n")
        code, output = self.run_main("allowlist-approval", "--base", "HEAD")
        self.assertEqual(code, 2, output)
        self.assertIn("SAS-99 is not an approval pinned", output)
        # Removing the approval's ledger entry fails at load too.
        self.write_file("migration/decisions/nofastmath/filters/test.md", ALLOWLIST_FILE + comb)
        os.remove(os.path.join(self.tmp.name, "migration", "ledger", "2026-10-21-owner-approval-sas-35-sas-36.md"))
        code, output = self.run_main("allowlist-approval", "--base", "HEAD")
        self.assertEqual(code, 2, output)
        self.assertIn("cites SAS-35", output)

        code, output = self.run_main("allowlist-approval", "--base", "no-such-rev")
        self.assertEqual(code, 2, output)

    def write_park(self, cases):
        """cases: {id: reason}; each park cites a dated approval entry."""
        os.makedirs(os.path.join(self.tmp.name, "migration", "ledger"), exist_ok=True)
        with open(os.path.join(self.tmp.name, "migration", "ledger", "2026-10-21-parks.md"), "w") as handle:
            handle.write("## 2026-10-21: owner approval — macOS parks\n\nParked on macOS: %s.\n"
                         % ", ".join(sorted(cases)))
        path = os.path.join(self.tmp.name, "parked_arm64.json")
        rc.write_json(path, {"description": "test", "cases": {
            case_id: {"approved_in": "2026-10-21: owner approval — macOS parks", "reason": reason}
            for case_id, reason in cases.items()}})
        return path

    # Arm64-only parks used to drop x86 coverage: run_corpus.py ignored them,
    # so threads moved macOS-only noisy cases to pending/.
    def test_compare_skips_and_reports_parked_cases(self):
        ((ref, _), (test, test_case)), fake_diff = self.make_text_only_trees()
        with open(os.path.join(test_case, "artifact_saved.json"), "wb") as handle:
            handle.write(b'{"version": "1.0.7"}\n')
        only = ["--audio-diff", fake_diff, "--only", "harness/textonly"]
        code, output = self.run_main("compare", ref, test, *only)
        self.assertEqual(code, 1, output)

        park = self.write_park({"harness/textonly/json": "noisy on macOS (A11)"})
        code, output = self.run_main("compare", ref, test, "--parked", park, *only)
        self.assertEqual(code, 0, output)
        self.assertIn("parked harness/textonly/json (parked_arm64.json; not compared on this reference only): "
                      "noisy on macOS (A11)", output)
        self.assertNotIn("artifact/saved.json", output)

        # Strict: a parked id must be a loaded case.
        for cases in ({"harness/textonly/gone": "x"}, {"harness/textonly": "x"}):
            code, output = self.run_main("compare", ref, test, "--parked", self.write_park(cases), *only)
            self.assertEqual(code, 2, output)
            self.assertIn("is not a loaded case", output)
        rc.write_json(park, {"cases": [], "description": "x"})
        code, output = self.run_main("compare", ref, test, "--parked", park, *only)
        self.assertEqual(code, 2, output)

    def test_hashes_skip_parked_cases_and_keep_their_hashes(self):
        tree = self.make_tree()
        self.write_baseline(tree)
        hash_file = os.path.join(rc.MODULE_CASES, "filters", "test.hashes.json")
        with open(hash_file) as handle:
            committed = json.load(handle)
        npy = os.path.join(tree, "modules", "filters", "test", "a", "out_audio.npy")
        digest = write_npy(npy, [0.25] * 16, 4)
        capture = os.path.join(tree, "modules", "filters", "test", "a", "capture.json")
        with open(capture) as handle:
            data = json.load(handle)
        data["outputs"]["audio"]["fnv1a64"] = digest
        rc.write_json(capture, data)
        code, output = self.run_main("hashes", tree, "--check")
        self.assertEqual(code, 1, output)
        self.assertIn("MISMATCH", output)

        park = self.write_park({"filters/test/a": "noisy on macOS"})
        code, output = self.run_main("hashes", tree, "--check", "--require-known-identity", "--parked", park)
        self.assertEqual(code, 0, output)
        self.assertIn("parked filters/test/a (parked_arm64.json; hashes not checked on this reference only)", output)
        # Writing with the park keeps the committed hashes of the parked case.
        code, output = self.run_main("hashes", tree, "--write", "--parked", park)
        self.assertEqual(code, 0, output)
        with open(hash_file) as handle:
            self.assertEqual(json.load(handle), committed)
        code, output = self.run_main("hashes", tree, "--check", "--parked", self.write_park({"filters/test/b": "x"}))
        self.assertEqual(code, 2, output)

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
