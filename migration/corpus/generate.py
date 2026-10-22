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
PRESET_TEST_DIR = os.path.join(HERE, "preset_tests")

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
    "velocity_filter_poly": preset(
        "velocity filter poly",
        "Oscillator 1 into the analog low-pass; velocity opens each voice's cutoff, so voices that sound together carry different poly modulation.",
        {
            "osc_1_random_phase": 0.0,
            "filter_1_on": 1.0,
            "filter_1_model": 0.0,
            "filter_1_style": 0.0,
            "filter_1_cutoff": 40.0,
            "filter_1_resonance": 0.5,
            "env_1_attack": 0.01,
            "env_1_release": 0.6,
        },
        modulations=[("velocity", "filter_1_cutoff", 0.6)],
    ),
}


# --------------------------------------------------------------------------
# Preset state tests (M2): load -> save cases for the preset loader, written to
# migration/corpus/preset_tests. They are not rendered; the reference's saved
# state for each is captured by the module cases in modules/preset/state.json.
# --------------------------------------------------------------------------

def float_sample(length=256, sample_rate=44100):
    """Like corpus_sample, but as float32 base64, the format before 0.3.7."""
    data = bytearray()
    for i in range(length):
        value = 0.5 * math.sin(2 * math.pi * 3 * i / length) * (1.0 - i / length)
        data += struct.pack("<f", value)
    return {
        "name": "corpus float tone",
        "length": length,
        "sample_rate": sample_rate,
        "samples": base64.b64encode(bytes(data)).decode("ascii"),
    }


def stereo_sample(length=512, sample_rate=48000):
    left = corpus_sample(length, sample_rate)
    pcm = bytearray()
    for i in range(length):
        value = 0.4 * math.sin(2 * math.pi * 5 * i / length)
        pcm += struct.pack("<h", int(round(value * 32767)))
    left["name"] = "corpus stereo tone"
    left["samples_stereo"] = base64.b64encode(bytes(pcm)).decode("ascii")
    return left


def state_preset(version, settings, modulations=(), sample=True, **fields):
    """A preset with raw modulation entries; amounts go in `settings`."""
    settings = dict(settings)
    settings["modulations"] = [dict(entry) for entry in modulations]
    if sample is True:
        settings["sample"] = corpus_sample()
    elif sample is not None:
        settings["sample"] = sample
    data = {"synth_version": version, "author": AUTHOR, "settings": settings}
    data.update(fields)
    if data.get("synth_version") is None:
        del data["synth_version"]
    return data


def mod(source, destination, **extra):
    entry = {"source": source, "destination": destination}
    entry.update(extra)
    return entry


# Settings read by the upgrade steps from 0.2.0 to 0.9.0 (except the sub
# oscillator step at 0.5.0, which only runs when "sub_on" is present).
OLD_SETTINGS = {
    "sub_waveform": 4.0, "sub_octave": -1.0,
    "osc_1_filter_routing": 0.0, "osc_2_filter_routing": 1.0,
    "sample_filter_routing": 1.0, "sub_filter_routing": 0.0,
    "filter_1_style": 2.0, "filter_2_style": 3.0,
    "env_1_attack": 0.3, "env_1_decay": 0.75, "env_1_release": 1.7,
    "env_2_attack": 0.0, "env_2_decay": 2.0, "env_2_release": 0.1,
    "portamento_type": 0.0,
    "osc_1_level": 0.8, "osc_2_level": 0.6, "sub_level": 0.5, "sample_level": 0.9,
    "reverb_damping": 0.25, "reverb_feedback": 0.85,
    "sample_transpose": -12.0, "sample_keytrack": True,
    "osc_1_transpose": 7.0, "osc_1_midi_track": 0.0,
    "osc_2_transpose": -5.0, "osc_2_midi_track": 1.0,
    "effect_chain_order": 1234.0,
    "osc_1_distortion_type": 2.0, "osc_1_distortion_amount": 0.6,
    "osc_2_distortion_type": 8.0, "osc_2_distortion_amount": 0.49,
    "lfo_1_sync_type": 3.0, "lfo_2_sync_type": 1.0,
    "compressor_low_band": 1.0, "compressor_high_band": 0.0,
    "flanger_tempo": 3.0, "phaser_tempo": 2.0, "chorus_tempo": 4.0, "delay_tempo": 8.0,
    "lfo_2_tempo": 6.0, "random_1_tempo": 5.0,
    "delay_sync": 1.0, "delay_frequency": 3.5, "delay_style": 2.0,
    "stereo_routing": -0.4,
    "flanger_center": 20.0,
    "filter_1_model": 6.0, "filter_2_model": 4.0, "filter_2_blend": 1.2,
    "modulation_1_amount": 0.3, "modulation_2_amount": 0.5, "modulation_3_amount": -0.4,
    "modulation_4_amount": 0.7, "modulation_5_amount": 0.2, "modulation_6_amount": 0.6,
    "modulation_7_amount": 0.8, "modulation_8_amount": 0.1, "modulation_8_bipolar": True,
}

