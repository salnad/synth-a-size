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
BENCH_DIR = os.path.join(HERE, "bench")
TUNING_DIR = os.path.join(HERE, "tunings")

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


# Control values of the enumerated parameters the feature-stacking presets use.
DISTORTION = {
    "sync": 1, "formant": 2, "quantize": 3, "bend": 4, "squeeze": 5, "pulse": 6,
    "fm_a": 7, "fm_b": 8, "fm_sample": 9, "rm_a": 10, "rm_b": 11, "rm_sample": 12,
}
MORPH = {
    "vocode": 1, "form_scale": 2, "harmonic_scale": 3, "inharmonic_scale": 4, "smear": 5,
    "random_amplitudes": 6, "low_pass": 7, "high_pass": 8, "phase_disperse": 9,
    "shepard_tone": 10, "skew": 11,
}
FILTER_MODEL = {
    "analog": 0, "dirty": 1, "ladder": 2, "digital": 3, "diode": 4, "formant": 5, "comb": 6, "phase": 7,
}
DESTINATION = {"filter_1": 0, "filter_2": 1, "dual_filters": 2, "effects": 3, "direct_out": 4}
EFFECTS = ["chorus", "compressor", "delay", "distortion", "eq", "filter_fx", "flanger", "phaser", "reverb"]
RANDOM_STYLE = {"perlin": 0, "sample_and_hold": 1, "sin_interpolate": 2, "lorenz": 3}
LFO_SYNC = {"time": 0, "tempo": 1, "dotted": 2, "triplet": 3, "keytrack": 4}
LFO_SYNC_TYPE = {"trigger": 0, "sync": 1, "envelope": 2, "sustain_envelope": 3, "loop_point": 4, "loop_hold": 5}


def effect_order(names):
    """effect_chain_order for an effect order (encodeOrderToFloat)."""
    order = [EFFECTS.index(name) for name in names]
    assert sorted(order) == list(range(len(EFFECTS)))
    code = 0
    for i in range(1, len(order)):
        code = code * (i + 1) + sum(order[i] < order[j] for j in range(i))
    return float(code)


def oscillator(index, distortion, morph, **controls):
    """osc_<index>_* controls with a distortion (or None) and a spectral morph."""
    settings = {
        "osc_%d_on" % index: 1.0,
        "osc_%d_random_phase" % index: 0.0,
        "osc_%d_distortion_type" % index: float(DISTORTION[distortion] if distortion else 0),
        "osc_%d_spectral_morph_type" % index: float(MORPH[morph]),
    }
    for key, value in controls.items():
        settings["osc_%d_%s" % (index, key)] = float(value)
    return settings


