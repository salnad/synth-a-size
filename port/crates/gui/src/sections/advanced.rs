// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Advanced tab's settings: voice, oversampling, display and the output
//! displays, under the oscillators' advanced sections. Ports
//! `MasterControlsInterface` with `VoiceSettings`, `OversampleSettings`,
//! `DisplaySettings`, `OutputDisplays` and `TuningSelector`
//! (`vital/src/interface/editor_sections/master_controls_interface.cpp`).

use std::cell::RefCell;

use egui::Sense;
use sas_engine::Tuning;

use super::{MenuOwner, Panel};
use crate::controls::button::SynthButton;
use crate::controls::file_dialog::{self, FileRequest};
use crate::controls::oscilloscope;
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::spectrogram::{Spectrogram, SpectrogramAnalysis};
use crate::fonts::Face;
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::popup::{Menu, MenuItem};
use crate::scope::Scope;
use crate::section::{
    draw_label, draw_text_component_background, paint_border, paint_container, paint_heading_text,
    paint_tab_shadow, title_width, Heading,
};
use crate::sections::oscillator_advanced::{OscillatorAdvancedLayout, OscillatorAdvancedSection};
use crate::sections::synthesis::SynthesisLayout;
use crate::skin::{ColorId, Section, ValueId};

/// The tab's skin override (`setSkinOverride(Skin::kAdvanced)`).
pub const SKIN: &[Section] = &[Section::Advanced];

/// `kTuningNames`: the built-in tunings, then the loaded file's.
pub const TUNING_NAMES: [&str; 5] = [
    "Default",
    "Just - 7 Limit",
    "Just - 5 Limit",
    "Pythagorean",
    "Custom",
];

/// `kFrequencyDisplayNames`.
pub const FREQUENCY_DISPLAY_NAMES: [&str; 2] = ["Semitones", "Hz"];

/// The skin selector's options with no skin files installed: the default,
/// then `"Load Custom Skin..."` (shown as `"Custom"`).
const SKIN_NAMES: [&str; 2] = ["Default", "Load Custom Skin..."];

/// The built-in tunings' scales, for `Just - 7 Limit`, `Just - 5 Limit`
/// and `Pythagorean`: each degree of the octave above the tonic as a
/// ratio. These are the standard scales, written here from their published
/// ratios rather than copied from upstream's files (which load the same
/// tables; see the tests).
pub const TUNING_SCALES: [(&str, [(u32, u32); 12]); 3] = [
    (
        "7-limit just intonation",
        [
            (15, 14),
            (8, 7),
            (6, 5),
            (5, 4),
            (4, 3),
            (7, 5),
            (3, 2),
            (8, 5),
            (5, 3),
            (7, 4),
            (15, 8),
            (2, 1),
        ],
    ),
    (
        "5-limit just intonation",
        [
            (16, 15),
            (9, 8),
            (6, 5),
            (5, 4),
            (4, 3),
            (45, 32),
            (3, 2),
            (8, 5),
            (5, 3),
            (9, 5),
            (15, 8),
            (2, 1),
        ],
    ),
    (
        "Pythagorean tuning",
        [
            (256, 243),
            (9, 8),
            (32, 27),
            (81, 64),
            (4, 3),
            (729, 512),
            (3, 2),
            (128, 81),
            (27, 16),
            (16, 9),
            (243, 128),
            (2, 1),
        ],
    ),
];

/// A built-in scale as the lines of a Scala `.scl` file: its description,
/// the number of degrees, then each degree's ratio.
pub fn scala_lines(scale: &(&str, [(u32, u32); 12])) -> Vec<String> {
    let (description, ratios) = scale;
    let mut lines = vec![description.to_string(), ratios.len().to_string()];
    lines.extend(ratios.iter().map(|(n, d)| format!("{n}/{d}")));
    lines
}

/// `strings::kVoicePriorityNames`.
const VOICE_PRIORITY_NAMES: &[&str] = &["Newest", "Oldest", "Highest", "Lowest", "Round Robin"];
/// `strings::kVoiceOverrideNames`.
const VOICE_OVERRIDE_NAMES: &[&str] = &["Kill", "Steal"];

/// `OversampleSettings`' buttons, for `oversampling` 0 to 3.
pub const OVERSAMPLING_NAMES: [&str; 4] = [
    "1x (Draft)",
    "2x (Recommended)",
    "4x (High CPU)",
    "8x (Ultra CPU)",
];

