#!/usr/bin/env python3
"""Tests for mac_missing_only.py (the backfill gate of reference-arm-main.yml)."""

import unittest

import mac_missing_only as m

STEPS_OK = {"build": {"outcome": "success"}, "justify": {"outcome": "success"},
            "parity": {"outcome": "failure"}, "hashes": {"outcome": "failure"}, "write": {"outcome": "success"}}
HASH_MISSING = (
    "migration/corpus/modules/effects/x.hashes.json has no hashes for the CI build identity: write them\n"
    "migration/corpus/modules/effects/y.hashes.json: no stored hash for a:audio (run hashes --write)\n"
    "2740 hashes checked, 2 problems\n")
PARITY_MISSING = (
    "thread 'x' (1) panicked at crates/parity/src/captures.rs:446:21:\n"
    "sas-parity: migration/corpus/modules/effects/x.hashes.json has no hashes for the CI build identity; write\n"
    "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n")
PATCH = (
    "--- a/migration/corpus/modules/effects/y.hashes.json\n"
    "+++ b/migration/corpus/modules/effects/y.hashes.json\n"
    "@@ -4,1 +4,2 @@\n"
    '-        "a:other": "1111111111111111"\n'
    '+        "a:other": "1111111111111111",\n'
    '+        "a:audio": "2222222222222222"\n')


class MissingOnly(unittest.TestCase):
    def check(self, steps=STEPS_OK, hashes=(HASH_MISSING,), parity=PARITY_MISSING, patch=PATCH):
        return m.classify(steps, list(hashes), parity, patch)

    def test_missing_keys_only(self):
        ok, reason = self.check()
        self.assertTrue(ok, reason)

    def test_mismatch_is_real(self):
        ok, reason = self.check(hashes=(HASH_MISSING + "MISMATCH migration/corpus/x.hashes.json a:audio expected 1 got 2\n",))
        self.assertFalse(ok)
        self.assertIn("MISMATCH", reason)

    def test_stale_or_unrendered_is_real(self):
        for line in ("migration/corpus/x.hashes.json: stale hash for removed case a:audio\n",
                     "migration/corpus/x.hashes.json: a:audio has a stored hash but was not rendered\n"):
            ok, _ = self.check(hashes=(line,))
            self.assertFalse(ok, line)

    def test_other_failed_step_is_real(self):
        steps = dict(STEPS_OK, repeat={"outcome": "failure"})
        ok, reason = self.check(steps=steps)
        self.assertFalse(ok)
        self.assertIn("repeat", reason)

    def test_nothing_failed(self):
        steps = {k: {"outcome": "success"} for k in STEPS_OK}
        self.assertFalse(self.check(steps=steps)[0])

    def test_parity_comparison_failure_is_real(self):
        parity = PARITY_MISSING + "thread 'y' panicked at a.rs:1:1:\nparity FAILED for effects/x:audio\n"
        self.assertFalse(self.check(parity=parity)[0])

    def test_other_panic_is_real(self):
        parity = "thread 'y' panicked at a.rs:1:1:\nindex out of bounds\ntest result: FAILED. 0 passed; 1 failed;\n"
        self.assertFalse(self.check(parity=parity)[0])

    def test_build_error_is_real(self):
        self.assertFalse(self.check(parity="error: could not compile `sas-filters`\n")[0])

    def test_patch_must_only_add_hash_keys(self):
        self.assertFalse(self.check(patch="")[0])
        changed = PATCH.replace('+        "a:other": "1111111111111111",', '+        "a:other": "9999999999999999",')
        ok, reason = self.check(patch=changed)
        self.assertFalse(ok)
        self.assertIn("changes an existing line", reason)
        other = PATCH.replace("y.hashes.json", "y.json")
        self.assertFalse(self.check(patch=other)[0])


if __name__ == "__main__":
    unittest.main()
