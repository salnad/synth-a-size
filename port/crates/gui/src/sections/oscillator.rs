// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! An oscillator section: pitch, unison and phase, level and pan, the
//! spectral and wave morphs with their type selectors, the frame slider and
//! the destination. Ports `OscillatorSection` and its `UnisonViewer`
//! (`vital/src/interface/editor_sections/oscillator_section.cpp`), and the
//! FM/RM dependency check of `SynthesisInterface::distortionTypeChanged`.
//!
//! The wavetable view is `controls::wavetable_3d`. The wavetable's browser
//! and the edit button's action come later. The text-to-wavetable field is
//! absent (D20: it needs sign-in).

use std::cell::RefCell;

use super::{MenuOwner, Panel};
use crate::colour::Colour;
use crate::controls::button::SynthButton;
use crate::controls::paths::Icon;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::slider::{Placement, Slider, SliderStyle};
use crate::controls::transpose_quantize;
use crate::controls::wavetable_3d::{RenderType, ViewSettings, Wavetable3d};
use crate::fonts::Face;
use crate::host::Host;
use crate::layout::{PresetSelectorLayout, Rect};
use crate::paint::PixelPainter;
use crate::popup::{Menu, MenuItem};
use crate::primitives::Quad;
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_label_connection, label_background_bounds, paint_border, paint_container,
    paint_joint_control, paint_sideways_heading_text, place_joint_controls, place_knobs_in_area,
    slider_overlap, slider_overlap_with_space, slider_width, title_width, Heading,
};
use crate::skin::{ColorId, Section, ValueId};

/// The section's skin override (`setSkinOverride(Skin::kOscillator)`).
pub const SKIN: &[Section] = &[Section::Oscillator];

/// `vital::kNumOscillators`.
pub const NUM_OSCILLATORS: usize = 3;

/// `OscillatorSection::kSectionWidthRatio`.
const SECTION_WIDTH_RATIO: f32 = 0.19;
/// `SynthSection::kTransposeMouseSensitivity`.
const TRANSPOSE_MOUSE_SENSITIVITY: f32 = 0.2;
/// `vital::SynthOscillator::kMaxUnison`.
const MAX_UNISON: usize = 16;

/// `kSpectralMorphTypes`: the text under the spectral morph knob.
pub const SPECTRAL_MORPH_TYPES: [&str; 12] = [
    "---",
    "VOCODE",
    "FORM SCALE",
    "HARM STRETCH",
    "INHARMONIC",
    "SMEAR",
    "RAND AMP",
    "LOW PASS",
    "HIGH PASS",
    "PHASE DISP",
    "SHEPARD TONE",
    "TIME SKEW",
];

/// `strings::kSpectralMorphNames`: the spectral morph menu.
pub const SPECTRAL_MORPH_NAMES: [&str; 12] = [
    "None",
    "Vocode",
    "Formant Scale",
    "Harmonic Stretch",
    "Inharmonic Stretch",
    "Smear",
    "Random Amplitudes",
    "Low Pass",
    "High Pass",
    "Phase Disperse",
    "Shepard Tone",
    "Spectral Time Skew",
];

/// `kDistortionTypes`: the text under the wave morph knob.
pub const DISTORTION_TYPES: [&str; 13] = [
    "---",
    "SYNC",
    "FORMANT",
    "QUANTIZE",
    "BEND",
    "SQUEEZE",
    "PULSE",
    "FM <- OSC",
    "FM <- OSC",
    "FM <- SAMPLE",
    "RM <- OSC",
    "RM <- OSC",
    "RM <- SAMPLE",
];

/// `strings::kPhaseDistortionNames`: the wave morph menu.
pub const DISTORTION_NAMES: [&str; 13] = [
    "None",
    "Sync",
    "Formant",
    "Quantize",
    "Bend",
    "Squeeze",
    "Pulse",
    "FM <- Osc",
    "FM <- Osc",
    "FM <- Sample",
    "RM <- Osc",
    "RM <- Osc",
    "RM <- Sample",
];

/// `strings::kDestinationNames`, for the sources' destinations.
pub const DESTINATION_NAMES: [&str; 5] = [
    "FILTER 1",
    "FILTER 2",
    "FILTER 1+2",
    "EFFECTS",
    "DIRECT OUT",
];

/// `strings::kDestinationMenuNames`, for the sources' destinations.
pub const DESTINATION_MENU_NAMES: [&str; 5] = [
    "Filter 1",
    "Filter 2",
    "Filter 1+2",
    "Effects",
    "Direct Out",
];

/// `strings::kWavetableDimensionNames`, by `osc_N_view_2d`.
pub const DIMENSION_NAMES: [&str; 3] = ["3D", "2D", "SP"];

/// `SynthOscillator::DistortionType`s the section treats specially.
mod distortion {
    pub const NONE: usize = 0;
    pub const SYNC: usize = 1;
    pub const FORMANT: usize = 2;
    pub const QUANTIZE: usize = 3;
    pub const BEND: usize = 4;
    pub const SQUEEZE: usize = 5;
    pub const PULSE_WIDTH: usize = 6;
    pub const FM_OSCILLATOR_A: usize = 7;
    pub const FM_OSCILLATOR_B: usize = 8;
    pub const RM_OSCILLATOR_A: usize = 10;
    pub const RM_OSCILLATOR_B: usize = 11;
    pub const COUNT: usize = 13;
}