/// `VoiceSettings::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceSettingsLayout {
    pub bounds: Rect,
    pub title: Rect,
    pub mpe_enabled: Rect,
    pub voice_priority: Rect,
    pub voice_override: Rect,
    pub tuning: Rect,
    pub voice_tune: Rect,
    pub voice_transpose: Rect,
}

impl VoiceSettingsLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> VoiceSettingsLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = title_width(scope) as i32;
        let component_height = (scope.knob_section_height() - widget_margin as f32) as i32;
        let width = bounds.width - 2 * widget_margin;
        let x = widget_margin;
        let y = title + widget_margin;

        let width_left = (width - widget_margin) / 2;
        let voice_y = bounds.height - widget_margin - component_height;
        let voice_tune = Rect::new(x, voice_y, width_left, component_height);
        let x_right = x + width_left + widget_margin;
        let width_right = bounds.width - x_right - widget_margin;
        let voice_transpose = Rect::new(x_right, voice_y, width_right, component_height);

        let mpe_height = scope.value(ValueId::TextButtonHeight) as i32;
        let mpe_enabled = Rect::new(x, voice_y - mpe_height - widget_margin, width, mpe_height);
        let remaining_height = mpe_enabled.y - y;
        let override_y = y + remaining_height / 3;
        let tuning_y = y + (2 * remaining_height) / 3;
        let voice_priority = Rect::new(x, y, width, override_y - y - widget_margin);
        let voice_override = Rect::new(x, override_y, width, tuning_y - override_y - widget_margin);
        let tuning = Rect::new(x, tuning_y, width, mpe_enabled.y - tuning_y - widget_margin);

        let (dx, dy) = (bounds.x, bounds.y);
        VoiceSettingsLayout {
            bounds,
            title: Rect::new(dx, dy, bounds.width, title),
            mpe_enabled: mpe_enabled.translated(dx, dy),
            voice_priority: voice_priority.translated(dx, dy),
            voice_override: voice_override.translated(dx, dy),
            tuning: tuning.translated(dx, dy),
            voice_tune: voice_tune.translated(dx, dy),
            voice_transpose: voice_transpose.translated(dx, dy),
        }
    }
}

/// `OversampleSettings::resized`: its title and the 1x to 8x buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OversampleSettingsLayout {
    pub bounds: Rect,
    pub title: Rect,
    pub buttons: [Rect; 4],
}

impl OversampleSettingsLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> OversampleSettingsLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = title_width(scope) as i32;
        let width = bounds.width - 2 * widget_margin;
        let x = widget_margin;
        let y = title + widget_margin;
        let bottom = bounds.height;
        let height = bottom - y;
        let tops = [
            y,
            y + height / 4,
            y + (2 * height) / 4,
            y + (3 * height) / 4,
            bottom,
        ];
        let buttons = std::array::from_fn(|i| {
            Rect::new(x, tops[i], width, tops[i + 1] - tops[i] - widget_margin)
                .translated(bounds.x, bounds.y)
        });
        OversampleSettingsLayout {
            bounds,
            title: Rect::new(bounds.x, bounds.y, bounds.width, title),
            buttons,
        }
    }
}

/// `DisplaySettings::resized`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplaySettingsLayout {
    pub bounds: Rect,
    pub title: Rect,
    pub frequency_display: Rect,
    pub skin: Rect,
}

impl DisplaySettingsLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> DisplaySettingsLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = title_width(scope) as i32;
        let width = bounds.width - 2 * widget_margin;
        let x = widget_margin;
        let y = title + widget_margin;
        let bottom = bounds.height - widget_margin;
        let frequency_height = (bottom - y - widget_margin) / 2;
        let frequency_display = Rect::new(x, y, width, frequency_height);
        let skin_y = y + frequency_height + widget_margin;
        let skin = Rect::new(x, skin_y, width, bottom - skin_y);
        let (dx, dy) = (bounds.x, bounds.y);
        DisplaySettingsLayout {
            bounds,
            title: Rect::new(dx, dy, bounds.width, title),
            frequency_display: frequency_display.translated(dx, dy),
            skin: skin.translated(dx, dy),
        }
    }
}

