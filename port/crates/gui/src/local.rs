// SPDX-License-Identifier: GPL-3.0-or-later

//! Controls the editor keeps itself: upstream's sliders that aren't synth
//! parameters (each LFO section's grid sizes and paint pattern). They
//! behave as sliders through [`crate::host::Host`], which stores their
//! values here instead of in the host, so they aren't saved or automated,
//! as upstream's aren't.

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