/// `SynthOscillator::kNumSpectralMorphTypes`.
const NUM_SPECTRAL_MORPH_TYPES: usize = 12;

/// `SynthOscillator::isFirstModulation`.
fn is_first_modulation(kind: usize) -> bool {
    kind == distortion::FM_OSCILLATOR_A || kind == distortion::RM_OSCILLATOR_A
}

/// `SynthOscillator::isSecondModulation`.
fn is_second_modulation(kind: usize) -> bool {
    kind == distortion::FM_OSCILLATOR_B || kind == distortion::RM_OSCILLATOR_B
}

/// `ProducersModule::getFirstModulationIndex`.
fn first_modulation_index(index: usize) -> usize {
    if index == 0 {
        1
    } else {
        0
    }
}

/// `ProducersModule::getSecondModulationIndex`.
fn second_modulation_index(index: usize) -> usize {
    if index == 1 {
        2
    } else {
        first_modulation_index(index) + 1
    }
}

/// `SynthOscillator::usesDistortionPhase`.
pub fn uses_distortion_phase(kind: usize) -> bool {
    matches!(
        kind,
        distortion::SYNC
            | distortion::FORMANT
            | distortion::QUANTIZE
            | distortion::BEND
            | distortion::SQUEEZE
            | distortion::PULSE_WIDTH
    )
}

/// `isBipolarDistortionType`.
fn is_bipolar_distortion(kind: usize) -> bool {
    matches!(
        kind,
        distortion::NONE
            | distortion::SQUEEZE
            | distortion::SYNC
            | distortion::FORMANT
            | distortion::BEND
    )
}

/// `isBipolarSpectralMorphType`.
fn is_bipolar_spectral_morph(kind: usize) -> bool {
    matches!(kind, 0 | 1 | 2 | 3 | 4 | 9)
}

/// `getDistortionSuffix`: which oscillator an FM/RM type reads, as " N".
fn distortion_suffix(kind: usize, index: usize) -> String {
    if kind == distortion::FM_OSCILLATOR_A || kind == distortion::RM_OSCILLATOR_A {
        return format!(" {}", 1 + first_modulation_index(index));
    }
    if kind == distortion::FM_OSCILLATOR_B || kind == distortion::RM_OSCILLATOR_B {
        return format!(" {}", 1 + second_modulation_index(index));
    }
    String::new()
}

/// `getDistortionString`.
pub fn distortion_text(kind: usize, index: usize) -> String {
    format!(
        "{}{}",
        DISTORTION_TYPES[kind],
        distortion_suffix(kind, index)
    )
}

/// The parameter names of oscillator `index` (0-based).
#[derive(Clone, Copy, Debug)]
pub struct Names {
    pub on: &'static str,
    pub transpose: &'static str,
    pub transpose_quantize: &'static str,
    pub tune: &'static str,
    pub unison_voices: &'static str,
    pub unison_detune: &'static str,
    pub detune_power: &'static str,
    pub phase: &'static str,
    pub random_phase: &'static str,
    pub distortion_phase: &'static str,
    pub level: &'static str,
    pub pan: &'static str,
    pub distortion_type: &'static str,
    pub distortion_amount: &'static str,
    pub spectral_morph_type: &'static str,
    pub spectral_morph_amount: &'static str,
    pub wave_frame: &'static str,
    pub destination: &'static str,
    pub view_2d: &'static str,
}

macro_rules! names {
    ($n:literal) => {
        Names {
            on: concat!("osc_", $n, "_on"),
            transpose: concat!("osc_", $n, "_transpose"),
            transpose_quantize: concat!("osc_", $n, "_transpose_quantize"),
            tune: concat!("osc_", $n, "_tune"),
            unison_voices: concat!("osc_", $n, "_unison_voices"),
            unison_detune: concat!("osc_", $n, "_unison_detune"),
            detune_power: concat!("osc_", $n, "_detune_power"),
            phase: concat!("osc_", $n, "_phase"),
            random_phase: concat!("osc_", $n, "_random_phase"),
            distortion_phase: concat!("osc_", $n, "_distortion_phase"),
            level: concat!("osc_", $n, "_level"),
            pan: concat!("osc_", $n, "_pan"),
            distortion_type: concat!("osc_", $n, "_distortion_type"),
            distortion_amount: concat!("osc_", $n, "_distortion_amount"),
            spectral_morph_type: concat!("osc_", $n, "_spectral_morph_type"),
            spectral_morph_amount: concat!("osc_", $n, "_spectral_morph_amount"),
            wave_frame: concat!("osc_", $n, "_wave_frame"),
            destination: concat!("osc_", $n, "_destination"),
            view_2d: concat!("osc_", $n, "_view_2d"),
        }
    };
}

/// Each oscillator's parameter names.
pub const NAMES: [Names; NUM_OSCILLATORS] = [names!("1"), names!("2"), names!("3")];