/// `OutputDisplays::resized`: the oscilloscope above the spectrogram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputDisplaysLayout {
    pub bounds: Rect,
    pub title: Rect,
    pub oscilloscope: Rect,
    pub spectrogram: Rect,
}

impl OutputDisplaysLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> OutputDisplaysLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = title_width(scope) as i32;
        let x = widget_margin;
        let width = bounds.width - 2 * widget_margin;
        let oscilloscope_y = title + widget_margin;
        let oscilloscope_height = (bounds.height - oscilloscope_y) / 2;
        let oscilloscope = Rect::new(x, oscilloscope_y, width, oscilloscope_height);
        let spectrogram_y = oscilloscope.bottom() + widget_margin;
        let spectrogram_height = bounds.height - spectrogram_y - widget_margin;
        let spectrogram = Rect::new(x, spectrogram_y, width, spectrogram_height);
        let (dx, dy) = (bounds.x, bounds.y);
        OutputDisplaysLayout {
            bounds,
            title: Rect::new(dx, dy, bounds.width, title),
            oscilloscope: oscilloscope.translated(dx, dy),
            spectrogram: spectrogram.translated(dx, dy),
        }
    }
}

/// `MasterControlsInterface::resized`, with the oscillators' advanced
/// sections where the Voice tab's oscillators are (`setOscillatorBounds`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvancedLayout {
    pub bounds: Rect,
    pub oscillators: [Rect; 3],
    pub voice: Rect,
    pub oversample: Rect,
    pub display: Rect,
    pub output: Rect,
}

impl AdvancedLayout {
    /// The tab at `bounds` (`FullLayout::main`); `top` is the interface's
    /// scope.
    pub fn new(top: &Scope, bounds: Rect) -> AdvancedLayout {
        let oscillators = SynthesisLayout::new(top, bounds).oscillators;
        let scope = top.within(SKIN);
        let large_padding = scope.value(ValueId::LargePadding) as i32;
        let padding = scope.padding() as i32;
        let settings_top = oscillators[2].bottom() - bounds.y + large_padding;
        let settings_height = bounds.height - settings_top;
        let panel_width = (bounds.width as f32 * 0.22) as i32;
        let voice = Rect::new(0, settings_top, panel_width, settings_height);
        let oversample_x = voice.right() + padding;
        let display_height = (title_width(&scope)
            + scope.widget_margin()
            + 1.5 * scope.knob_section_height()) as i32;
        let oversample_height = settings_height - display_height - padding;
        let oversample = Rect::new(oversample_x, settings_top, panel_width, oversample_height);
        let display_y = oversample.bottom() + padding;
        let display = Rect::new(oversample_x, display_y, panel_width, display_height);
        let displays_x = display.right() + padding;
        let output = Rect::new(
            displays_x,
            settings_top,
            bounds.width - displays_x,
            settings_height,
        );
        let (dx, dy) = (bounds.x, bounds.y);
        AdvancedLayout {
            bounds,
            oscillators,
            voice: voice.translated(dx, dy),
            oversample: oversample.translated(dx, dy),
            display: display.translated(dx, dy),
            output: output.translated(dx, dy),
        }
    }
}

/// `SynthSection::paintBackground` for a settings panel: the container with
/// its heading across the top, its name, and the border.
fn paint_panel(pixels: &PixelPainter, scope: &Scope, name: &str, bounds: Rect, title: Rect) {
    paint_container(pixels, scope, bounds, Heading::Horizontal);
    paint_heading_text(pixels, scope, name, title);
    paint_border(pixels, scope, bounds);
}

/// The tab's controls.
pub struct AdvancedTab {
    voice_priority: Slider,
    voice_override: Slider,
    voice_tune: Slider,
    voice_transpose: Slider,
    mpe_enabled: SynthButton,
    /// The output spectrogram's running amplitudes.
    spectrogram: RefCell<SpectrogramAnalysis>,
    oscillators: [OscillatorAdvancedSection; 3],
}

impl Default for AdvancedTab {
    fn default() -> AdvancedTab {
        /// `kTransposeMouseSensitivity`.
        const TRANSPOSE_MOUSE_SENSITIVITY: f32 = 0.2;
        AdvancedTab {
            voice_priority: Slider::selector("voice_priority", Some(VOICE_PRIORITY_NAMES)),
            voice_override: Slider::selector("voice_override", Some(VOICE_OVERRIDE_NAMES)),
            voice_tune: Slider::new("voice_tune", SliderStyle::Text),
            voice_transpose: Slider::new("voice_transpose", SliderStyle::Text)
                .sensitivity(TRANSPOSE_MOUSE_SENSITIVITY),
            mpe_enabled: SynthButton::text("mpe_enabled", "MPE ENABLED"),
            spectrogram: RefCell::default(),
            oscillators: std::array::from_fn(OscillatorAdvancedSection::new),
        }
    }
}

