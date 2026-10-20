#!/usr/bin/env python3
"""Attribute ARM-vs-x86 reference differences to upstream mechanisms.

Derives variant presets from the golden corpus that each switch off one
mechanism that behaves differently on ARM and x86 in upstream code
(migration/decisions/reference-arm.md), renders them, and compares two
platforms' renders with the corpus default tolerances:

  random phase   synth_oscillator.cpp: negative float -> uint32_t (UB; x86
                 wraps, AArch64 saturates to 0)       -> osc_N_random_phase = 0
  delay lines    memory.h Memory::get uses utils::toInt (x86 rounds, NEON
                 truncates)                           -> chorus/delay/flanger off
  wave frame     synth_oscillator.cpp wave_index = toInt(frame)
                                                      -> no wave-frame modulation
  sum order      poly_float::sum() adds (v0+v2)+(v1+v3) on SSE2 and
                 (v0+v1)+(v2+v3) on NEON; the reverb's feedback matrix uses it
                 (its toInt only sees floored, exact integers)
                                                      -> reverb off / reverb only

  arm_attribution.py render --ref-render BIN --out DIR
  arm_attribution.py compare REFERENCE_DIR TEST_DIR [--audio-diff BIN] [--markdown FILE]

The variants are experiment inputs, not corpus cases; nothing here changes
cases.json or its tolerances. Python 3.8+ standard library only.
"""

import argparse
import json
import os
import subprocess
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
CORPUS = os.path.join(REPO, "migration", "corpus")
DEFAULT_DIFF = os.path.join(REPO, "migration", "tools", "audio-diff", "target", "release", "audio-diff")


def no_random_phase(settings):
    for osc in (1, 2, 3):
        settings["osc_%d_random_phase" % osc] = 0.0


def no_modulated_delays(settings):
    for effect in ("chorus", "delay", "flanger"):
        settings[effect + "_on"] = 0.0


def no_reverb(settings):
    settings["reverb_on"] = 0.0


def reverb_only(settings):
    settings["reverb_on"] = 1.0
    settings["reverb_dry_wet"] = 0.5


def no_wave_frame_modulation(settings):
    settings["modulations"] = [m for m in settings.get("modulations", []) if not m["destination"].endswith("wave_frame")]
    for key in [k for k in settings if k.startswith("modulation_") and k.endswith("_amount")]:
        settings[key] = 0.0


# name: (base preset, midi, [transforms], what it isolates)
VARIANTS = {
    "init_saw__single_note": ("init_saw", "single_note_c3", [], "baseline (random phase on)"),
    "init_saw__single_note__no_random_phase": ("init_saw", "single_note_c3", [no_random_phase], "random phase off"),
    "init_saw__single_note__no_random_phase_reverb": ("init_saw", "single_note_c3", [no_random_phase, reverb_only], "random phase off, reverb on (sum order only)"),
    "unison_pad_fx__chords": ("unison_pad_fx", "chord_progression", [], "baseline (random phase on, chorus + delay on)"),
    "unison_pad_fx__chords__no_random_phase": ("unison_pad_fx", "chord_progression", [no_random_phase], "random phase off"),
    "unison_pad_fx__chords__no_random_phase_no_delays": ("unison_pad_fx", "chord_progression", [no_random_phase, no_modulated_delays], "random phase off, chorus/delay off"),
    "unison_pad_fx__chords__no_random_phase_no_delays_no_reverb": ("unison_pad_fx", "chord_progression", [no_random_phase, no_modulated_delays, no_reverb], "random phase off, chorus/delay/reverb off"),
    "line_wavetable_fold__velocity": ("line_wavetable_fold", "velocity_ramp", [], "baseline (env 2 sweeps the wave frame)"),
    "line_wavetable_fold__velocity__no_frame_mod": ("line_wavetable_fold", "velocity_ramp", [no_wave_frame_modulation], "wave-frame modulation off"),
}


def cmd_render(args):
    os.makedirs(args.out, exist_ok=True)
    failures = 0
    for name, (preset, midi, transforms, _) in VARIANTS.items():
        with open(os.path.join(CORPUS, "presets", preset + ".vital")) as handle:
            data = json.load(handle)
        for transform in transforms:
            transform(data["settings"])
        preset_path = os.path.join(args.out, name + ".vital")
        with open(preset_path, "w") as handle:
            json.dump(data, handle)
        command = [args.ref_render, preset_path, os.path.join(CORPUS, "midi", midi + ".mid"),
                   "-o", os.path.join(args.out, name + ".wav"), "--seed", "0"]
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        print(("ok   " if result.returncode == 0 else "FAIL ") + name)
        failures += result.returncode != 0
    return 2 if failures else 0


def cmd_compare(args):
    with open(os.path.join(CORPUS, "cases.json")) as handle:
        tolerances = json.load(handle)["defaults"]["tolerances"]
    flags = {"max_abs_error": "--max-abs-error", "rms_diff_db": "--rms-diff-db",
             "residual_db": "--residual-db", "lsd_db": "--lsd-db"}
    rows = ["| Variant | Isolates | Pass | max abs | residual dB | LSD dB |", "|---|---|---|---|---|---|"]
    for name, (_, _, _, isolates) in VARIANTS.items():
        command = [args.audio_diff, os.path.join(args.reference, name + ".wav"),
                   os.path.join(args.test, name + ".wav"), "--quiet", "--json", "-"]
        for key, value in tolerances.items():
            command += [flags[key], str(value)]
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        if result.returncode == 2:
            rows.append("| %s | %s | error | | | |" % (name, isolates))
            continue
        report = json.loads(result.stdout)
        rows.append("| %s | %s | %s | %.2e | %s | %.4g |" % (
            name, isolates, "yes" if report["pass"] else "**no**", report["max_abs_error"],
            report["residual_db"] if isinstance(report["residual_db"], str) else "%.1f" % report["residual_db"],
            report["lsd_db"]))
    markdown = "\n".join(rows) + "\n"
    print(markdown)
    if args.markdown:
        with open(args.markdown, "w") as handle:
            handle.write(markdown)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    render = sub.add_parser("render")
    render.add_argument("--ref-render", required=True)
    render.add_argument("--out", required=True)
    compare = sub.add_parser("compare")
    compare.add_argument("reference")
    compare.add_argument("test")
    compare.add_argument("--audio-diff", default=DEFAULT_DIFF)
    compare.add_argument("--markdown")
    args = parser.parse_args()
    return cmd_render(args) if args.command == "render" else cmd_compare(args)


if __name__ == "__main__":
    sys.exit(main())