/// `SynthesisInterface::distortionTypeChanged`: after oscillator `index`'s
/// wave morph type is set to `chosen`, follow which oscillator each FM/RM
/// type reads. If that comes back round, the last oscillator in the loop
/// stops reading another (`resetOscillatorModulationDistortionType`).
/// `chosen` is passed in because the host applies the set later; reading
/// it back would see the old type.
pub fn resolve_modulation_cycle(host: &Host, index: usize, chosen: usize) {
    let start = index;
    let kind = |i: usize| {
        if i == start {
            chosen
        } else {
            host.value(NAMES[i].distortion_type) as usize
        }
    };
    let mut dependents = [false; NUM_OSCILLATORS];
    let mut index = index;
    let mut last_index = index;
    while !dependents[index] {
        dependents[index] = true;
        last_index = index;
        let kind = kind(index);
        if is_first_modulation(kind) {
            index = first_modulation_index(index);
        } else if is_second_modulation(kind) {
            index = second_modulation_index(index);
        } else {
            return;
        }
    }
    let last = kind(last_index);
    if is_first_modulation(last) || is_second_modulation(last) {
        host.set_once(NAMES[last_index].distortion_type, distortion::NONE as f32);
    }
}

/// The follow-up for each oscillator's wave morph menu.
const RESOLVE_CYCLE: [fn(&Host, f32); NUM_OSCILLATORS] = [
    |host, chosen| resolve_modulation_cycle(host, 0, chosen as usize),
    |host, chosen| resolve_modulation_cycle(host, 1, chosen as usize),
    |host, chosen| resolve_modulation_cycle(host, 2, chosen as usize),
];

/// `OscillatorSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OscillatorLayout {
    pub bounds: Rect,
    pub transpose: Rect,
    pub transpose_quantize: Rect,
    pub tune: Rect,
    pub unison_voices: Rect,
    pub unison_detune: Rect,
    pub detune_power: Rect,
    pub phase: Rect,
    pub random_phase: Rect,
    pub level: Rect,
    pub pan: Rect,
    pub spectral_morph_amount: Rect,
    pub distortion_amount: Rect,
    /// The text under each morph knob, and its selector between the
    /// arrows.
    pub spectral_morph_text: Rect,
    pub spectral_morph_selector: Rect,
    pub prev_spectral: Rect,
    pub next_spectral: Rect,
    pub distortion_text: Rect,
    pub distortion_selector: Rect,
    pub prev_distortion: Rect,
    pub next_distortion: Rect,
    pub wave_frame: Rect,
    /// `wavetable_`, the 3D/2D/spectrum view.
    pub wavetable: Rect,
    pub preset_selector: PresetSelectorLayout,
    pub dimension: Rect,
    pub edit: Rect,
    /// The destination text and its selector.
    pub destination: Rect,
    pub prev_destination: Rect,
    pub next_destination: Rect,
    /// Shown for wave morphs that use a phase.
    pub distortion_phase: Option<Rect>,
    /// `paintJointControl`'s PITCH, UNISON and PHASE areas.
    pub pitch_area: Rect,
    pub unison_area: Rect,
    pub phase_area: Rect,
}