OLD_MODULATIONS = [
    mod("lfo_1", "reverb_damping"),
    mod("perlin", "osc_1_wave_frame"),
    mod("env_2", "osc_1_distortion_amount"),
    mod("macro_control_1", "chorus_damping"),
    mod("lfo_2", "flanger_center"),
    mod("env_3", "phaser_center"),
    mod("velocity", "filter_2_blend"),
    mod("random_1", "osc_2_distortion_amount"),
]

LATE_SETTINGS = {
    "flanger_tempo": 3.0, "phaser_tempo": 2.0, "chorus_tempo": 4.0, "delay_tempo": 8.0,
    "delay_sync": 2.0, "delay_frequency": -1.0, "delay_style": 0.0,
    "stereo_routing": 0.6,
    "filter_1_model": 4.0, "filter_2_model": 6.0, "filter_2_style": 1.0,
    "filter_1_blend": 0.7,
}


SUB_SETTINGS = {
    "sub_on": 1.0, "sub_level": 0.6, "sub_pan": -0.25, "sub_transpose": -12.0, "sub_tune": 0.1,
    "filter_1_sub_input": 1.0, "filter_2_sub_input": 1.0, "sub_direct_out": 0.0,
    "filter_1_osc1_input": 1.0, "filter_2_osc1_input": 0.0,
    "filter_1_osc2_input": 0.0, "filter_2_osc2_input": 1.0,
    "filter_1_sample_input": 0.0, "filter_2_sample_input": 0.0,
}


def old_wavetable(name, version):
    wavetable = line_fold_wavetable()
    wavetable["name"] = name
    wavetable["version"] = version
    return wavetable


def bypass_line(name, points, powers, smooth=False):
    flat = []
    for x, y in points:
        flat += [x, y]
    return {"name": name, "num_points": len(points), "points": flat, "powers": powers, "smooth": smooth}


