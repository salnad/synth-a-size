// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Controls the editor keeps itself: upstream's sliders that aren't synth
//! parameters (each LFO section's grid sizes, paint pattern and paint
//! button, from `vital/src/interface/editor_sections/lfo_section.cpp` with
//! the limits in `editor_components/line_editor.h`). They behave as sliders
//! through [`crate::host::Host`], which stores their values here instead of
//! in the host, so they aren't saved or automated, as upstream's aren't.
//! The preset browser's style filters
//! (`vital/src/interface/editor_sections/preset_browser.cpp`) are kept here
//! too, and so are the wavetable editor's Wave Source overlay controls
//! (`vital/src/interface/wavetable/overlays/wave_source_overlay.cpp`): each
//! oscillator's editor has an overlay for Wave Sources and another for
//! Shepard Tone sources, each with its own grid sizes and interpolation
//! selector.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use sas_preset::registry::{ParamInfo, ValueScale};

/// `LfoSection`'s `kDefaultGridSizeX` / `kDefaultGridSizeY`.
const DEFAULT_GRID_SIZE_X: f32 = 8.0;
const DEFAULT_GRID_SIZE_Y: f32 = 1.0;
/// `LineEditor::kMaxGridSizeX` / `kMaxGridSizeY`.
pub const MAX_GRID_SIZE_X: f32 = 32.0;
pub const MAX_GRID_SIZE_Y: f32 = 24.0;
/// `LfoSection::kDown`, the pattern a section starts with.
const DEFAULT_PAINT_PATTERN: f32 = 2.0;
/// `LfoSection::kNumPaintPatterns`.
const NUM_PAINT_PATTERNS: f32 = 5.0;
/// `vital::kNumLfos`.
const NUM_LFOS: usize = 8;
/// `WaveSourceOverlay::kDefaultXGrid` / `kDefaultYGrid`.
pub const DEFAULT_WAVE_SOURCE_GRID_X: f32 = 6.0;
pub const DEFAULT_WAVE_SOURCE_GRID_Y: f32 = 4.0;
/// `WavetableComponentOverlay::kMaxGrid`.
pub const MAX_WAVE_SOURCE_GRID: f32 = 16.0;
/// The Wave Source overlay's interpolation types (`interpolation_types`).
pub const WAVE_SOURCE_INTERPOLATIONS: [&str; 5] = [
    "None",
    "Waveform Blend",
    "Spectral Blend",
    "Smooth Waveform Blend",
    "Smooth Spectral Blend",
];

/// One Wave Source overlay's controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveSourceControls {
    pub grid_x: &'static str,
    pub grid_y: &'static str,
    pub interpolation: &'static str,
}

macro_rules! wave_source_controls {
    ($prefix:literal) => {
        WaveSourceControls {
            grid_x: concat!($prefix, "wave_source_horizontal_grid"),
            grid_y: concat!($prefix, "wave_source_vertical_grid"),
            interpolation: concat!($prefix, "wave_source_interpolation"),
        }
    };
}

/// Each oscillator's Wave Source overlay controls: `[oscillator][0]` for
/// Wave Sources, `[oscillator][1]` for Shepard Tone sources.
pub const WAVE_SOURCE_CONTROLS: [[WaveSourceControls; 2]; 3] = [
    [
        wave_source_controls!("osc_1_"),
        wave_source_controls!("osc_1_shepard_"),
    ],
    [
        wave_source_controls!("osc_2_"),
        wave_source_controls!("osc_2_shepard_"),
    ],
    [
        wave_source_controls!("osc_3_"),
        wave_source_controls!("osc_3_shepard_"),
    ],
];

/// `strings::kPresetStyleNames`: the preset browser's style filters.
pub const PRESET_STYLE_NAMES: [&str; 9] = [
    "Bass",
    "Lead",
    "Keys",
    "Pad",
    "Percussion",
    "Sequence",
    "Experiment",
    "SFX",
    "Template",
];

/// The preset browser's style filter buttons, on while filtering by that
/// style.
pub const PRESET_STYLE_FILTERS: [&str; 9] = [
    "preset_style_bass",
    "preset_style_lead",
    "preset_style_keys",
    "preset_style_pad",
    "preset_style_percussion",
    "preset_style_sequence",
    "preset_style_experiment",
    "preset_style_sfx",
    "preset_style_template",
];

/// The save section's style buttons, of which at most one is on.
pub const SAVE_STYLE_BUTTONS: [&str; 9] = [
    "save_style_bass",
    "save_style_lead",
    "save_style_keys",
    "save_style_pad",
    "save_style_percussion",
    "save_style_sequence",
    "save_style_experiment",
    "save_style_sfx",
    "save_style_template",
];