impl OscillatorLayout {
    /// The section at `bounds`, with the distortion phase slider shown or
    /// not (`setDistortionPhaseVisible`).
    pub fn new(scope: &Scope, bounds: Rect, distortion_phase_visible: bool) -> OscillatorLayout {
        let width = bounds.width;
        let height = bounds.height;
        let label_height = scope.value(ValueId::LabelBackgroundHeight) as i32;
        let title_width = title_width(scope) as i32;
        let widget_margin = scope.widget_margin() as i32;
        let text_height = scope.value(ValueId::TextButtonHeight) as i32;
        let slider_width = slider_width(scope);
        let knob_section_height = scope.knob_section_height() as i32;
        let slider_overlap = slider_overlap(scope);
        let overlap_with_space = slider_overlap_with_space(scope);
        let mut wave_section_height = height - 2 * widget_margin;
        if distortion_phase_visible {
            wave_section_height -= slider_width - slider_overlap - overlap_with_space;
        }

        let level_pan_x = title_width;
        let level_pan_width = (width as f32 * SECTION_WIDTH_RATIO) as i32;
        let top_row_width = level_pan_width - 2 * widget_margin;
        let knob_y = height - label_height - widget_margin - knob_section_height;
        let big_knob_height = height - knob_y;
        let level_pan = place_knobs_in_area(
            scope,
            Rect::new(level_pan_x, knob_y, level_pan_width, knob_section_height),
            2,
        );

        let section2_x = width - 2 * top_row_width - 2 * widget_margin;
        let wave_frame_x = section2_x - slider_width + overlap_with_space;
        let wavetable_x = level_pan_width + level_pan_x;
        let wavetable_width = wave_frame_x - wavetable_x + overlap_with_space;
        let wavetable = Rect::new(
            wavetable_x,
            widget_margin,
            wavetable_width,
            wave_section_height,
        );
        let preset_selector = Rect::new(
            wavetable_x,
            widget_margin,
            wavetable_width,
            title_width - 2 * widget_margin,
        );
        let button_y = widget_margin + wave_section_height - text_height;
        let dimension = Rect::new(wavetable_x, button_y, text_height, text_height);
        let wave_frame = Rect::new(
            wave_frame_x,
            0,
            slider_width,
            wave_section_height + 2 * widget_margin,
        );
        let edit = Rect::new(
            wavetable.right() - text_height,
            button_y,
            text_height,
            text_height,
        );

        let top_row_y = widget_margin;
        let text_section_height = knob_y - widget_margin;
        let pitch_area = Rect::new(
            title_width + widget_margin,
            top_row_y,
            level_pan_width - 2 * widget_margin,
            text_section_height,
        );
        let (transpose, tune, transpose_quantize) = place_joint_controls(
            scope,
            pitch_area.x,
            pitch_area.y,
            pitch_area.width,
            pitch_area.height,
        );

        let section2_width = width - section2_x;
        let unison_area = Rect::new(section2_x, top_row_y, top_row_width, text_section_height);
        let (unison_voices, unison_detune, detune_power) = place_joint_controls(
            scope,
            unison_area.x,
            unison_area.y,
            unison_area.width,
            unison_area.height,
        );
        let phase_area = Rect::new(
            section2_x + top_row_width + widget_margin,
            top_row_y,
            top_row_width,
            text_section_height,
        );
        let (phase, random_phase, _) = place_joint_controls(
            scope,
            phase_area.x,
            phase_area.y,
            phase_area.width,
            phase_area.height,
        );

        let morphs = place_knobs_in_area(
            scope,
            Rect::new(
                section2_x - widget_margin,
                knob_y,
                section2_width + widget_margin,
                big_knob_height,
            ),
            2,
        );
        let (spectral_morph_amount, distortion_amount) = (morphs[0], morphs[1]);
        let morph_y = height - knob_section_height + widget_margin;
        let morph_label = |knob: Rect| {
            label_background_bounds(
                scope,
                Rect::new(
                    knob.x,
                    morph_y,
                    knob.width,
                    knob_section_height - 2 * widget_margin,
                ),
                false,
            )
        };
        let spectral_morph_text = morph_label(spectral_morph_amount);
        let browse_width = spectral_morph_text.height;
        let browse_y = spectral_morph_text.y;
        let arrows = |knob: Rect| {
            (
                Rect::new(knob.x, browse_y, browse_width, browse_width),
                Rect::new(
                    knob.right() - browse_width,
                    browse_y,
                    browse_width,
                    browse_width,
                ),
            )
        };
        let (prev_spectral, next_spectral) = arrows(spectral_morph_amount);
        let (prev_distortion, next_distortion) = arrows(distortion_amount);
        let between = |prev: Rect, next: Rect| {
            Rect::new(prev.right(), prev.y, next.x - prev.right(), prev.height)
        };
        let distortion_text = morph_label(distortion_amount);

        let distortion_phase = distortion_phase_visible.then(|| {
            Rect::new(
                wavetable.x - widget_margin,
                wavetable.bottom() - slider_overlap + widget_margin,
                wavetable.width + 2 * widget_margin,
                slider_width,
            )
        });

        let destination_x = level_pan_x + widget_margin;
        let destination_y = height - label_height - widget_margin;
        let destination = Rect::new(destination_x, destination_y, top_row_width, label_height);
        let prev_destination = Rect::new(destination_x, destination_y, browse_width, browse_width);
        let next_destination = Rect::new(
            destination_x + top_row_width - browse_width,
            destination_y,
            browse_width,
            browse_width,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        let at = |r: Rect| r.translated(dx, dy);
        OscillatorLayout {
            bounds,
            transpose: at(transpose),
            transpose_quantize: at(transpose_quantize),
            tune: at(tune),
            unison_voices: at(unison_voices),
            unison_detune: at(unison_detune),
            detune_power: at(detune_power),
            phase: at(phase),
            random_phase: at(random_phase),
            level: at(level_pan[0]),
            pan: at(level_pan[1]),
            spectral_morph_amount: at(spectral_morph_amount),
            distortion_amount: at(distortion_amount),
            spectral_morph_text: at(spectral_morph_text),
            spectral_morph_selector: at(between(prev_spectral, next_spectral)),
            prev_spectral: at(prev_spectral),
            next_spectral: at(next_spectral),
            distortion_text: at(distortion_text),
            distortion_selector: at(between(prev_distortion, next_distortion)),
            prev_distortion: at(prev_distortion),
            next_distortion: at(next_distortion),
            wave_frame: at(wave_frame),
            wavetable: at(wavetable),
            preset_selector: PresetSelectorLayout::new(at(preset_selector)),
            dimension: at(dimension),
            edit: at(edit),
            destination: at(destination),
            prev_destination: at(prev_destination),
            next_destination: at(next_destination),
            distortion_phase: distortion_phase.map(at),
            pitch_area: at(pitch_area),
            unison_area: at(unison_area),
            phase_area: at(phase_area),
        }
    }
}

/// One oscillator section's controls (`OscillatorSection`'s constructor).
pub struct OscillatorSection {
    index: usize,
    /// The wavetable view, which keeps its drawing between frames.
    view: RefCell<Wavetable3d>,
    names: Names,
    on: SynthButton,
    transpose: Slider,
    tune: Slider,
    unison_voices: Slider,
    unison_detune: Slider,
    detune_power: Slider,
    phase: Slider,
    random_phase: Slider,
    distortion_phase: Slider,
    level: Slider,
    pan: Slider,
    distortion_amount: Slider,
    spectral_morph_amount: Slider,
    wave_frame: Slider,
}

impl OscillatorSection {
    /// Oscillator `index` (0-based).
    pub fn new(index: usize) -> OscillatorSection {
        let names = NAMES[index];
        let text = |name| Slider::new(name, SliderStyle::Text).text_entry(1.0, Some(0.0));
        let mut view = Wavetable3d::new(index);
        view.save_available = true;
        OscillatorSection {
            index,
            view: RefCell::new(view),
            names,
            on: SynthButton::power(names.on),
            transpose: text(names.transpose)
                .sensitivity(TRANSPOSE_MOUSE_SENSITIVITY)
                .modulation_bar_right(false),
            tune: text(names.tune).bipolar().display(3, 0),
            unison_voices: text(names.unison_voices).modulation_bar_right(false),
            unison_detune: text(names.unison_detune).display(3, 0),
            // InvisibleSlider: the unison viewer draws in its place.
            detune_power: Slider::new(names.detune_power, SliderStyle::Curve)
                .text_entry(1.0, None)
                .popup_prefix("Unison Detune Power: "),
            phase: text(names.phase).display(3, 0),
            random_phase: text(names.random_phase).display(3, 0),
            distortion_phase: Slider::new(names.distortion_phase, SliderStyle::Horizontal)
                .bipolar()
                .modulation_placement(Placement::Above),
            level: Slider::new(names.level, SliderStyle::Rotary),
            pan: Slider::new(names.pan, SliderStyle::Rotary).bipolar(),
            distortion_amount: Slider::new(names.distortion_amount, SliderStyle::Rotary)
                .knob_size_scale(1.5)
                .popup_prefix("Wave Morph: "),
            spectral_morph_amount: Slider::new(names.spectral_morph_amount, SliderStyle::Rotary)
                .knob_size_scale(1.5)
                .popup_prefix("Spectral Morph: "),
            wave_frame: Slider::new(names.wave_frame, SliderStyle::Vertical)
                .popup_placement(Placement::Right)
                .modulation_placement(Placement::Left)
                .popup_prefix("Frame: "),
        }
    }