impl AdvancedTab {
    /// `MasterControlsInterface`: the oscillators' advanced sections, then
    /// each settings panel's background and controls. `oscilloscope` is the
    /// telemetry's oscilloscope memory (none without an engine).
    pub fn show(
        &self,
        panel: &mut Panel,
        layout: &AdvancedLayout,
        oscilloscope: Option<&[[f32; 2]]>,
    ) {
        let pixels = panel.pixels;
        let scope = panel.scope;
        for (section, bounds) in self.oscillators.iter().zip(layout.oscillators) {
            section.show(panel, &OscillatorAdvancedLayout::new(&scope, bounds));
        }
        for bounds in [
            layout.voice,
            layout.oversample,
            layout.display,
            layout.output,
        ] {
            // paintBackgroundShadow: paintTabShadow.
            paint_tab_shadow(pixels, &scope, bounds);
        }
        self.show_voice(panel, &VoiceSettingsLayout::new(&scope, layout.voice));
        self.show_oversample(
            panel,
            &OversampleSettingsLayout::new(&scope, layout.oversample),
        );
        self.show_display(panel, &DisplaySettingsLayout::new(&scope, layout.display));
        show_output(
            panel,
            &OutputDisplaysLayout::new(&scope, layout.output),
            oscilloscope,
            &mut self.spectrogram.borrow_mut(),
        );
    }

    /// `VoiceSettings`.
    fn show_voice(&self, panel: &mut Panel, l: &VoiceSettingsLayout) {
        let (pixels, scope) = (panel.pixels, panel.scope);
        paint_panel(pixels, &scope, "VOICE", l.bounds, l.title);
        pixels.fill_rounded_rect(
            l.mpe_enabled,
            scope.value(ValueId::LabelBackgroundRounding),
            scope.color(ColorId::TextComponentBackground),
        );
        for rect in [
            l.voice_priority,
            l.voice_override,
            l.tuning,
            l.voice_tune,
            l.voice_transpose,
        ] {
            draw_text_component_background(pixels, &scope, rect, true);
        }
        for (text, rect) in [
            ("NOTE PRIORITY", l.voice_priority),
            ("VOICE OVERRIDE", l.voice_override),
            ("TUNING", l.tuning),
            ("TUNE", l.voice_tune),
            ("TRANSPOSE", l.voice_transpose),
        ] {
            draw_label(pixels, &scope, text, rect, true);
        }

        panel.button(&self.mpe_enabled, l.mpe_enabled);
        panel.slider(&self.voice_priority, l.voice_priority);
        panel.slider(&self.voice_override, l.voice_override);
        show_tuning(panel, l.tuning);
        panel.slider(&self.voice_tune, l.voice_tune);
        panel.slider(&self.voice_transpose, l.voice_transpose);
    }

    /// `OversampleSettings`: one button per amount, the current one on; a
    /// click sets `oversampling` (`valueChangedInternal`).
    fn show_oversample(&self, panel: &mut Panel, l: &OversampleSettingsLayout) {
        let (pixels, scope) = (panel.pixels, panel.scope);
        paint_panel(pixels, &scope, "OVERSAMPLING", l.bounds, l.title);
        let rounding = scope.value(ValueId::LabelBackgroundRounding);
        for rect in l.buttons {
            pixels.fill_rounded_rect(
                rect,
                rounding,
                scope.color(ColorId::TextComponentBackground),
            );
        }
        let current = panel.host.value("oversampling").round() as usize;
        for (index, (name, rect)) in OVERSAMPLING_NAMES.iter().zip(l.buttons).enumerate() {
            if panel.text_toggle(("oversampling", index), name, current == index, rect) {
                panel.host.set_once("oversampling", index as f32);
            }
        }
    }

