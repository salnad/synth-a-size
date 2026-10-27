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


class MissingOnlyWithoutPr(unittest.TestCase):
    def body(self, env):
        saved = {k: os.environ.get(k) for k in env}
        os.environ.update(env)
        try:
            return report.body_for_failure("o/r", "7", "u", "a" * 40, {"macos-15": "failure", "backfill": "failure"},
                                           {"macOS 15": ["arm64 parity tests on the whole workspace (NEON path)"]},
                                           {"macos-hashes"})
        finally:
            for k, v in saved.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

    def test_backfill_failed_is_not_called_a_real_failure(self):
        # Runs 40438960207 and 40448236989: missing keys only, the backfill job failed after opening the PR.
        body = self.body({"MISSING_ONLY": "true", "MISSING_ONLY_REASON": "only missing macOS hash keys",
                          "BACKFILL_PR": ""})
        self.assertIn("backfill: failure", body)
        self.assertIn("macos_hashes.sh 7", body)
        self.assertNotIn("Not a missing-keys-only failure", body)
        self.assertNotIn("real failure", body)

    def test_backfill_pr_reported(self):
        body = self.body({"MISSING_ONLY": "true", "MISSING_ONLY_REASON": "r", "BACKFILL_PR": "https://x/pull/1"})
        self.assertIn("https://x/pull/1", body)


class BackfillOutput(unittest.TestCase):
    """mac_hash_backfill.sh's stdout feeds $GITHUB_OUTPUT: it must be exactly one `pr=` line."""

    def test_only_the_pr_line_reaches_stdout(self):
        script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "mac_hash_backfill.sh")
        root = tempfile.mkdtemp()
        origin, work, bin_dir = (os.path.join(root, d) for d in ("origin.git", "work", "bin"))
        git = ["git", "-c", "user.name=t", "-c", "user.email=t@t"]
        subprocess.run(["git", "init", "-q", "--bare", origin], check=True)
        subprocess.run(["git", "init", "-q", work], check=True)
        os.makedirs(os.path.join(work, "c"))
        with open(os.path.join(work, "c", "x.hashes.json"), "w") as handle:
            handle.write("{\n}\n")
        subprocess.run(git + ["add", "-A"], cwd=work, check=True)
        subprocess.run(git + ["commit", "-qm", "base"], cwd=work, check=True)
        subprocess.run(["git", "remote", "add", "origin", origin], cwd=work, check=True)
        with open(os.path.join(work, "c", "x.hashes.json"), "w") as handle:
            handle.write("{\n  \"k\": \"v\"\n}\n")
        patch = subprocess.run(["git", "diff"], cwd=work, check=True, stdout=subprocess.PIPE,
                               universal_newlines=True).stdout
        subprocess.run(["git", "checkout", "-q", "--", "."], cwd=work, check=True)
        with open(os.path.join(root, "p.patch"), "w") as handle:
            handle.write(patch)
        os.makedirs(bin_dir)
        with open(os.path.join(bin_dir, "gh"), "w") as handle:
            # Like the real gh: `pr create` and `workflow run` print URLs on stdout.
            handle.write("#!/bin/sh\ncase \"$1 $2\" in\n"
                         "  'pr list') ;;\n"
                         "  'pr create') echo https://example/pull/9 ;;\n"
                         "  'workflow run') echo https://example/actions/runs/1 ;;\n"
                         "esac\n")
        os.chmod(os.path.join(bin_dir, "gh"), 0o755)
        env = dict(os.environ, PATH=bin_dir + os.pathsep + os.environ["PATH"], RUN_ID="5",
                   GITHUB_REPOSITORY="o/r", GITHUB_SERVER_URL="https://example")
        result = subprocess.run([script, os.path.join(root, "p.patch")], cwd=work, env=env, check=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, universal_newlines=True)
        self.assertEqual(result.stdout, "pr=https://example/pull/9\n")


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