    /// "Save to Wavetables" was chosen in the wavetable view's menu since
    /// the last call (`OscillatorSection::saveWavetable`).
    pub fn take_save_request(&self) -> bool {
        self.view.borrow_mut().take_save_request()
    }

    /// The section's layout for the current wave morph type.
    pub fn layout(&self, host: &Host, scope: &Scope, bounds: Rect) -> OscillatorLayout {
        let kind = self.distortion_type(host);
        OscillatorLayout::new(scope, bounds, uses_distortion_phase(kind))
    }

    fn distortion_type(&self, host: &Host) -> usize {
        (host.value(self.names.distortion_type) as usize).min(distortion::COUNT - 1)
    }

    fn spectral_morph_type(&self, host: &Host) -> usize {
        (host.value(self.names.spectral_morph_type) as usize).min(NUM_SPECTRAL_MORPH_TYPES - 1)
    }

    fn destination(&self, host: &Host) -> usize {
        (host.value(self.names.destination) as usize).min(DESTINATION_NAMES.len() - 1)
    }

    /// The morph knobs as `setupSpectralMorph` / `setupDistortion` leave
    /// them for the current types: bipolar for the centred types, back to
    /// the centre or the start on a double click, and inactive with no
    /// type.
    fn morph_knobs(&self, host: &Host, active: bool) -> (Slider, Slider) {
        let spectral = self.spectral_morph_type(host);
        let mut spectral_knob = self.spectral_morph_amount;
        spectral_knob.bipolar = is_bipolar_spectral_morph(spectral);
        spectral_knob.double_click_value = Some(if spectral_knob.bipolar { 0.5 } else { 0.0 });
        spectral_knob.active = active && spectral != 0;

        let kind = self.distortion_type(host);
        let mut distortion_knob = self.distortion_amount;
        distortion_knob.bipolar = is_bipolar_distortion(kind);
        distortion_knob.double_click_value = Some(if distortion_knob.bipolar { 0.5 } else { 0.0 });
        distortion_knob.active = active && kind != distortion::NONE;
        (spectral_knob, distortion_knob)
    }