    /// `DisplaySettings`: the frequency units (an editor setting, not a
    /// parameter) and the skin.
    fn show_display(&self, panel: &mut Panel, l: &DisplaySettingsLayout) {
        let (pixels, scope) = (panel.pixels, panel.scope);
        paint_panel(pixels, &scope, "DISPLAY", l.bounds, l.title);
        draw_text_component_background(pixels, &scope, l.frequency_display, true);
        draw_text_component_background(pixels, &scope, l.skin, true);
        draw_label(pixels, &scope, "FREQUENCY UNITS", l.frequency_display, true);
        draw_label(pixels, &scope, "SKIN", l.skin, true);

        let hz = panel.host.model().hz_frequency_display();
        let units = FREQUENCY_DISPLAY_NAMES[usize::from(hz)];
        if text_selector(panel, "frequency display", units, l.frequency_display) {
            open_list(
                panel,
                &FREQUENCY_DISPLAY_NAMES,
                below(l.frequency_display),
                |host, _, id| host.model().set_hz_frequency_display(id == 1),
            );
        }
        // Only the default skin until skins can be loaded; choosing "Load
        // Custom Skin..." does nothing yet.
        if text_selector(panel, "skin", SKIN_NAMES[0], l.skin) {
            open_list(panel, &SKIN_NAMES, below(l.skin), |_, _, _| {});
        }
    }
}

/// `OutputDisplays`: the output's oscilloscope, and its spectrogram.
fn show_output(
    panel: &mut Panel,
    l: &OutputDisplaysLayout,
    memory: Option<&[[f32; 2]]>,
    analysis: &mut SpectrogramAnalysis,
) {
    let (pixels, scope) = (panel.pixels, panel.scope);
    let body = scope.color(ColorId::Body);
    paint_panel(pixels, &scope, "ANALYSIS", l.bounds, l.title);
    oscilloscope::paint_background(pixels, &scope, l.oscilloscope);
    oscilloscope::paint(pixels, &scope, l.oscilloscope, memory, body);

    // Spectrogram::render: the synth's sample rate, and the output memory.
    let mut spectrogram = Spectrogram::default();
    if let Some(telemetry) = panel.host.model().telemetry() {
        spectrogram.sample_rate = telemetry.read(|snapshot| snapshot.sample_rate);
        let frame = panel.ui.ctx().cumulative_frame_nr();
        analysis.update_once(&spectrogram, telemetry.audio_memory(), frame);
    }
    spectrogram.paint_background(pixels, &scope, l.spectrogram);
    let colours = (
        scope.color(ColorId::WidgetPrimary1),
        scope.color(ColorId::WidgetPrimary2),
    );
    spectrogram.paint(pixels, &scope, l.spectrogram, analysis, colours, body);
}

/// A `TextSelector` that isn't a parameter: `text` as
/// `TextLookAndFeel::drawRotarySlider` draws it. True on the press that
/// opens its options (`mouseDown`).
fn text_selector(panel: &mut Panel, salt: &str, text: &str, bounds: Rect) -> bool {
    let scope = panel.scope;
    let offset = scope.value(ValueId::TextComponentOffset).round();
    let area = crate::paint::RectF::new(
        bounds.x as f32,
        bounds.y as f32 + offset,
        bounds.width as f32,
        bounds.height as f32,
    );
    panel.layer().text_centred(
        text,
        area,
        Face::Light,
        scope.value(ValueId::TextComponentFontSize),
        scope.color(ColorId::TextComponentText),
    );
    let rect = panel.pixels.rect(bounds.into());
    let response = panel
        .ui
        .interact(rect, panel.ui.scope_id().with(salt), Sense::click());
    response.is_pointer_button_down_on() && panel.ui.input(|input| input.pointer.any_pressed())
}

/// Where a `TextSelector` opens its options: below itself.
fn below(bounds: Rect) -> egui::Pos2 {
    egui::pos2(bounds.x as f32, bounds.bottom() as f32)
}