def preset(name, comments, settings, modulations=(), wavetables=None, lfos=None):
    """Build a preset. Unlisted controls load as their parameter defaults;
    omitted wavetables / LFO shapes keep the engine's init state.

    A modulation is (source, destination, amount) or (source, destination,
    amount, extras): extras may set the slot's "bipolar", "power" and
    "stereo" controls and its "line_mapping" (a line())."""
    settings = dict(settings)
    mods = []
    for index, modulation in enumerate(modulations, start=1):
        source, destination, amount = modulation[:3]
        extras = modulation[3] if len(modulation) > 3 else {}
        entry = {"source": source, "destination": destination}
        if "line_mapping" in extras:
            entry["line_mapping"] = extras["line_mapping"]
        mods.append(entry)
        settings["modulation_%d_amount" % index] = float(amount)
        for key in ("bipolar", "power", "stereo"):
            if key in extras:
                settings["modulation_%d_%s" % (index, key)] = float(extras[key])
    settings["modulations"] = mods
    settings["sample"] = corpus_sample()
    if wavetables is not None:
        settings["wavetables"] = wavetables
    if lfos is not None:
        settings["lfos"] = lfos
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
    # Feature-stacking presets: each stacks engine features that no other
    # render case reaches, so a full render checks their wiring, preset
    # plumbing and modulation together.
    "osc_stack_serial": preset(
        "osc stack serial",
        "Three oscillators (sync + vocode, sample FM + inharmonic scale, ring mod by oscillator A + harmonic scale) "
        "and the sample oscillator; the dirty filter feeds the digital filter in series.",
        {
            **oscillator(1, "sync", "vocode", distortion_amount=0.6, spectral_morph_amount=0.4,
                         unison_voices=3, unison_detune=2.0, destination=DESTINATION["filter_1"]),
            **oscillator(2, "fm_sample", "inharmonic_scale", transpose=7, distortion_amount=0.45,
                         spectral_morph_amount=0.6, level=0.5, destination=DESTINATION["filter_1"]),
            **oscillator(3, "rm_a", "harmonic_scale", transpose=-12, distortion_amount=0.5,
                         spectral_morph_amount=0.7, level=0.5, destination=DESTINATION["filter_2"]),
            "sample_on": 1.0,
            "sample_keytrack": 1.0,
            "sample_level": 0.4,
            "sample_destination": float(DESTINATION["filter_2"]),
            "filter_1_on": 1.0,
            "filter_1_model": float(FILTER_MODEL["dirty"]),
            "filter_1_style": 0.0,
            "filter_1_cutoff": 72.0,
            "filter_1_resonance": 0.55,
            "filter_1_drive": 6.0,
            "filter_2_on": 1.0,
            "filter_2_model": float(FILTER_MODEL["digital"]),
            "filter_2_style": 1.0,
            "filter_2_blend": 1.0,
            "filter_2_cutoff": 80.0,
            "filter_2_resonance": 0.4,
            "filter_2_filter_input": 1.0,
            "env_1_attack": 0.02,
            "env_1_release": 0.5,
            "env_2_attack": 0.3,
            "env_2_decay": 0.8,
            "env_2_sustain": 0.2,
        },
        modulations=[
            ("env_2", "osc_1_spectral_morph_amount", 0.45),
            ("env_2", "filter_1_cutoff", 0.25),
            ("lfo_1", "osc_2_distortion_amount", 0.3),
            ("velocity", "osc_3_distortion_amount", 0.3),
        ],
    ),
    "osc_formant_comb": preset(
        "osc formant comb",
        "Formant + smear, quantize + random amplitudes and bend + low-pass oscillators; the formant filter feeds the "
        "comb filter in series, at 8x oversampling.",
        {
            **oscillator(1, "formant", "smear", distortion_amount=0.55, spectral_morph_amount=0.5,
                         level=0.4, destination=DESTINATION["filter_2"]),
            **oscillator(2, "quantize", "random_amplitudes", transpose=12, distortion_amount=0.4,
                         spectral_morph_amount=0.6, level=0.3, destination=DESTINATION["filter_2"]),
            **oscillator(3, "bend", "low_pass", transpose=-12, distortion_amount=0.7,
                         spectral_morph_amount=0.55, level=0.35, destination=DESTINATION["filter_1"]),
            "filter_1_on": 1.0,
            "filter_1_model": float(FILTER_MODEL["comb"]),
            "filter_1_style": 0.0,
            "filter_1_blend": 0.6,
            "filter_1_cutoff": 64.0,
            "filter_1_resonance": 0.6,
            "filter_1_filter_input": 1.0,
            "filter_2_on": 1.0,
            "filter_2_model": float(FILTER_MODEL["formant"]),
            "filter_2_formant_x": 0.3,
            "filter_2_formant_y": 0.7,
            "filter_2_formant_resonance": 0.8,
            "filter_2_formant_transpose": 3.0,
            "filter_2_formant_spread": 0.2,
            "env_3_attack": 0.4,
            "env_3_decay": 1.0,
            "env_3_sustain": 0.3,
            "env_3_release": 0.4,
            "lfo_2_sync": float(LFO_SYNC["time"]),
            "lfo_2_frequency": 2.0,
            "voice_amplitude": 0.6,
            "oversampling": 3.0,
        },
        modulations=[
            ("env_3", "filter_2_formant_x", 0.5),
            ("lfo_2", "filter_1_cutoff", 0.2),
        ],
    ),
    "osc_diode_phase": preset(
        "osc diode phase",
        "Squeeze + high-pass, pulse width + phase disperse and FM by oscillator B + Shepard tone oscillators into the "
        "diode and phaser filters in parallel, with envelope 1 and 3 shaped, at 1x oversampling.",
        {
            **oscillator(1, "squeeze", "high_pass", distortion_amount=0.6, spectral_morph_amount=0.35,
                         destination=DESTINATION["dual_filters"]),
            **oscillator(2, "pulse", "phase_disperse", transpose=-12, distortion_amount=0.3,
                         spectral_morph_amount=0.7, level=0.5, destination=DESTINATION["filter_1"]),
            **oscillator(3, "fm_b", "shepard_tone", transpose=19, distortion_amount=0.1,
                         spectral_morph_amount=0.5, level=0.35, destination=DESTINATION["filter_2"]),
            "filter_1_on": 1.0,
            "filter_1_model": float(FILTER_MODEL["diode"]),
            "filter_1_style": 1.0,
            "filter_1_cutoff": 66.0,
            "filter_1_resonance": 0.7,
            "filter_1_drive": 8.0,
            "filter_2_on": 1.0,
            "filter_2_model": float(FILTER_MODEL["phase"]),
            "filter_2_cutoff": 70.0,
            "filter_2_resonance": 0.6,
            "filter_2_keytrack": 0.5,
            "env_1_attack": 0.1,
            "env_1_decay": 0.6,
            "env_1_sustain": 0.6,
            "env_1_release": 0.8,
            "env_3_attack": 0.0,
            "env_3_decay": 1.5,
            "env_3_sustain": 0.0,
            "oversampling": 0.0,
        },
        modulations=[
            ("env_3", "filter_2_cutoff", 0.4),
            ("note", "filter_1_cutoff", 0.3),
        ],
    ),
    "fx_chain_reordered": preset(
        "fx chain reordered",
        "Every effect on, in a custom order (EQ, filter FX, distortion, multiband compressor, flanger, phaser, chorus, "
        "delay, reverb), with tempo-synced modulation effects and LFO, at 4x oversampling.",
        {
            "osc_1_unison_voices": 4.0,
            "osc_1_unison_detune": 2.5,
            "osc_2_on": 1.0,
            "osc_2_transpose": -12.0,
            "osc_2_level": 0.4,
            "env_1_attack": 0.01,
            "env_1_release": 0.25,
            "effect_chain_order": effect_order(
                ["eq", "filter_fx", "distortion", "compressor", "flanger", "phaser", "chorus", "delay", "reverb"]),
            "eq_on": 1.0,
            "eq_low_mode": 1.0,
            "eq_low_cutoff": 36.0,
            "eq_band_cutoff": 84.0,
            "eq_band_gain": 6.0,
            "eq_high_mode": 1.0,
            "eq_high_cutoff": 110.0,
            "eq_high_gain": -4.0,
            "filter_fx_on": 1.0,
            "filter_fx_model": float(FILTER_MODEL["digital"]),
            "filter_fx_style": 2.0,
            "filter_fx_cutoff": 70.0,
            "filter_fx_resonance": 0.5,
            "filter_fx_mix": 0.7,
            "distortion_on": 1.0,
            "distortion_type": 2.0,
            "distortion_drive": 4.0,
            "distortion_mix": 0.5,
            "compressor_on": 1.0,
            "compressor_enabled_bands": 0.0,
            "compressor_low_gain": 6.0,
            "compressor_band_gain": 4.0,
            "compressor_high_gain": 6.0,
            "compressor_attack": 0.3,
            "compressor_release": 0.6,
            "compressor_mix": 0.8,
            "flanger_on": 1.0,
            "flanger_sync": float(LFO_SYNC["tempo"]),
            "flanger_tempo": 5.0,
            "flanger_feedback": 0.6,
            "flanger_dry_wet": 0.4,
            "phaser_on": 1.0,
            "phaser_sync": float(LFO_SYNC["dotted"]),
            "phaser_tempo": 4.0,
            "phaser_feedback": 0.6,
            "phaser_dry_wet": 0.6,
            "chorus_on": 1.0,
            "chorus_sync": float(LFO_SYNC["triplet"]),
            "chorus_tempo": 5.0,
            "chorus_dry_wet": 0.4,
            "delay_on": 1.0,
            "delay_sync": float(LFO_SYNC["tempo"]),
            "delay_tempo": 7.0,
            "delay_style": 1.0,
            "delay_feedback": 0.45,
            "delay_dry_wet": 0.3,
            "reverb_on": 1.0,
            "reverb_dry_wet": 0.25,
            "reverb_decay_time": -1.0,
            "lfo_1_sync": float(LFO_SYNC["tempo"]),
            "lfo_1_tempo": 8.0,
            "oversampling": 2.0,
        },
        modulations=[("lfo_1", "phaser_center", 0.3)],
    ),
    "random_macro_shapes": preset(
        "random macro shapes",
        "All four random LFO styles, two authored LFO shapes, macros and remapped (bipolar, power, stereo, custom "
        "line map) modulations over ring mod by the sample + skew, ring mod by oscillator B + form scale and "
        "vocode oscillators.",
        {
            **oscillator(1, "rm_sample", "skew", distortion_amount=0.4, spectral_morph_amount=0.5),
            **oscillator(2, "rm_b", "form_scale", transpose=5, distortion_amount=0.5,
                         spectral_morph_amount=0.4, level=0.5),
            **oscillator(3, None, "vocode", transpose=-7, spectral_morph_amount=0.6, level=0.4),
            "sample_on": 1.0,
            "sample_keytrack": 1.0,
            "sample_level": 0.2,
            "sample_destination": float(DESTINATION["direct_out"]),
            "filter_1_on": 1.0,
            "filter_1_cutoff": 60.0,
            "filter_1_resonance": 0.5,
            "random_1_style": float(RANDOM_STYLE["perlin"]),
            "random_1_sync": float(LFO_SYNC["time"]),
            "random_1_frequency": 1.5,
            "random_2_style": float(RANDOM_STYLE["sample_and_hold"]),
            "random_2_sync": float(LFO_SYNC["tempo"]),
            "random_2_tempo": 9.0,
            "random_3_style": float(RANDOM_STYLE["sin_interpolate"]),
            "random_3_stereo": 1.0,
            "random_3_sync": float(LFO_SYNC["time"]),
            "random_3_frequency": 0.5,
            "random_4_style": float(RANDOM_STYLE["lorenz"]),
            "random_4_sync": float(LFO_SYNC["time"]),
            "random_4_frequency": 2.0,
            "lfo_1_sync": float(LFO_SYNC["tempo"]),
            "lfo_1_tempo": 6.0,
            "lfo_1_sync_type": float(LFO_SYNC_TYPE["trigger"]),
            "lfo_2_sync": float(LFO_SYNC["dotted"]),
            "lfo_2_tempo": 5.0,
            "lfo_2_sync_type": float(LFO_SYNC_TYPE["sync"]),
            "lfo_2_stereo": 0.25,
            "lfo_3_sync": float(LFO_SYNC["keytrack"]),
            "macro_control_1": 0.7,
            "macro_control_2": 0.4,
            "macro_control_3": 0.5,
        },
        modulations=[
            ("random_1", "osc_1_spectral_morph_amount", 0.4),
            ("random_2", "filter_1_cutoff", 0.3, {"bipolar": 1}),
            ("random_3", "osc_2_pan", 0.6, {"stereo": 1}),
            ("random_4", "osc_3_spectral_morph_amount", 0.4),
            ("lfo_1", "filter_1_resonance", 0.3, {
                "power": 2.0,
                "line_mapping": line([(0.0, 1.0), (0.4, 0.2), (1.0, 0.0)], powers=[1.5, -2.0, 0.0]),
            }),
            ("lfo_2", "osc_1_level", 0.25, {"bipolar": 1, "stereo": 1}),
            ("lfo_3", "osc_2_spectral_morph_amount", 0.2),
            ("macro_control_1", "filter_1_cutoff", 0.25, {
                "line_mapping": line([(0.0, 1.0), (0.5, 0.75), (1.0, 0.0)], smooth=True),
            }),
            ("macro_control_2", "osc_3_level", 0.3, {"bipolar": 1}),
            ("macro_control_3", "osc_1_distortion_amount", 0.3, {"power": -3.0}),
        ],
        lfos=[
            line([(0.0, 1.0), (0.2, 0.3), (0.45, 0.8), (0.7, 0.0), (1.0, 1.0)],
                 powers=[0.0, 3.0, -2.0, 1.0, 0.0], name="corpus steps"),
            line([(0.0, 0.5), (0.25, 1.0), (0.6, 0.1), (1.0, 0.5)], smooth=True, name="corpus swell"),
        ],
    ),
    "mpe_expression": preset(
        "mpe expression",
        "Per-channel expression: aftertouch opens the ladder filter, slide scans an authored wavetable, lift sets "
        "the resonance in release, per-channel pitch bend, with the sustain pedal, envelope 3 and FM of oscillator 2 by "
        "oscillator A in play.",
        {
            "osc_1_random_phase": 0.0,
            "osc_1_wave_frame": 0.0,
            "osc_2_on": 1.0,
            "osc_2_random_phase": 0.0,
            "osc_2_transpose": 12.0,
            "osc_2_level": 0.3,
            "osc_2_distortion_type": float(DISTORTION["fm_a"]),
            "osc_2_distortion_amount": 0.3,
            "pitch_bend_range": 24.0,
            "filter_1_on": 1.0,
            "filter_1_model": float(FILTER_MODEL["ladder"]),
            "filter_1_cutoff": 50.0,
            "filter_1_resonance": 0.5,
            "env_1_attack": 0.02,
            "env_1_sustain": 0.7,
            "env_1_release": 1.2,
            "env_3_attack": 0.5,
            "env_3_decay": 0.5,
            "env_3_sustain": 0.5,
        },
        modulations=[
            ("aftertouch", "filter_1_cutoff", 0.5),
            ("slide", "osc_1_wave_frame", 0.8),
            ("lift", "filter_1_resonance", 0.4),
            ("velocity", "osc_1_level", 0.3),
            ("mod_wheel", "osc_2_level", 0.4),
            ("env_3", "osc_2_tune", 0.2),
        ],
        wavetables=[line_fold_wavetable()],
    ),
    "tuned_glide": preset(
        "tuned glide",
        "Two oscillators gliding through an authored just scale and keyboard map; the filter and LFO 3 keytrack the "
        "tuned pitch and the note drives the cutoff through a bipolar custom line map. Rendered with "
        "tunings/corpus_septimal.scl and tunings/corpus_septimal.kbm.",
        {
            "osc_1_random_phase": 0.0,
            "osc_2_on": 1.0,
            "osc_2_random_phase": 0.0,
            "osc_2_transpose": 7.0,
            "osc_2_level": 0.4,
            "portamento_time": -3.0,
            "portamento_scale": 1.0,
            "filter_1_on": 1.0,
            "filter_1_cutoff": 55.0,
            "filter_1_resonance": 0.45,
            "filter_1_keytrack": 0.6,
            "lfo_3_sync": float(LFO_SYNC["keytrack"]),
            "lfo_3_keytrack_transpose": -24.0,
            "env_1_attack": 0.01,
            "env_1_release": 0.4,
        },
        modulations=[
            ("note", "filter_1_cutoff", 0.4, {
                "bipolar": 1,
                "line_mapping": line([(0.0, 1.0), (0.3, 0.6), (0.7, 0.4), (1.0, 0.0)], powers=[2.0, 0.0, -2.0, 0.0]),
            }),
            ("lfo_3", "osc_2_tune", 0.1),
        ],
    ),
}