    /// `OscillatorSection::paintBackground`.
    pub fn paint_background(
        &self,
        pixels: &PixelPainter,
        scope: &Scope,
        layout: &OscillatorLayout,
        host: &Host,
    ) {
        if layout.bounds.width == 0 {
            return;
        }
        let name = format!("OSC {}", self.index + 1);
        paint_container(pixels, scope, layout.bounds, Heading::Sideways);
        paint_sideways_heading_text(pixels, scope, &name, layout.bounds, true);

        // paintChildrenBackgrounds: the wavetable view, then the
        // wavetable name's selector.
        {
            let mut view = self.view.borrow_mut();
            view.set_view_settings(ViewSettings::from_skin(scope));
            let render_type = RenderType::from_value(host.value(self.names.view_2d));
            let wavetable = host.model().wavetable(self.index);
            view.paint_background(
                pixels,
                scope,
                layout.wavetable,
                render_type,
                self.on.is_on(host),
                wavetable.as_ref(),
            );
        }
        let preset = layout.preset_selector;
        pixels.fill_rounded_rect(
            preset.bounds,
            scope.value(ValueId::WidgetRoundedCorner),
            scope.color(ColorId::PopupSelectorBackground),
        );
        paint_border(pixels, scope, layout.bounds);

        draw_label_connection(pixels, scope, layout.level, layout.pan);
        draw_label(pixels, scope, "PAN", layout.pan, false);
        draw_label(pixels, scope, "LEVEL", layout.level, false);

        let rounding = scope.value(ValueId::LabelBackgroundRounding) as i32 as f32;
        let background = scope.color(ColorId::TextComponentBackground);
        pixels.fill_rounded_rect(layout.spectral_morph_text, rounding, background);
        pixels.fill_rounded_rect(layout.distortion_text, rounding, background);
        pixels.fill_rounded_rect(layout.destination, rounding, background);

        // paintKnobShadows.
        for (slider, bounds) in [
            (&self.level, layout.level),
            (&self.pan, layout.pan),
            (&self.spectral_morph_amount, layout.spectral_morph_amount),
            (&self.distortion_amount, layout.distortion_amount),
        ] {
            slider.paint_shadow(pixels, scope, bounds);
        }

        paint_joint_control(pixels, scope, layout.pitch_area, "PITCH");
        paint_joint_control(pixels, scope, layout.unison_area, "UNISON");
        paint_joint_control(pixels, scope, layout.phase_area, "PHASE");
    }

    /// The controls, the type texts and their selectors, and the power
    /// button with the section's off overlay.
    pub fn show(&self, panel: &mut Panel, layout: &OscillatorLayout) {
        self.show_view(panel, layout);
        let host = panel.host;
        let active = self.on.is_on(host);
        let with_active = |slider: &Slider| {
            let mut slider = *slider;
            slider.active = active;
            slider
        };

        for (slider, bounds) in [
            (&self.transpose, layout.transpose),
            (&self.tune, layout.tune),
            (&self.unison_voices, layout.unison_voices),
            (&self.unison_detune, layout.unison_detune),
            (&self.phase, layout.phase),
            (&self.random_phase, layout.random_phase),
            (&self.level, layout.level),
            (&self.pan, layout.pan),
            (&self.wave_frame, layout.wave_frame),
        ] {
            panel.slider(&with_active(slider), bounds);
        }
        if let Some(bounds) = layout.distortion_phase {
            panel.slider(&with_active(&self.distortion_phase), bounds);
        }
        let (spectral_knob, distortion_knob) = self.morph_knobs(host, active);
        panel.slider(&spectral_knob, layout.spectral_morph_amount);
        panel.slider(&distortion_knob, layout.distortion_amount);

        // The detune power slider is invisible; the unison viewer draws its
        // place.
        let mut detune_power = with_active(&self.detune_power);
        detune_power.style = SliderStyle::Curve;
        panel.slider_input(&detune_power, layout.detune_power);
        for quad in self.unison_quads(panel, layout.detune_power) {
            panel.layer().additive_quad(quad);
        }

        let quantize = host.value(self.names.transpose_quantize);
        let bounds = layout.transpose_quantize;
        let colours = transpose_quantize::QuantizeColours::new(&panel.scope);
        panel.layer().custom(move |pixels| {
            transpose_quantize::paint(pixels, colours, bounds, quantize, false)
        });

        self.show_selectors(panel, layout);
        self.show_wavetable_buttons(panel, layout);
        panel.activator(&self.on, layout.bounds);
    }

    /// `UnisonViewer::render` at rest (the slider values, without
    /// modulation): one bar per unison voice, spread by the detune and its
    /// power.
    fn unison_quads(&self, panel: &Panel, bounds: Rect) -> Vec<Quad> {
        let host = panel.host;
        let voices = (host.value(self.names.unison_voices) as i32).clamp(1, MAX_UNISON as i32);
        let colour = panel
            .scope
            .color(ColorId::WidgetPrimary1)
            .with_multiplied_alpha(0.5);
        unison_bars(
            bounds,
            voices,
            0.1 * host.value(self.names.unison_detune),
            host.value(self.names.detune_power),
            colour,
        )
    }