fn local_info(name: &str, min: f32, max: f32, default_value: f32, display_name: &str) -> ParamInfo {
    ParamInfo {
        name: name.to_string(),
        version_added: 0,
        min,
        max,
        default_value,
        post_offset: 0.0,
        display_multiply: 1.0,
        value_scale: ValueScale::Indexed,
        display_invert: false,
        display_units: "",
        display_name: display_name.to_string(),
        string_table: None,
        local_description: String::new(),
    }
}

fn infos() -> &'static HashMap<String, ParamInfo> {
    static INFOS: OnceLock<HashMap<String, ParamInfo>> = OnceLock::new();
    INFOS.get_or_init(|| {
        let mut infos = HashMap::new();
        for i in 0..NUM_LFOS {
            for info in [
                local_info(
                    GRID_SIZE_X[i],
                    1.0,
                    MAX_GRID_SIZE_X,
                    DEFAULT_GRID_SIZE_X,
                    "X Grid",
                ),
                local_info(
                    GRID_SIZE_Y[i],
                    1.0,
                    MAX_GRID_SIZE_Y,
                    DEFAULT_GRID_SIZE_Y,
                    "Y Grid",
                ),
                local_info(
                    PAINT_PATTERN[i],
                    0.0,
                    NUM_PAINT_PATTERNS - 1.0,
                    DEFAULT_PAINT_PATTERN,
                    "Paint Pattern",
                ),
                local_info(PAINT[i], 0.0, 1.0, 0.0, "Paint"),
            ] {
                infos.insert(info.name.clone(), info);
            }
        }
        for controls in WAVE_SOURCE_CONTROLS.iter().flatten() {
            for info in [
                local_info(
                    controls.grid_x,
                    0.0,
                    MAX_WAVE_SOURCE_GRID,
                    DEFAULT_WAVE_SOURCE_GRID_X,
                    "Grid X",
                ),
                local_info(
                    controls.grid_y,
                    0.0,
                    MAX_WAVE_SOURCE_GRID,
                    DEFAULT_WAVE_SOURCE_GRID_Y,
                    "Grid Y",
                ),
                local_info(
                    controls.interpolation,
                    0.0,
                    WAVE_SOURCE_INTERPOLATIONS.len() as f32 - 1.0,
                    0.0,
                    "Interpolation",
                ),
            ] {
                infos.insert(info.name.clone(), info);
            }
        }
        for (name, display_name) in PRESET_STYLE_FILTERS
            .iter()
            .chain(&SAVE_STYLE_BUTTONS)
            .zip(PRESET_STYLE_NAMES.iter().cycle())
        {
            let info = local_info(name, 0.0, 1.0, 0.0, display_name);
            infos.insert(info.name.clone(), info);
        }
        infos
    })
}

macro_rules! lfo_names {
    ($suffix:literal) => {
        [
            concat!("lfo_1_", $suffix),
            concat!("lfo_2_", $suffix),
            concat!("lfo_3_", $suffix),
            concat!("lfo_4_", $suffix),
            concat!("lfo_5_", $suffix),
            concat!("lfo_6_", $suffix),
            concat!("lfo_7_", $suffix),
            concat!("lfo_8_", $suffix),
        ]
    };
}

/// Each LFO's grid width, grid height, paint pattern and paint button.
pub const GRID_SIZE_X: [&str; NUM_LFOS] = lfo_names!("grid_size_x");
pub const GRID_SIZE_Y: [&str; NUM_LFOS] = lfo_names!("grid_size_y");
pub const PAINT_PATTERN: [&str; NUM_LFOS] = lfo_names!("paint_pattern");
pub const PAINT: [&str; NUM_LFOS] = lfo_names!("paint");

/// The local control called `name`, if there is one.
pub fn info(name: &str) -> Option<&'static ParamInfo> {
    infos().get(name)
}

/// The local controls' values, one set per editor.
#[derive(Default)]
pub struct LocalValues {
    values: Mutex<HashMap<String, f32>>,
}

impl LocalValues {
    pub fn get(&self, name: &str) -> Option<f32> {
        let info = info(name)?;
        let values = self.values.lock().unwrap_or_else(|p| p.into_inner());
        Some(values.get(name).copied().unwrap_or(info.default_value))
    }

    /// Sets a local control, clamped and rounded to its range. False when
    /// `name` isn't one.
    pub fn set(&self, name: &str, value: f32) -> bool {
        let Some(info) = info(name) else {
            return false;
        };
        let value = value.round().clamp(info.min, info.max);
        let mut values = self.values.lock().unwrap_or_else(|p| p.into_inner());
        values.insert(name.to_string(), value);
        true
    }
}