PRESET_TESTS = {
    "controls_and_metadata": state_preset(
        "1.0.6",
        {
            "osc_1_on": True,
            "filter_1_on": 1,
            "filter_1_cutoff": 500,
            "osc_1_level": 0.1,
            "env_1_attack": 1e-3,
            "voice_tune": -0.0,
            "volume": 6000.123456789,
            "not_a_parameter": 5.0,
            "sub_level": 0.3,
            "lfo_0_smooth_mode": 1.0,
            "lfos": [
                bypass_line("custom", [(0.0, 0.0), (0.3, 1.0), (1.0, 0.0)], [1.5, -2.0, 0.0], smooth=True),
                {"num_points": 2, "points": [0.0, 1.0, 1.0, 0.1], "powers": [0.0, 0.25]},
            ],
        },
        sample=stereo_sample(),
        preset_name="ignored: the file name wins",
        comments="Unicode h\u00e9llo \u2713, quotes \" and a\nnewline.",
        preset_style="Lead",
        macro1="Cutoff",
        macro2="   ",
        macro4="",
    ),
    "modulation_slots": state_preset(
        "1.0.6",
        {
            "modulation_1_amount": 0.5, "modulation_3_amount": 0.25, "modulation_4_amount": 0.75,
            "modulation_6_amount": -0.5, "modulation_7_amount": 0.125, "modulation_8_amount": 1.0,
            "modulation_6_bipolar": 1.0,
        },
        modulations=[
            mod("lfo_1", "filter_1_cutoff",
                line_mapping=bypass_line("", [(0.0, 1.0), (0.5, 0.25), (1.0, 0.0)], [0.0, 2.0, 0.0])),
            mod("", ""),
            mod("nope", "osc_1_level", line_mapping=bypass_line("", [(0.0, 0.0), (1.0, 1.0)], [0.0, 0.0])),
            mod("env_2", "osc_1_on"),
            mod("env_2", "sub_level"),
            mod("macro_control_1", "modulation_1_amount"),
            mod("velocity", "filter_1_cutoff",
                line_mapping=bypass_line("Linear", [(0.0, 1.0), (1.0, 0.0)], [0.0, 0.0])),
            mod("lfo_1", "filter_1_cutoff"),
            mod("random", "osc_1_wave_frame",
                line_mapping={"num_points": 2, "points": [0.0, 0.1, 1.0, 0.9], "powers": [0.5, 0.0]}),
        ],
    ),
    "version_1_0_9": state_preset("1.0.9", {"osc_2_on": 1.0}),
    "sub_octave_current": state_preset(
        "1.0.6",
        {key: OLD_SETTINGS[key] for key in [
            "sub_waveform", "sub_octave", "osc_1_filter_routing", "osc_2_filter_routing",
            "sample_filter_routing", "sub_filter_routing", "filter_1_style", "filter_2_style"]},
    ),
    "upgrade_from_0_1_0": state_preset(
        "0.1.0",
        dict(OLD_SETTINGS, wave_tables=[line_fold_wavetable()]),
        modulations=OLD_MODULATIONS,
        sample=float_sample(),
    ),
    "upgrade_from_0_4_3_sync": state_preset(
        "0.4.3",
        dict(LATE_SETTINGS, **{
            "osc_1_distortion_type": 1.0, "osc_1_distortion_amount": 0.4,
            "osc_2_distortion_type": 0.0, "osc_2_distortion_amount": 0.5,
            "compressor_low_band": 0.0, "compressor_high_band": 0.0,
            "modulation_1_amount": 0.6, "modulation_2_amount": -0.2,
        }),
        modulations=[mod("lfo_3", "osc_1_distortion_amount"), mod("env_1", "osc_1_distortion_amount")],
    ),
    "upgrade_from_0_4_6_fm": state_preset(
        "0.4.6",
        dict(LATE_SETTINGS, **{
            "osc_1_distortion_type": 9.0, "osc_1_distortion_amount": 0.36,
            "osc_2_distortion_type": 8.0, "osc_2_distortion_amount": 0.5,
            "osc_1_spectral_morph_type": 9.0, "osc_1_spectral_morph_amount": 0.3,
            "modulation_1_amount": 0.0, "modulation_2_amount": 0.2, "modulation_3_amount": 0.4,
            "modulation_2_bipolar": 0.0, "modulation_4_amount": -0.3, "modulation_4_bipolar": 1.0,
            "modulation_5_amount": 0.5,
        }),
        modulations=[
            mod("lfo_1", "osc_1_distortion_amount"),
            mod("lfo_2", "filter_1_cutoff"),
            mod("lfo_3", "osc_1_distortion_amount"),
            mod("env_2", "osc_2_distortion_amount"),
            mod("env_3", "osc_1_spectral_morph_amount"),
        ],
    ),
    "upgrade_from_0_6_5": state_preset(
        "0.6.5",
        {
            "stereo_mode": 0.0, "stereo_routing": 0.3, "chorus_damping": 0.4,
            "filter_1_model": 6.0, "filter_1_style": 1.0, "filter_2_model": 2.0,
            "filter_fx_model": 6.0, "filter_fx_style": 1.0,
            "osc_2_spectral_morph_type": 9.0, "osc_2_spectral_morph_amount": 0.8,
            "osc_3_spectral_morph_type": 10.0, "flanger_center": 30.0,
            "lfo_8_smooth_mode": 1.0, "modulation_1_amount": 0.5, "modulation_2_amount": 0.25,
        },
        modulations=[mod("lfo_1", "chorus_damping"), mod("lfo_2", "osc_2_spectral_morph_amount")],
    ),
    "upgrade_from_0_4_9_sub": state_preset(
        "0.4.9",
        dict(LATE_SETTINGS, **SUB_SETTINGS, **{
            "sub_waveform": 1.0,
            "wavetables": [old_wavetable("first", "1.0.6"), old_wavetable("second", "1.0.6")],
            "modulation_1_amount": 0.5, "modulation_2_amount": -0.5,
        }),
        modulations=[mod("lfo_1", "sub_level"), mod("env_2", "sub_tune")],
    ),
    "upgrade_from_0_4_9_sub_direct": state_preset(
        "0.4.9",
        dict(dict(LATE_SETTINGS, **SUB_SETTINGS), **{
            "sub_waveform": 5.0, "sub_direct_out": 1.0, "sub_transpose_quantize": 1365.0,
            "modulation_1_amount": 0.3,
        }),
        modulations=[mod("macro_control_2", "sub_pan")],
    ),
    "error_newer_version": state_preset("1.1.0", {}),
    "error_missing_sample": state_preset("1.0.6", {}, sample=None),
    "error_missing_version": state_preset(None, {}),
    "error_upgrade_missing_key": state_preset("0.8.5", {"filter_2_model": 1.0}),
    "error_control_string": state_preset("1.0.6", {"osc_1_level": "loud"}),
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


def velocity_overlap():
    events = []
    # A chord whose notes differ only in velocity, then overlapping notes
    # with contrasting velocities.
    for key, velocity in ((48, 30), (55, 110), (60, 70)):
        events += note(0.0, 1.0, key, velocity)
    for index, (key, velocity) in enumerate(((52, 120), (57, 25), (59, 90), (64, 50))):
        events += note(1.2 + index * 0.25, 0.8, key, velocity)
    return events


MIDI = {
    "single_note_c3": single_note(),
    "chord_progression": chord_progression(),
    "legato_bend_modwheel": legato_bend_modwheel(),
    "velocity_ramp": velocity_ramp(),
    "velocity_overlap": velocity_overlap(),
}


# --------------------------------------------------------------------------

def outputs():
    files = {}
    for name, data in PRESETS.items():
        files[os.path.join(PRESET_DIR, name + ".vital")] = (json.dumps(data, indent=2, sort_keys=True) + "\n").encode()
    for name, events in MIDI.items():
        files[os.path.join(MIDI_DIR, name + ".mid")] = midi_file(events)
    for name, data in PRESET_TESTS.items():
        path = os.path.join(PRESET_TEST_DIR, name + ".vital")
        files[path] = (json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode()
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