    /// The spectral morph, wave morph and destination texts with their
    /// menus and arrows (`buttonClicked`).
    fn show_selectors(&self, panel: &mut Panel, layout: &OscillatorLayout) {
        let host = panel.host;
        let index = self.index;
        let scope = panel.scope;
        let text_colour = scope.color(ColorId::BodyText);
        let text_size = scope.value(ValueId::LabelHeight);
        let arrows = ShapeColours::icon(&scope);
        let arrow = |shape| ShapeButton {
            shape,
            colours: arrows,
        };
        let invisible = ShapeButton {
            shape: Shape::None,
            colours: arrows,
        };

        // Spectral morph.
        let spectral = self.spectral_morph_type(host);
        panel.text_centred(
            SPECTRAL_MORPH_TYPES[spectral],
            layout.spectral_morph_text,
            Face::Regular,
            text_size,
            text_colour,
        );
        if panel
            .shape_button(
                ("spectral menu", index),
                invisible,
                layout.spectral_morph_selector,
            )
            .pressed
        {
            let mut items = Vec::new();
            for (i, name) in SPECTRAL_MORPH_NAMES.iter().enumerate() {
                if i == 1 {
                    items.push(MenuItem::divider());
                }
                items.push(MenuItem::new(i as i32, *name));
            }
            open_menu(
                panel,
                items,
                layout.spectral_morph_selector,
                self.names.spectral_morph_type,
                None,
            );
        }
        let count = NUM_SPECTRAL_MORPH_TYPES;
        if panel
            .shape_button(
                ("prev spectral", index),
                arrow(Shape::Prev),
                layout.prev_spectral,
            )
            .clicked
        {
            host.set_once(
                self.names.spectral_morph_type,
                ((spectral + count - 1) % count) as f32,
            );
        }
        if panel
            .shape_button(
                ("next spectral", index),
                arrow(Shape::Next),
                layout.next_spectral,
            )
            .clicked
        {
            host.set_once(
                self.names.spectral_morph_type,
                ((spectral + 1) % count) as f32,
            );
        }

        // Wave morph (distortion).
        let kind = self.distortion_type(host);
        panel.text_centred(
            &distortion_text(kind, index),
            layout.distortion_text,
            Face::Regular,
            text_size,
            text_colour,
        );
        if panel
            .shape_button(
                ("distortion menu", index),
                invisible,
                layout.distortion_selector,
            )
            .pressed
        {
            let mut items = Vec::new();
            for (i, name) in DISTORTION_NAMES.iter().enumerate() {
                if i == distortion::NONE + 1 || i == distortion::FM_OSCILLATOR_A {
                    items.push(MenuItem::divider());
                }
                items.push(MenuItem::new(
                    i as i32,
                    format!("{name}{}", distortion_suffix(i, index)),
                ));
            }
            open_menu(
                panel,
                items,
                layout.distortion_selector,
                self.names.distortion_type,
                Some(RESOLVE_CYCLE[index]),
            );
        }
        let count = distortion::COUNT;
        let mut new_kind = None;
        if panel
            .shape_button(
                ("prev distortion", index),
                arrow(Shape::Prev),
                layout.prev_distortion,
            )
            .clicked
        {
            new_kind = Some((kind + count - 1) % count);
        }
        if panel
            .shape_button(
                ("next distortion", index),
                arrow(Shape::Next),
                layout.next_distortion,
            )
            .clicked
        {
            new_kind = Some((kind + 1) % count);
        }
        if let Some(new_kind) = new_kind {
            host.set_once(self.names.distortion_type, new_kind as f32);
            resolve_modulation_cycle(host, index, new_kind);
        }

        // Destination.
        let destination = self.destination(host);
        panel.text_centred(
            DESTINATION_NAMES[destination],
            layout.destination,
            Face::Regular,
            text_size,
            text_colour,
        );
        if panel
            .shape_button(("destination menu", index), invisible, layout.destination)
            .pressed
        {
            let items = DESTINATION_MENU_NAMES
                .iter()
                .enumerate()
                .map(|(i, name)| MenuItem::new(i as i32, *name))
                .collect();
            open_menu(
                panel,
                items,
                layout.destination,
                self.names.destination,
                None,
            );
        }
        let count = DESTINATION_NAMES.len();
        if panel
            .shape_button(
                ("prev destination", index),
                arrow(Shape::Prev),
                layout.prev_destination,
            )
            .clicked
        {
            host.set_once(
                self.names.destination,
                ((destination + count - 1) % count) as f32,
            );
        }
        if panel
            .shape_button(
                ("next destination", index),
                arrow(Shape::Next),
                layout.next_destination,
            )
            .clicked
        {
            host.set_once(self.names.destination, ((destination + 1) % count) as f32);
        }
    }

    /// The wavetable's name strip and arrows, and the dimension button.
    /// The wavetable view (`Wavetable3d`): its lines and its drag, which
    /// moves the frame slider.
    fn show_view(&self, panel: &mut Panel, layout: &OscillatorLayout) {
        // Upstream's default until the editor has the animate setting.
        const ANIMATE: bool = true;
        let model = panel.host.model();
        let wavetable = model.wavetable(self.index);
        let mut view = self.view.borrow_mut();
        let frame = (&self.wave_frame, layout.wave_frame);
        let telemetry = model
            .telemetry()
            .map(|telemetry| telemetry.read(|snapshot| view.read_telemetry(snapshot)));
        view.show(
            panel,
            layout.wavetable,
            frame,
            wavetable.as_ref(),
            telemetry.as_ref(),
            ANIMATE,
        );
    }

