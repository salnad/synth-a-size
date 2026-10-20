#!/usr/bin/env python3
"""Generate or check the reference preset-state goldens (M2).

The sas-preset tests compare the port against these files, which are written
by the reference harness's ref-preset tool and committed so `cargo test` needs
no C++ build. CI rebuilds the reference and runs --check, so a golden can only
change through the pinned reference, never by hand.

  migration/corpus/preset_state/
    registry.json            ref-preset registry --seed 0
    init.json                ref-preset state --init --seed 0
    init.roundtrip.json      the state of init.json loaded again
    <preset>.json            ref-preset state <preset> --seed 0, for every
                             corpus/presets/*.vital and corpus/preset_tests/*.vital
                             ({"load_error": ...} when the load fails)
    <preset>.roundtrip.json  the state of <preset>.json loaded again (corpus
                             presets only), for the idempotence check

A round trip reloads the saved state from a temporary file with the same name
as the original, because SynthBase::loadFromFile names the preset after its
file.

Usage:
  python3 migration/scripts/preset_goldens.py --write
  python3 migration/scripts/preset_goldens.py --check

Exit status: 0 ok, 1 stale goldens (--check), 2 tool error.
Python 3.8+ standard library only.
"""

import argparse
import glob
import os
import subprocess
import sys
import tempfile

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CORPUS = os.path.join(REPO, "migration", "corpus")
GOLDENS = os.path.join(CORPUS, "preset_state")
DEFAULT_TOOL = os.path.join(REPO, "migration", "reference", "build", "ref-preset")
SEED = "0"


def run(tool, args):
    result = subprocess.run([tool] + args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode not in (0, 3):
        raise RuntimeError("ref-preset %s failed (%d): %s" % (" ".join(args), result.returncode,
                                                             result.stderr.decode(errors="replace")))
    return result.stdout, result.returncode == 0


def roundtrip(tool, name, state):
    with tempfile.TemporaryDirectory() as directory:
        path = os.path.join(directory, name + ".vital")
        with open(path, "wb") as handle:
            handle.write(state)
        output, ok = run(tool, ["state", path, "--seed", SEED])
    if not ok:
        raise RuntimeError("reloading the saved state of %s failed: %s" % (name, output.decode(errors="replace")))
    return output


def outputs(tool):
    files = {}
    files["registry.json"], _ = run(tool, ["registry", "--seed", SEED])
    init, _ = run(tool, ["state", "--init", "--seed", SEED])
    files["init.json"] = init
    files["init.roundtrip.json"] = roundtrip(tool, "init", init)

    corpus = sorted(glob.glob(os.path.join(CORPUS, "presets", "*.vital")))
    tests = sorted(glob.glob(os.path.join(CORPUS, "preset_tests", "*.vital")))
    for path in corpus + tests:
        name = os.path.splitext(os.path.basename(path))[0]
        if name + ".json" in files:
            raise RuntimeError("duplicate preset name " + name)
        state, ok = run(tool, ["state", path, "--seed", SEED])
        files[name + ".json"] = state
        if ok and path in corpus:
            files[name + ".roundtrip.json"] = roundtrip(tool, name, state)
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--check", action="store_true")
    parser.add_argument("--ref-preset", default=DEFAULT_TOOL)
    args = parser.parse_args()

    if not os.access(args.ref_preset, os.X_OK):
        print("missing tool: %s (run migration/scripts/build_reference.sh)" % args.ref_preset, file=sys.stderr)
        return 2
    try:
        files = outputs(args.ref_preset)
    except RuntimeError as error:
        print(error, file=sys.stderr)
        return 2

    existing = set(os.listdir(GOLDENS)) if os.path.isdir(GOLDENS) else set()
    stale = sorted(name for name in files
                   if not os.path.exists(os.path.join(GOLDENS, name))
                   or open(os.path.join(GOLDENS, name), "rb").read() != files[name])
    extra = sorted(existing - set(files))

    if args.check:
        for name in stale:
            print("stale or missing golden:", name)
        for name in extra:
            print("golden without a source preset:", name)
        print("%d goldens checked, %d stale, %d extra" % (len(files), len(stale), len(extra)))
        return 1 if stale or extra else 0

    os.makedirs(GOLDENS, exist_ok=True)
    for name in stale:
        with open(os.path.join(GOLDENS, name), "wb") as handle:
            handle.write(files[name])
        print("wrote", os.path.relpath(os.path.join(GOLDENS, name), REPO))
    for name in extra:
        os.remove(os.path.join(GOLDENS, name))
        print("removed", os.path.relpath(os.path.join(GOLDENS, name), REPO))
    return 0


if __name__ == "__main__":
    sys.exit(main())
