#!/usr/bin/env python3
"""Generate the synth-a-size golden test corpus (presets + MIDI files).

Everything in migration/corpus/presets and migration/corpus/midi is produced by
this script from the definitions below; nothing is copied from any factory or
third-party preset bank. Re-run after editing and commit the outputs:

    python3 migration/corpus/generate.py

CI runs it with --check and fails if the committed files are stale.

Python 3.8+ standard library only.
"""

import argparse
import base64
import json
import math
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PRESET_DIR = os.path.join(HERE, "presets")
MIDI_DIR = os.path.join(HERE, "midi")

# Matches ProjectInfo::versionString of the upstream plugin build the reference
# harness compiles (see migration/reference/juce_config/JuceHeader.h).
SYNTH_VERSION = "1.0.6"
AUTHOR = "synth-a-size test corpus"
PPQ = 480
TEMPO_US_PER_QUARTER = 500000  # 120 BPM


# --------------------------------------------------------------------------
# Presets
# --------------------------------------------------------------------------

def corpus_sample(length=2048, sample_rate=44100):
    """A short deterministic sample (decaying two-partial tone), int16 base64.

    The preset loader requires a "sample" object, so every corpus preset embeds
    this tiny one instead of any upstream asset.
    """
    pcm = bytearray()
    for i in range(length):
        t = i / sample_rate
        env = math.exp(-6.0 * i / length)
        value = env * (0.6 * math.sin(2 * math.pi * 220.0 * t) + 0.3 * math.sin(2 * math.pi * 661.0 * t))
        pcm += struct.pack("<h", int(round(max(-1.0, min(1.0, value)) * 32767)))
    return {
        "name": "corpus decay tone",
        "length": length,
        "sample_rate": sample_rate,
        "samples": base64.b64encode(bytes(pcm)).decode("ascii"),
    }


def line(points, powers=None, smooth=False, name=""):
    """LineGenerator state from a list of (x, y) pairs, x and y in [0, 1]."""
    flat = []
    for x, y in points:
        flat += [float(x), float(y)]
    return {
        "name": name,
        "num_points": len(points),
        "points": flat,
        "powers": [float(p) for p in (powers or [0.0] * len(points))],
        "smooth": smooth,
    }


def line_fold_wavetable():
    """Line-source wavetable morphing square -> skewed triangle, then folded."""
    square = [(0.0, 0.0), (0.5, 0.0), (0.5, 1.0), (1.0, 1.0)]
    triangle = [(0.0, 0.5), (0.25, 0.0), (0.75, 1.0), (1.0, 0.5)]
    return {
        "name": "corpus line fold",
        "author": AUTHOR,
        "version": SYNTH_VERSION,
        "full_normalize": True,
        "remove_all_dc": True,
        "groups": [{
            "components": [
                {
                    "type": "Line Source",
                    "num_points": 4,
                    "interpolation_style": 1,
                    "keyframes": [
                        {"position": 0, "pull_power": 0.0, "line": line(square)},
                        {"position": 256, "pull_power": 0.0, "line": line(triangle)},
                    ],
                },
                {
                    "type": "Wave Folder",
                    "interpolation_style": 1,
                    "keyframes": [
                        {"position": 0, "fold_boost": 1.0},
                        {"position": 256, "fold_boost": 2.5},
                    ],
                },
            ]
        }],
    }


def preset(name, comments, settings, modulations=(), wavetables=None):
    """Build a preset. Unlisted controls load as their parameter defaults;
    omitted wavetables / LFO shapes keep the engine's init state."""
    settings = dict(settings)
    mods = []
    for index, (source, destination, amount) in enumerate(modulations, start=1):
        mods.append({"source": source, "destination": destination})
        settings["modulation_%d_amount" % index] = float(amount)
    settings["modulations"] = mods
    settings["sample"] = corpus_sample()
    if wavetables is not None:
        settings["wavetables"] = wavetables
    return {
        "synth_version": SYNTH_VERSION,
        "preset_name": name,
        "author": AUTHOR,
        "comments": comments,
        "preset_style": "",
        "macro1": "MACRO 1",
        "macro2": "MACRO 2",
        "macro3": "MACRO 3",
        "macro4": "MACRO 4",
        "settings": settings,
    }