/// Opens `names` as a list at `pos` (interface pixels); choosing one calls
/// `then` with its index.
fn open_list(
    panel: &mut Panel,
    names: &[&'static str],
    pos: egui::Pos2,
    then: fn(&Host, &egui::Context, i32),
) {
    let items = names
        .iter()
        .enumerate()
        .map(|(index, name)| MenuItem::new(index as i32, *name))
        .collect();
    open_menu(panel, items, pos, then);
}

/// Opens `items` at `pos` (interface pixels); choosing one calls `then`
/// with its id.
fn open_menu(
    panel: &mut Panel,
    items: Vec<MenuItem>,
    pos: egui::Pos2,
    then: fn(&Host, &egui::Context, i32),
) {
    panel.overlay.menu = Some(Menu::new(items, pos, MenuOwner::Setting(then)));
}

/// The tuning file dialog's id (`TuningSelector::loadTuningFile`).
fn tuning_dialog() -> egui::Id {
    egui::Id::unique("tuning file")
}

/// `TuningSelector::setTuning`: a built-in tuning, or the file dialog for
/// "Load Tuning File..." (`kNumTunings`).
fn choose_tuning(host: &Host, ctx: &egui::Context, index: i32) {
    if index == TUNING_NAMES.len() as i32 - 1 {
        open_tuning_file(ctx);
    } else {
        load_tuning(host, index);
    }
}

/// `TuningSelector::loadTuningFile`: the dialog for a tuning file, which
/// [`poll_tuning_file`] then loads.
pub fn open_tuning_file(ctx: &egui::Context) {
    let filters = vec![(
        "Tuning".to_string(),
        file_dialog::extensions(&Tuning::all_file_extensions()),
    )];
    file_dialog::open(
        ctx,
        tuning_dialog(),
        &FileRequest::open("Load Tuning", filters),
    );
}

/// `loadTuningFile(file)`: a file chosen in the tuning dialog changes the
/// synth's tuning as `SynthBase::loadTuningFile` does (a keyboard map keeps
/// the scale). The interface calls this each frame, whichever tab shows.
pub fn poll_tuning_file(host: &Host, ctx: &egui::Context) {
    if let Some(Some(path)) = file_dialog::take_result(ctx, tuning_dialog()) {
        let model = host.model();
        let mut tuning = model.tuning();
        if tuning.load_file(&path).is_ok() {
            model.load_tuning(tuning);
        }
    }
}

/// `TuningSelector::loadTuning`: a built-in tuning, named after itself.
pub fn load_tuning(host: &Host, index: i32) {
    let model = host.model();
    let mut tuning: Tuning = model.tuning();
    let Some(&name) = TUNING_NAMES.get(index as usize) else {
        return;
    };
    tuning.set_name(name);
    match index {
        1..=3 => tuning.load_scala_lines(&scala_lines(&TUNING_SCALES[index as usize - 1])),
        _ => tuning.set_default_tuning(),
    }
    model.load_tuning(tuning);
}

/// `TuningSelector`: the tuning's name; a press lists the built-in tunings
/// and "Load Tuning File..." at the pointer (`mouseDown`).
fn show_tuning(panel: &mut Panel, bounds: Rect) {
    let name = panel.host.model().tuning_name();
    let name = if name.is_empty() {
        "Default".to_string()
    } else {
        name
    };
    if text_selector(panel, "tuning", &name, bounds) {
        let area = panel.pixels.area();
        let ppp = panel.pixels.pixels_per_point();
        let pos = panel
            .ui
            .input(|input| input.pointer.interact_pos())
            .map(|pos| egui::pos2((pos.x - area.min.x) * ppp, (pos.y - area.min.y) * ppp))
            .unwrap_or_else(|| below(bounds));
        let mut items: Vec<MenuItem> = TUNING_NAMES[..4]
            .iter()
            .enumerate()
            .map(|(index, name)| MenuItem::new(index as i32, *name))
            .collect();
        items.push(MenuItem::divider());
        items.push(MenuItem::new(4, "Load Tuning File..."));
        open_menu(panel, items, pos, choose_tuning);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    /// The built-in scales load the same tuning as upstream's files of the
    /// same name, note by note.
    #[test]
    fn the_built_in_scales_match_upstreams_tunings() {
        let upstream = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../vital/tunings");
        for (scale, file) in
            TUNING_SCALES
                .iter()
                .zip(["7 Limit.scl", "5 Limit.scl", "Pythagorean.scl"])
        {
            let mut ours = Tuning::new();
            ours.load_scala_lines(&scala_lines(scale));
            let theirs = Tuning::from_file(&upstream.join(file)).unwrap();
            for note in 0..128 {
                assert_eq!(
                    ours.convert_midi_note(note),
                    theirs.convert_midi_note(note),
                    "{file}: note {note}"
                );
            }
            assert!(!ours.is_default());
        }
    }
}