# --------------------------------------------------------------------------
# Tuning files, written to migration/corpus/tunings
# --------------------------------------------------------------------------

TUNINGS = {
    # Seven-note septimal just scale; two degrees given in cents, the rest as
    # ratios, so both Scala pitch forms are read.
    "corpus_septimal.scl": """! corpus_septimal.scl
!
synth-a-size corpus: seven-note septimal just scale
 7
!
 8/7
 315.641
 4/3
 3/2
 813.686 cents
 7/4
 2/1
""",
    # Twelve keys per period: five unmapped ("x") keys, the scale's middle at
    # MIDI 62 and A4 (69) at 432 Hz.
    "corpus_septimal.kbm": """! corpus_septimal.kbm
! map size
12
! first and last MIDI note
0
127
! middle note
62
! reference note and frequency
69
432.0
! formal octave degree
7
! mapping
0
x
1
2
x
3
4
5
x
6
x
x
""",
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


def wide_arpeggio():
    events = []
    keys = [36, 48, 55, 64, 72, 79, 67, 60, 43, 52, 71, 84]
    for index, key in enumerate(keys):
        events += note(index * 0.22, 0.6, key, 40 + (index * 37) % 81)
    return events


def held_pad():
    events = []
    for key in (48, 55, 62):
        events += note(0.0, 2.0, key, 80)
    for key in (50, 57, 65):
        events += note(2.2, 2.0, key, 95)
    events += note(1.0, 2.5, 74, 60)
    return events


def ramp(seconds, length, steps, start, end, message):
    """steps + 1 evenly spaced messages from value start to end."""
    return [(seconds + length * i / steps, message(int(round(start + (end - start) * i / steps))))
            for i in range(steps + 1)]


def pitch_bend(channel):
    return lambda value: [0xE0 | channel, value & 0x7F, (value >> 7) & 0x7F]


def scale_run():
    """A chromatic run over the mapped and unmapped keys of the keyboard map,
    each note overlapping the next so the voices glide, then a chord."""
    events = []
    for index, key in enumerate(range(50, 75)):
        events += note(index * 0.12, 0.16, key, 70 + (index * 13) % 50)
    for key in (55, 62, 69):
        events += note(3.2, 1.0, key, 90)
    return events


def mpe_expression():
    """Notes on their own channels with per-channel pitch bend, pressure and
    slide (CC 74) and contrasting release velocities (lift), then a sustain
    pedal passage with polyphonic aftertouch and the mod wheel."""
    events = []
    for channel, key, start, length, bend, release in (
            (1, 60, 0.0, 1.6, 10000, 100),
            (2, 64, 0.4, 1.6, 5000, 15),
            (3, 67, 0.8, 1.6, 8192, 120)):
        events.append((start, [0x90 | channel, key, 90]))
        events += ramp(start + 0.2, 0.8, 16, 8192, bend, pitch_bend(channel))
        events += ramp(start + 0.1, 1.3, 13, 0, 110, lambda v, c=channel: [0xD0 | c, v])
        events += ramp(start, 1.4, 14, 127 if channel == 2 else 0, 0 if channel == 2 else 127,
                       lambda v, c=channel: [0xB0 | c, 74, v])
        events.append((start + length, [0x80 | channel, key, release]))
    events.append((2.6, [0xB0, 64, 127]))
    events += note(2.7, 0.3, 48, 100)
    events += note(2.9, 0.3, 55, 70)
    events += ramp(2.9, 0.6, 12, 0, 120, lambda v: [0xA0, 55, v])
    events += ramp(2.6, 1.0, 10, 0, 127, lambda v: [0xB0, 1, v])
    events.append((3.8, [0xB0, 64, 0]))
    return events


MIDI = {
    "wide_arpeggio": wide_arpeggio(),
    "held_pad": held_pad(),
    "mpe_expression": mpe_expression(),
    "scale_run": scale_run(),
    "single_note_c3": single_note(),
    "chord_progression": chord_progression(),
    "legato_bend_modwheel": legato_bend_modwheel(),
    "velocity_ramp": velocity_ramp(),
    "velocity_overlap": velocity_overlap(),
}


# --------------------------------------------------------------------------
# Benchmark content (migration/BENCHMARKS.md), written to bench/. Not
# rendered by the parity corpus. Notes are held past the end of every
# benchmark render, so each one measures a steady voice count.
# --------------------------------------------------------------------------

def bench_oversampling(oversampling):
    """Unison into the ladder filter with distortion; `oversampling` is the
    control's index (0 = 1x, 1 = 2x, 2 = 4x, 3 = 8x)."""
    return preset(
        "bench oversampling %dx" % (1 << oversampling),
        "Benchmark: four-voice unison into the ladder filter, distortion on, oversampling %dx." % (1 << oversampling),
        {
            "oversampling": float(oversampling),
            "osc_1_unison_voices": 4.0,
            "osc_1_unison_detune": 2.0,
            "filter_1_on": 1.0,
            "filter_1_model": 2.0,
            "filter_1_cutoff": 70.0,
            "filter_1_resonance": 0.5,
            "distortion_on": 1.0,
            "distortion_type": 2.0,
            "distortion_drive": 8.0,
        },
    )


BENCH_PRESETS = {
    "poly_unison": preset(
        "bench poly unison",
        "Benchmark: 32-voice polyphony, oscillator 1 with 8 unison voices and oscillator 2 with 4, into the analog low-pass.",
        {
            "polyphony": 32.0,
            "osc_1_level": 0.3,
            "osc_1_unison_voices": 8.0,
            "osc_1_unison_detune": 2.5,
            "osc_1_stereo_spread": 0.8,
            "osc_2_on": 1.0,
            "osc_2_level": 0.2,
            "osc_2_transpose": -12.0,
            "osc_2_unison_voices": 4.0,
            "filter_1_on": 1.0,
            "filter_1_model": 0.0,
            "filter_1_cutoff": 70.0,
            "filter_1_resonance": 0.4,
            "env_1_attack": 0.02,
            "env_1_release": 0.5,
        },
    ),
    "heavy_fx": preset(
        "bench heavy fx",
        "Benchmark: a four-voice unison patch through all nine effects (chorus, compressor, delay, distortion, EQ, filter, flanger, phaser, reverb).",
        {
            "osc_1_unison_voices": 4.0,
            "osc_1_unison_detune": 2.0,
            "filter_1_on": 1.0,
            "filter_1_cutoff": 80.0,
            "chorus_on": 1.0,
            "chorus_dry_wet": 0.5,
            "compressor_on": 1.0,
            "delay_on": 1.0,
            "delay_dry_wet": 0.3,
            "delay_feedback": 0.5,
            "distortion_on": 1.0,
            "distortion_type": 1.0,
            "distortion_drive": 6.0,
            "distortion_mix": 0.5,
            "eq_on": 1.0,
            "eq_low_gain": 3.0,
            "eq_high_gain": -3.0,
            "filter_fx_on": 1.0,
            "filter_fx_model": 1.0,
            "filter_fx_cutoff": 90.0,
            "flanger_on": 1.0,
            "flanger_dry_wet": 0.4,
            "phaser_on": 1.0,
            "phaser_dry_wet": 0.4,
            "reverb_on": 1.0,
            "reverb_dry_wet": 0.35,
            "reverb_decay_time": 1.0,
        },
    ),
    "oversampling_1x": bench_oversampling(0),
    "oversampling_2x": bench_oversampling(1),
    "oversampling_4x": bench_oversampling(2),
    "oversampling_8x": bench_oversampling(3),
    "mod_matrix": preset(
        "bench mod matrix",
        "Benchmark: two unison oscillators and both filters under 16 modulation connections from LFOs, envelopes, random LFOs, velocity and note.",
        {
            "osc_1_unison_voices": 4.0,
            "osc_2_on": 1.0,
            "osc_2_level": 0.4,
            "osc_2_unison_voices": 2.0,
            "filter_1_on": 1.0,
            "filter_1_cutoff": 60.0,
            "filter_2_on": 1.0,
            "filter_2_model": 1.0,
            "filter_2_cutoff": 80.0,
            "lfo_1_sync": 0.0,
            "lfo_1_frequency": 1.5,
            "lfo_2_sync": 0.0,
            "lfo_2_frequency": 0.3,
            "lfo_3_sync": 0.0,
            "lfo_3_frequency": 2.5,
            "lfo_4_sync": 0.0,
            "lfo_4_frequency": -1.0,
            "env_2_attack": 0.5,
            "env_2_decay": 1.0,
            "env_3_attack": 1.2,
        },
        modulations=[
            ("lfo_1", "filter_1_cutoff", 0.4),
            ("lfo_2", "osc_1_wave_frame", 0.6),
            ("lfo_3", "osc_2_pan", 0.5),
            ("lfo_4", "osc_1_unison_detune", 0.3),
            ("env_2", "filter_1_resonance", 0.4),
            ("env_2", "filter_2_cutoff", -0.3),
            ("env_3", "osc_2_wave_frame", 0.7),
            ("random_1", "osc_1_pan", 0.3),
            ("random_2", "osc_2_tune", 0.1),
            ("velocity", "osc_1_level", 0.3),
            ("note", "filter_2_cutoff", 0.2),
            ("lfo_1", "osc_2_level", 0.2),
            ("lfo_2", "filter_2_resonance", 0.3),
            ("lfo_3", "osc_1_spectral_morph_amount", 0.4),
            ("env_3", "osc_1_unison_detune", 0.2),
            ("mod_wheel", "filter_1_cutoff", 0.2),
        ],
    ),
}


def held_chord(keys, length=60.0, velocity=100):
    events = []
    for key in keys:
        events += note(0.0, length, key, velocity)
    return events


BENCH_MIDI = {
    "held_note": held_chord([48]),
    "held_chord_4": held_chord([48, 55, 60, 64]),
    "held_chord_8": held_chord([36, 43, 48, 52, 55, 60, 64, 67]),
    "held_chord_16": held_chord([36 + 3 * i for i in range(16)]),
    "held_chord_32": held_chord([30 + 2 * i for i in range(32)]),
}


# --------------------------------------------------------------------------

def outputs():
    files = {}
    for name, data in PRESETS.items():
        files[os.path.join(PRESET_DIR, name + ".vital")] = (json.dumps(data, indent=2, sort_keys=True) + "\n").encode()
    for name, events in MIDI.items():
        files[os.path.join(MIDI_DIR, name + ".mid")] = midi_file(events)
    for name, text in TUNINGS.items():
        files[os.path.join(TUNING_DIR, name)] = text.encode()
    for name, data in PRESET_TESTS.items():
        path = os.path.join(PRESET_TEST_DIR, name + ".vital")
        files[path] = (json.dumps(data, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode()
    for name, data in BENCH_PRESETS.items():
        files[os.path.join(BENCH_DIR, "presets", name + ".vital")] = (json.dumps(data, indent=2, sort_keys=True) + "\n").encode()
    for name, events in BENCH_MIDI.items():
        files[os.path.join(BENCH_DIR, "midi", name + ".mid")] = midi_file(events)
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
