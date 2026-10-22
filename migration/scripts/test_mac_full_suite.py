#!/usr/bin/env python3
"""Tests for mac_full_suite_gate.py and mac_full_suite_report.py (reference-arm-main.yml)."""

import os
import subprocess
import tempfile
import unittest

import mac_full_suite_gate as gate
import mac_full_suite_report as report

DOWNLOAD = "Run actions/download-artifact@v8"
UPLOAD = "Run actions/upload-artifact@v7"


class InfrastructureOnly(unittest.TestCase):
    def test_artifact_transfer_only(self):
        # Run 39775970688: the macOS 14 job could not download the macOS 15 artifact.
        self.assertTrue(report.infrastructure_only(
            {"macOS 14 (same binary as macOS 15; Apple clang 15 diagnostics)": [DOWNLOAD, UPLOAD]}))

    def test_real_step_failed_too(self):
        self.assertFalse(report.infrastructure_only(
            {"macOS 14": [DOWNLOAD], "macOS 15": ["Hashes match the committed macOS arm64 identity", UPLOAD]}))

    def test_failed_job_without_reported_steps_is_not_infrastructure(self):
        self.assertFalse(report.infrastructure_only({"macOS 14": [DOWNLOAD], "macOS 15": []}))
        self.assertFalse(report.infrastructure_only({}))

    def test_body_says_rerun_not_real_failure(self):
        body = report.body_for_failure("o/r", "1", "u", "a" * 40, {"macos-14": "failure", "macos-15": "success"},
                                       {"macOS 14": [DOWNLOAD, UPLOAD]}, set())
        self.assertIn("Infrastructure — re-run", body)
        self.assertNotIn("real failure", body)

    def test_body_real_failure_unchanged(self):
        body = report.body_for_failure("o/r", "1", "u", "a" * 40, {"macos-14": "failure"},
                                       {"macOS 14": ["Same binary on macOS 14 vs macOS 15 is bit-identical "
                                                     "(OS / Apple libm drift)", UPLOAD]}, set())
        self.assertIn("real failure", body)
        self.assertNotIn("Infrastructure", body)


class SuiteFilesChanged(unittest.TestCase):
    def setUp(self):
        self.cwd = os.getcwd()
        self.dir = tempfile.mkdtemp()
        os.chdir(self.dir)
        git = ["git", "-c", "user.name=t", "-c", "user.email=t@t"]
        subprocess.run(["git", "init", "-q"], check=True)
        os.makedirs(".github/workflows")
        os.makedirs("migration/scripts")
        self.write("README", "a")
        subprocess.run(git + ["add", "-A"], check=True)
        subprocess.run(git + ["commit", "-qm", "base"], check=True)
        self.base = self.head()
        self.write(".github/workflows/reference-arm-main.yml", "x")
        self.write("migration/scripts/other.py", "x")
        subprocess.run(git + ["add", "-A"], check=True)
        subprocess.run(git + ["commit", "-qm", "change"], check=True)
        self.git = git

    def tearDown(self):
        os.chdir(self.cwd)

    def write(self, path, text):
        with open(path, "w") as handle:
            handle.write(text)

    def head(self):
        return subprocess.run(["git", "rev-parse", "HEAD"], check=True, stdout=subprocess.PIPE,
                              universal_newlines=True).stdout.strip()

    def test_workflow_change_runs_the_suite(self):
        self.assertEqual(gate.suite_files_changed(self.base), [".github/workflows/reference-arm-main.yml"])
        run, reason = gate.decide("push", 6, "o/r", "1", self.base)
        self.assertTrue(run)
        self.assertIn("changes the full suite itself", reason)

    def test_no_before_or_unknown_before(self):
        self.assertEqual(gate.suite_files_changed(""), [])
        self.assertEqual(gate.suite_files_changed("0" * 40), [])
        self.assertEqual(gate.suite_files_changed("f" * 40), [])

    def test_unrelated_change(self):
        self.write("README", "b")
        subprocess.run(self.git + ["commit", "-qam", "readme"], check=True)
        self.assertEqual(gate.suite_files_changed(self.head() + "~1"), [])


if __name__ == "__main__":
    unittest.main()