    /// The wavetable's name with its arrows; browsing wavetables and the
    /// edit button come with the wavetable view.
    fn show_wavetable_buttons(&self, panel: &mut Panel, layout: &OscillatorLayout) {
        let host = panel.host;
        let index = self.index;
        let ui_colours = ShapeColours::ui(&panel.scope);
        let preset = layout.preset_selector;
        let name = host.model().wavetable_name(index);
        panel.text_centred(
            &name,
            preset.text,
            Face::Title,
            preset.text_size,
            panel.scope.color(ColorId::PresetText),
        );
        panel.shape_button(
            ("wavetable prev", index),
            ShapeButton {
                shape: Shape::Prev,
                colours: ui_colours,
            },
            preset.previous,
        );
        // The edit button opens the wavetable editor (with the wavetable
        // view, G5).
        let pencil = ShapeButton {
            shape: Shape::from(Icon::Pencil),
            colours: ShapeColours::icon(&panel.scope),
        };
        panel.shape_button(("wavetable edit", index), pencil, layout.edit);
        panel.shape_button(
            ("wavetable next", index),
            ShapeButton {
                shape: Shape::Next,
                colours: ui_colours,
            },
            preset.next,
        );

        let count = DIMENSION_NAMES.len();
        let render_type = (host.value(self.names.view_2d) as usize).min(count - 1);
        let dimension = ShapeButton {
            shape: Shape::Text(DIMENSION_NAMES[render_type]),
            colours: ui_colours,
        };
        if panel
            .shape_button(("dimension", index), dimension, layout.dimension)
            .clicked
        {
            host.set_once(
                self.names.view_2d,
                ((render_type + count - 1) % count) as f32,
            );
        }
    }
}

/// Opens a section menu for control `name` below `bounds`
/// (`showPopupSelector(this, Point(x, bottom), ...)`).
fn open_menu(
    panel: &mut Panel,
    items: Vec<MenuItem>,
    bounds: Rect,
    name: &'static str,
    then: Option<fn(&Host, f32)>,
) {
    panel.overlay.menu = Some(Menu::new(
        items,
        egui::pos2(bounds.x as f32, bounds.bottom() as f32),
        MenuOwner::Control { name, then },
    ));
}

/// `UnisonViewer::render`'s bars for `voices` voices spread by `detune`
/// (0.1 × the slider) and `detune_power`, in `bounds` (interface pixels).
pub fn unison_bars(
    bounds: Rect,
    voices: i32,
    detune: f32,
    detune_power: f32,
    colour: Colour,
) -> Vec<Quad> {
    const HEIGHT_RATIO: f32 = 0.7;
    let width = bounds.width as f32;
    let height = bounds.height as f32;
    // BarRenderer::drawBars' width_percent: setBarWidth(1 / width) over
    // kMaxUnison bars, at least 4 / width.
    let bar_width = 1.0 / width;
    let width_percent = (4.0 / width).max(bar_width * 2.0 / MAX_UNISON as f32);
    let percent_active = 1.0 - height / (2.0 * width);
    let offset = -1.0 / width;
    let xs: Vec<f32> = if voices == 1 {
        vec![offset]
    } else {
        (0..voices)
            .map(|i| {
                let t = 2.0 * i as f32 / (voices as f32 - 1.0) - 1.0;
                let mut scale = crate::controls::slider::power_scale(t.abs(), detune_power);
                if t < 0.0 {
                    scale = -scale;
                }
                scale * percent_active * detune + offset
            })
            .collect()
    };
    xs.into_iter()
        .map(|x| {
            Quad::vertical_bar(
                bounds.into(),
                x,
                -HEIGHT_RATIO,
                HEIGHT_RATIO,
                width_percent,
                colour,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulation_indices_match_the_producers_module() {
        assert_eq!(
            (0..3)
                .map(|i| (first_modulation_index(i), second_modulation_index(i)))
                .collect::<Vec<_>>(),
            vec![(1, 2), (0, 2), (0, 1)]
        );
        assert_eq!(
            distortion_text(distortion::FM_OSCILLATOR_A, 0),
            "FM <- OSC 2"
        );
        assert_eq!(
            distortion_text(distortion::RM_OSCILLATOR_B, 2),
            "RM <- OSC 2"
        );
        assert_eq!(distortion_text(distortion::SYNC, 1), "SYNC");
    }
}

#[cfg(test)]
mod unison_tests {
    use super::*;

    /// Upstream sizes the bars in interface pixels (`BarRenderer`'s
    /// `display_scale_` is 1): at 2x, where the viewer is twice as many
    /// pixels, a bar's half length doubles with the height and the
    /// minimum width stays 4 / width.
    #[test]
    fn unison_bars_size_in_interface_pixels_at_2x() {
        let colour = Colour(0xffff_ffff);
        let one = unison_bars(Rect::new(0, 0, 100, 40), 1, 0.0, 0.0, colour);
        let two = unison_bars(Rect::new(0, 0, 200, 80), 1, 0.0, 0.0, colour);
        // size.x = (4 / w) · w / 2; size.y = 0.7 · h / 2.
        assert_eq!(one[0].shader_values[..2], [2.0, 14.0]);
        assert_eq!(two[0].shader_values[..2], [2.0, 28.0]);
        assert_eq!(two[0].rect.height, 2.0 * one[0].rect.height);
        assert_eq!(two[0].rect.width, one[0].rect.width);
    }
}