PRESETS = {
    "init_saw": preset(
        "init saw",
        "Engine defaults: one oscillator on the init wavetable, no filter, no effects. Baseline.",
        {},
    ),
    "analog_filter_lfo": preset(
        "analog filter lfo",
        "Oscillator 1 into the analog (Sallen-Key) low-pass with LFO 1 sweeping the cutoff.",
        {
            "osc_1_random_phase": 0.0,
            "filter_1_on": 1.0,
            "filter_1_model": 0.0,
            "filter_1_style": 0.0,
            "filter_1_cutoff": 52.0,
            "filter_1_resonance": 0.65,
            "filter_1_drive": 4.0,
            "lfo_1_sync": 0.0,
            "lfo_1_frequency": 1.0,
            "env_1_attack": 0.05,
            "env_1_release": 0.3,
        },
        modulations=[("lfo_1", "filter_1_cutoff", 0.35)],
    ),
    "unison_pad_fx": preset(
        "unison pad fx",
        "Two oscillators with unison, slow envelope, chorus + delay + reverb in the effect chain.",
        {
            "osc_1_unison_voices": 5.0,
            "osc_1_unison_detune": 3.0,
            "osc_1_stereo_spread": 0.8,
            "osc_2_on": 1.0,
            "osc_2_transpose": 12.0,
            "osc_2_level": 0.4,
            "osc_2_unison_voices": 3.0,
            "env_1_attack": 0.9,
            "env_1_decay": 1.2,
            "env_1_sustain": 0.8,
            "env_1_release": 1.1,
            "chorus_on": 1.0,
            "chorus_dry_wet": 0.4,
            "delay_on": 1.0,
            "delay_dry_wet": 0.25,
            "delay_feedback": 0.4,
            "reverb_on": 1.0,
            "reverb_dry_wet": 0.3,
            "reverb_decay_time": 0.5,
        },
    ),
    "line_wavetable_fold": preset(
        "line wavetable fold",
        "Authored line-source wavetable with a wave folder; envelope 2 scans the wave frame; distortion on.",
        {
            "osc_1_random_phase": 0.0,
            "osc_1_wave_frame": 0.0,
            "env_2_attack": 0.6,
            "env_2_decay": 0.9,
            "env_2_sustain": 0.4,
            "distortion_on": 1.0,
            "distortion_type": 1.0,
            "distortion_drive": 6.0,
            "distortion_mix": 0.6,
        },
        modulations=[("env_2", "osc_1_wave_frame", 0.9)],
        wavetables=[line_fold_wavetable()],
    ),
    "mono_legato_glide": preset(
        "mono legato glide",
        "Mono legato voice with portamento; mod wheel opens the ladder filter; velocity scales level.",
        {
            "polyphony": 1.0,
            "legato": 1.0,
            "portamento_time": -2.0,
            "osc_1_random_phase": 0.0,
            "filter_1_on": 1.0,
            "filter_1_model": 2.0,
            "filter_1_cutoff": 45.0,
            "filter_1_resonance": 0.4,
            "pitch_bend_range": 2.0,
        },
        modulations=[
            ("mod_wheel", "filter_1_cutoff", 0.5),
            ("velocity", "osc_1_level", 0.3),
        ],
    ),
}


# --------------------------------------------------------------------------
# MIDI
# --------------------------------------------------------------------------

def vlq(value):
    out = [value & 0x7F]
    value >>= 7
    while value:
        out.append(0x80 | (value & 0x7F))
        value >>= 7
    return bytes(reversed(out))


def midi_file(events):
    """Format-0 SMF at 120 BPM. events: list of (seconds, bytes)."""
    ticks_per_second = PPQ * 1_000_000 / TEMPO_US_PER_QUARTER
    timed = [(0, 0, b"\xff\x51\x03" + TEMPO_US_PER_QUARTER.to_bytes(3, "big"))]
    for order, (seconds, data) in enumerate(events, start=1):
        timed.append((int(round(seconds * ticks_per_second)), order, bytes(data)))
    timed.sort(key=lambda event: (event[0], event[1]))

    track = bytearray()
    last_tick = 0
    for tick, _, data in timed:
        track += vlq(tick - last_tick) + data
        last_tick = tick
    track += vlq(0) + b"\xff\x2f\x00"

    header = b"MThd" + struct.pack(">IHHH", 6, 0, 1, PPQ)
    return header + b"MTrk" + struct.pack(">I", len(track)) + bytes(track)


def note(seconds, length, key, velocity, channel=0):
    return [
        (seconds, [0x90 | channel, key, velocity]),
        (seconds + length, [0x80 | channel, key, 64]),
    ]


def single_note():
    return note(0.0, 1.0, 48, 100)


def chord_progression():
    events = []
    chords = [(48, 52, 55), (45, 48, 52), (41, 45, 48), (43, 47, 50)]
    for index, chord in enumerate(chords):
        for key in chord:
            events += note(index * 0.75, 0.7, key, 90)
    return events


def legato_bend_modwheel():
    events = []
    keys = [48, 50, 52, 55, 53, 52, 50, 48]
    for index, key in enumerate(keys):
        # Overlapping notes so a mono legato voice glides between them.
        events += note(index * 0.3, 0.35, key, 70 + 6 * index)
    for step in range(33):
        events.append((step * 0.075, [0xB0, 1, min(127, step * 4)]))
    for step in range(17):
        bend = 8192 + int(round(4000 * math.sin(math.pi * step / 16)))
        events.append((1.2 + step * 0.05, [0xE0, bend & 0x7F, (bend >> 7) & 0x7F]))
    return events


def velocity_ramp():
    events = []
    for index in range(8):
        events += note(index * 0.25, 0.2, 60, 16 + 15 * index)
    return events


MIDI = {
    "single_note_c3": single_note(),
    "chord_progression": chord_progression(),
    "legato_bend_modwheel": legato_bend_modwheel(),
    "velocity_ramp": velocity_ramp(),
}


# --------------------------------------------------------------------------

def outputs():
    files = {}
    for name, data in PRESETS.items():
        files[os.path.join(PRESET_DIR, name + ".vital")] = (json.dumps(data, indent=2, sort_keys=True) + "\n").encode()
    for name, events in MIDI.items():
        files[os.path.join(MIDI_DIR, name + ".mid")] = midi_file(events)
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if committed files differ from generated ones")
    args = parser.parse_args()

    stale = []
    for path, data in sorted(outputs().items()):
        existing = open(path, "rb").read() if os.path.exists(path) else None
        if existing == data:
            continue
        if args.check:
            stale.append(os.path.relpath(path, HERE))
            continue
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as handle:
            handle.write(data)
        print("wrote", os.path.relpath(path, HERE))

    if stale:
        print("stale corpus files (re-run migration/corpus/generate.py):", *stale, sep="\n  ")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
