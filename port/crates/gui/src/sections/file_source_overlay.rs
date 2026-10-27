// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor's Audio File Source overlay
//! (`vital/src/interface/wavetable/overlays/file_source_overlay.cpp`): shown
//! while an Audio File Source keyframe is selected, it covers none of the
//! editor's displays and lays its controls along the edit strip: the file
//! viewer (`AudioFileViewer`), the keyframe's start position and the
//! source's window size as text, the keyframe's window fade, the blend and
//! phase style selectors, and the normalize toggle.
//!
//! Audio loads only by dropping a file on the viewer
//! (`AudioFileDropSource`): upstream creates a LOAD button but never gives
//! it bounds, so it never shows. The viewer never sets its sample rate
//! (`sample_rate_` stays 0), so the audio loads at 0 Hz, as upstream's does.
//! A file that can't be read, or audio too short to detect its pitch,
//! loads nothing (D33).

use std::path::PathBuf;

use egui::{PointerButton, Sense};
use sas_engine::sas_oscillators::sas_wavetable::creator::component::FileSource;
use sas_engine::sas_oscillators::sas_wavetable::wave_frame::WAVEFORM_SIZE;
use sas_engine::Tuning;
use sas_simd::common::MIDI_SIZE;
use sas_simd::utils::midi_note_to_frequency;

use super::wave_source_overlay::{paint_controls_background, FrameEdit, OverlayLayout};
use super::wavetable_edit::{to_pixels, KeyframeRef, WavetableEditLayout};
use super::Panel;
use crate::controls::button::just_text_toggle;
use crate::controls::format::juce_double_value;
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::text_editor::{self, TextStyle};
use crate::host::Host;
use crate::layout::Rect;
use crate::local::{FileSourceControls, FILE_SOURCE_FADE_STYLES, FILE_SOURCE_PHASE_STYLES};
use crate::paint::RectF;
use crate::primitives::line::LineRenderer;
use crate::primitives::{Quad, QuadKind};
use crate::scope::Scope;
use crate::skin::{ColorId, ValueId};

/// `WavetableComponentOverlay::kTitleHeightRatio`.
const TITLE_HEIGHT_RATIO: f32 = 0.4;
/// `setEditBounds`' width-to-height ratios.
const TEXT_BOX_WIDTH_HEIGHT_RATIO: f32 = 2.5;
const WINDOW_FADE_WIDTH_HEIGHT_RATIO: f32 = 3.0;
const SELECTOR_WIDTH_HEIGHT_RATIO: f32 = 3.0;
const NORMALIZE_WIDTH_HEIGHT_RATIO: f32 = 2.5;
const BORDER_WIDTH_HEIGHT_RATIO: f32 = 1.5;
/// `setTextHeightPercentage(0.4f)` on the style selectors.
const SELECTOR_TEXT_HEIGHT: f32 = 0.4;
/// `AudioFileViewer::kResolution`.
const RESOLUTION: usize = 256;
/// `AudioFileViewer::resized`' `kBuffer` and `kCenterAlpha`.
const VIEWER_BUFFER: f32 = 0.1;
const CENTER_ALPHA: f32 = 0.1;
/// `AudioFileViewer::audioFileLoaded`'s `kMaxFileSamples`.
pub const MAX_FILE_SAMPLES: usize = 176_400;
/// `windowTextToSize`'s `kMaxWindowSize`.
const MAX_WINDOW_SIZE: f32 = 9999.9;

/// The controls background's titles (`addTitle`).
const TITLES: [&str; 7] = [
    "",
    "POSITION",
    "WINDOW SIZE",
    "WINDOW FADE",
    "BLEND STYLE",
    "PHASE STYLE",
    "",
];

/// `FileSourceOverlay::setEditBounds`, in interface coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FileSourceLayout {
    /// `controls_background_`, `setControlsWidth` wide, centred in the strip.
    pub controls: Rect,
    /// The background's divider lines, as x in it (`addLine`).
    pub lines: [i32; 6],
    pub titles: [Rect; 7],
    /// `audio_thumbnail_`, the file viewer.
    pub viewer: Rect,
    pub start_position: Rect,
    pub window_size: Rect,
    pub window_fade: Rect,
    pub fade_style: Rect,
    pub phase_style: Rect,
    pub normalize: Rect,
}

impl FileSourceLayout {
    /// The overlay in the section laid out as `edit`; `scope` is the
    /// section's.
    pub fn new(scope: &Scope, edit: &WavetableEditLayout) -> FileSourceLayout {
        let strip = OverlayLayout::wave_source(scope, edit).edit;
        // The overlay's padding is an int (`setPadding(getPadding())`).
        let padding = scope.padding() as i32;
        let height = strip.height;
        let ratio = |ratio: f32| (height as f32 * ratio) as i32;
        let text_box_width = ratio(TEXT_BOX_WIDTH_HEIGHT_RATIO);
        let window_fade_width = ratio(WINDOW_FADE_WIDTH_HEIGHT_RATIO);
        let selector_width = ratio(SELECTOR_WIDTH_HEIGHT_RATIO);
        let normalize_width = ratio(NORMALIZE_WIDTH_HEIGHT_RATIO);
        let border_width = ratio(BORDER_WIDTH_HEIGHT_RATIO);
        let total_width = strip.width - 2 * border_width;
        let audio_width = total_width
            - normalize_width
            - 2 * selector_width
            - window_fade_width
            - 2 * text_box_width
            - 6 * padding;

        let controls = Rect::new(
            strip.x + (strip.width - total_width) / 2,
            strip.y,
            total_width,
            height,
        );
        let title_height = (TITLE_HEIGHT_RATIO * height as f32) as i32;
        let x = strip.x + border_width;
        let y = strip.y;
        let y_title = y + title_height;
        let height_title = height - title_height;
        let viewer = Rect::new(x + 1, y + 1, audio_width - 1, height - 2);
        let start_position = Rect::new(
            viewer.right() + padding,
            y_title,
            text_box_width,
            height_title - 1,
        );
        let window_size = Rect::new(
            start_position.right() + padding,
            y_title,
            text_box_width,
            height_title - 1,
        );
        let window_fade = Rect::new(
            window_size.right() + padding,
            y_title,
            window_fade_width,
            height_title,
        );
        let fade_style = Rect::new(
            window_fade.right() + padding,
            y_title,
            selector_width,
            height_title,
        );
        let phase_style = Rect::new(
            fade_style.right() + padding,
            y_title,
            selector_width,
            height_title,
        );
        let normalize_padding = height / 6;
        let normalize = Rect::new(
            phase_style.right() + padding,
            y + normalize_padding,
            normalize_width,
            height - 2 * normalize_padding,
        );

        let lines = [
            audio_width,
            audio_width + text_box_width + padding,
            audio_width + 2 * text_box_width + 2 * padding,
            audio_width + 2 * text_box_width + window_fade_width + 3 * padding,
            audio_width + 2 * text_box_width + window_fade_width + selector_width + 4 * padding,
            audio_width + 2 * text_box_width + window_fade_width + 2 * selector_width + 5 * padding,
        ];
        let mut titles = [Rect::new(0, 0, 0, 0); 7];
        for (i, title) in titles.iter_mut().enumerate() {
            let start = if i > 0 { lines[i - 1] } else { 0 };
            let end = if i < lines.len() {
                lines[i]
            } else {
                total_width
            };
            *title = Rect::new(controls.x + start, controls.y, end - start, title_height);
        }

        FileSourceLayout {
            controls,
            lines,
            titles,
            viewer,
            start_position,
            window_size,
            window_fade,
            fade_style,
            phase_style,
            normalize,
        }
    }
}

/// `windowTextToSize`: a note name is its period at `sample_rate`; anything
/// else is read as a number of samples, from 1 to 9999.9.
pub fn window_text_to_size(text: &str, sample_rate: i32) -> f32 {
    let note = Tuning::note_to_midi_key(text);
    if !(0..MIDI_SIZE).contains(&note) {
        return (juce_double_value(text.trim()) as f32).clamp(1.0, MAX_WINDOW_SIZE);
    }
    sample_rate as f32 / midi_note_to_frequency(note as f32)
}

/// `positionTextToSize`.
fn position_text_to_size(text: &str) -> f32 {
    juce_double_value(text.trim()) as f32
}

/// JUCE's `String(value, 1)`.
fn one_decimal(value: f64) -> String {
    format!("{value:.1}")
}

/// One Audio File Source overlay's sliders.
pub struct FileSourceSliders {
    pub window_fade: Slider,
    pub fade_style: Slider,
    pub phase_style: Slider,
}

impl FileSourceSliders {
    pub fn new(controls: FileSourceControls) -> FileSourceSliders {
        // A plain `SynthSlider` named outside the parameters: its text is
        // JUCE's, seven decimals for a range with no interval.
        let mut window_fade = Slider::new(controls.window_fade, SliderStyle::Text);
        window_fade.format.unassigned_decimal_places = Some(7);
        window_fade.double_click_value = Some(1.0);
        let selector = |name, lookup: &'static [&'static str]| {
            let mut slider = Slider::selector(name, Some(lookup));
            slider.format.string_lookup = Some(lookup);
            slider.text_height_percentage = SELECTOR_TEXT_HEIGHT;
            slider
        };
        FileSourceSliders {
            window_fade,
            fade_style: selector(controls.fade_style, &FILE_SOURCE_FADE_STYLES),
            phase_style: selector(controls.phase_style, &FILE_SOURCE_PHASE_STYLES),
        }
    }
}

/// The text fields' salts.
const START_POSITION: &str = "file source start position";
const WINDOW_SIZE: &str = "file source window size";

/// One Audio File Source overlay between frames.
pub struct FileSourceOverlay {
    pub sliders: FileSourceSliders,
    /// The sliders' values as last applied (window fade, fade style, phase
    /// style).
    seen: [f32; 3],
    /// The window fade changed during a press, which commits on release
    /// (`sliderDragEnded`).
    fade_changed: bool,
    /// The selection and keyframe the controls were loaded for.
    loaded: Option<(u64, KeyframeRef)>,
    /// `start_position_` and `window_size_`'s text.
    pub start_text: String,
    pub window_text: String,
    /// The viewer's `window_position_` and `window_fade_` (its window size
    /// is read from the source).
    window_position: f32,
    window_fade: f32,
    /// `normalize_gain_`'s toggle state.
    normalize: bool,
    /// The viewer's press and `last_mouse_position_`'s x.
    viewer_down: bool,
    last_mouse_x: f32,
    /// A file the viewer would take is dragged over it (`dragging_quad_`).
    file_hovering: bool,
}

impl FileSourceOverlay {
    pub fn new(controls: FileSourceControls) -> FileSourceOverlay {
        FileSourceOverlay {
            sliders: FileSourceSliders::new(controls),
            seen: [0.0; 3],
            fade_changed: false,
            loaded: None,
            start_text: String::new(),
            window_text: String::new(),
            window_position: 0.0,
            window_fade: 0.0,
            normalize: false,
            viewer_down: false,
            last_mouse_x: 0.0,
            file_hovering: false,
        }
    }

    /// Whether an edit is waiting for its commit: a press on the viewer, or
    /// a window fade change until its drag ends (`sliderDragEnded`). The
    /// section keeps its copy of the creator until then, so a change
    /// reaching the model mid-drag doesn't drop the drag's.
    pub fn pressed(&self) -> bool {
        self.viewer_down || self.fade_changed
    }

    /// `frameSelected`: the controls load from keyframe `keyframe` of
    /// `source`.
    fn frame_selected(&mut self, host: &Host, source: &FileSource, keyframe: usize) {
        let Some(frame) = source.keyframes.keyframes.get(keyframe) else {
            return;
        };
        let start_position = frame.data.start_position;
        let window_fade = frame.data.window_fade;
        self.start_text = one_decimal(start_position);
        self.window_text = one_decimal(source.window_size);
        let num_samples = source.sample_buffer.size;
        self.window_position = if num_samples > 0 {
            (start_position / f64::from(num_samples)) as f32
        } else {
            0.0
        };
        self.window_fade = window_fade as f32;
        self.normalize = source.normalize_gain;
        // dontSendNotification: these aren't changes.
        let sliders = &self.sliders;
        host.set(sliders.window_fade.name, window_fade as f32);
        host.set(sliders.fade_style.name, source.fade_style as f32);
        host.set(sliders.phase_style.name, source.phase_style as f32);
        self.seen = [
            host.value(sliders.window_fade.name),
            host.value(sliders.fade_style.name),
            host.value(sliders.phase_style.name),
        ];
        self.fade_changed = false;
    }

    /// The overlay's input for keyframe `keyframe` of `source`, selected as
    /// selection `selection`: the controls load when the selection changed,
    /// then the sliders' changes, the text fields, the viewer's drag, a
    /// dropped file and the normalize toggle edit the source.
    /// `notifyChanged` becomes the returned edit: an audition while a
    /// change is under way, a commit when it's done.
    pub fn input(
        &mut self,
        panel: &mut Panel,
        layout: &FileSourceLayout,
        selection: u64,
        keyframe: KeyframeRef,
        source: &mut FileSource,
    ) -> Option<FrameEdit> {
        if self.loaded != Some((selection, keyframe)) {
            self.loaded = Some((selection, keyframe));
            self.frame_selected(panel.host, source, keyframe.keyframe);
        }
        let index = keyframe.keyframe;
        let mut result = None;
        let mut note = |edit: Option<FrameEdit>| {
            if let Some(edit) = edit {
                result = Some(match result {
                    Some(FrameEdit::Done) => FrameEdit::Done,
                    _ => edit,
                });
            }
        };
        note(self.slider_changes(panel, source, index));
        note(self.text_fields(panel, layout, source, index));
        note(self.viewer_mouse(panel, layout, source, index));
        note(self.file_drop(panel, layout, source, index));
        let id = panel.ui.scope_id().with("file source normalize");
        if just_text_toggle(
            panel.ui,
            panel.pixels,
            &mut panel.layer,
            &panel.scope,
            id,
            "NORMALIZE",
            self.normalize,
            layout.normalize,
        ) {
            // buttonClicked: the toggle flips, and the source takes it.
            self.normalize = !self.normalize;
            source.normalize_gain = self.normalize;
            note(Some(FrameEdit::Done));
        }
        result
    }

    /// `sliderValueChanged` and `sliderDragEnded`.
    fn slider_changes(
        &mut self,
        panel: &Panel,
        source: &mut FileSource,
        keyframe: usize,
    ) -> Option<FrameEdit> {
        let host = panel.host;
        let values = [
            host.value(self.sliders.window_fade.name),
            host.value(self.sliders.fade_style.name),
            host.value(self.sliders.phase_style.name),
        ];
        let mut result = None;
        if values[0] != self.seen[0] {
            if let Some(frame) = source.keyframes.keyframes.get_mut(keyframe) {
                frame.data.window_fade = f64::from(values[0]);
            }
            self.window_fade = values[0];
            self.fade_changed = true;
            result = Some(FrameEdit::Changed);
        }
        if values[1] != self.seen[1] {
            source.fade_style = values[1] as i32;
            result = Some(FrameEdit::Done);
        }
        if values[2] != self.seen[2] {
            source.set_phase_style(values[2] as i32);
            result = Some(FrameEdit::Done);
        }
        self.seen = values;
        let held = panel
            .ui
            .input(|input| input.pointer.button_down(PointerButton::Primary));
        if self.fade_changed && !held {
            self.fade_changed = false;
            result = Some(FrameEdit::Done);
        }
        result
    }

    /// `textEditorReturnKeyPressed` and `textEditorFocusLost`.
    fn text_fields(
        &mut self,
        panel: &mut Panel,
        layout: &FileSourceLayout,
        source: &mut FileSource,
        keyframe: usize,
    ) -> Option<FrameEdit> {
        let mut result = None;
        for (salt, name, bounds) in [
            (START_POSITION, "Start Position", layout.start_position),
            (WINDOW_SIZE, "Window Size", layout.window_size),
        ] {
            // setTextEditorVisuals' font gives way to OpenGlTextEditor's own
            // on resize: the light face at half the field's height.
            let style = TextStyle {
                font_size: bounds.height as f32 / 2.0,
                colour: ColorId::BodyText,
                centred: true,
                hide_prompt: true,
                mono: false,
            };
            let text = if salt == START_POSITION {
                &mut self.start_text
            } else {
                &mut self.window_text
            };
            let events = text_editor::single_line_styled(
                panel.ui,
                panel.pixels,
                &panel.scope,
                salt,
                bounds,
                text,
                name,
                style,
            );
            if !(events.return_pressed || events.lost_focus) {
                continue;
            }
            let edit = if salt == START_POSITION {
                self.load_starting_position_text(source, keyframe)
            } else {
                self.load_window_size_text(source)
            };
            if edit.is_some() {
                text_editor::surrender_focus(panel.ui, salt);
                result = edit;
            }
        }
        result
    }

    /// `clampStartingPosition`: the start position's text, within the audio
    /// less a window.
    fn clamp_starting_position(&mut self, source: &FileSource) {
        let max_position = source.sample_buffer.size as f32 - source.window_size as f32;
        let position = position_text_to_size(&self.start_text);
        let position = position.min(max_position).max(0.0);
        self.start_text = one_decimal(f64::from(position));
    }

    /// `loadStartingPositionText`.
    fn load_starting_position_text(
        &mut self,
        source: &mut FileSource,
        keyframe: usize,
    ) -> Option<FrameEdit> {
        source.keyframes.keyframes.get(keyframe)?;
        self.clamp_starting_position(source);
        let position = position_text_to_size(&self.start_text);
        if position < 0.0 {
            return None;
        }
        let num_samples = source.sample_buffer.size;
        if num_samples > 0 {
            source.keyframes.keyframes[keyframe].data.start_position = f64::from(position);
            self.window_position = position / num_samples as f32;
        }
        Some(FrameEdit::Done)
    }

    /// `loadWindowSizeText`.
    fn load_window_size_text(&mut self, source: &mut FileSource) -> Option<FrameEdit> {
        let window_size = window_text_to_size(&self.window_text, source.sample_buffer.sample_rate);
        if window_size <= 0.0 {
            return None;
        }
        source.set_window_size(f64::from(window_size));
        self.window_text = one_decimal(f64::from(window_size));
        Some(FrameEdit::Done)
    }

    /// `positionMovedRelative`: a drag on the viewer moves the start
    /// position by its share of the audio less a window.
    fn position_moved_relative(
        &mut self,
        source: &mut FileSource,
        keyframe: usize,
        ratio: f32,
        mouse_up: bool,
    ) -> FrameEdit {
        let num_samples = source.sample_buffer.size;
        let max_position = num_samples as f32 - source.window_size as f32;
        let position = position_text_to_size(&self.start_text) + ratio * max_position;
        let position = position.min(max_position).max(0.0);
        self.start_text = one_decimal(f64::from(position));
        if num_samples > 0 {
            if let Some(frame) = source.keyframes.keyframes.get_mut(keyframe) {
                frame.data.start_position = f64::from(position);
                self.window_position = position / num_samples as f32;
            }
        }
        if mouse_up {
            FrameEdit::Done
        } else {
            FrameEdit::Changed
        }
    }

    /// `AudioFileViewer::mouseDown`, `mouseDrag` and `mouseUp`.
    fn viewer_mouse(
        &mut self,
        panel: &mut Panel,
        layout: &FileSourceLayout,
        source: &mut FileSource,
        keyframe: usize,
    ) -> Option<FrameEdit> {
        let bounds = layout.viewer;
        let id = panel.ui.scope_id().with("file source viewer");
        let rect = panel.pixels.rect(bounds.into());
        let response = panel.ui.interact(rect, id, Sense::click_and_drag());
        let (held, pointer) = panel.ui.input(|input| {
            (
                input.pointer.button_down(PointerButton::Primary),
                input.pointer.interact_pos(),
            )
        });
        let x = pointer.map(|pos| to_pixels(panel.pixels, pos).0);
        if !self.viewer_down {
            if response.is_pointer_button_down_on() && held {
                self.viewer_down = true;
                self.last_mouse_x = x.unwrap_or(self.last_mouse_x);
            }
            return None;
        }
        let x = x.unwrap_or(self.last_mouse_x);
        let ratio = (x - self.last_mouse_x) / bounds.width as f32;
        if held {
            if x == self.last_mouse_x {
                return None;
            }
            self.last_mouse_x = x;
            return Some(self.position_moved_relative(source, keyframe, ratio, false));
        }
        self.viewer_down = false;
        self.last_mouse_x = x;
        Some(self.position_moved_relative(source, keyframe, ratio, true))
    }

    /// `AudioFileDropSource` on the viewer: a file it takes highlights it
    /// while dragged over (`fileDragEnter`, `fileDragExit`) and loads when
    /// dropped (`filesDropped`, `loadFile`).
    fn file_drop(
        &mut self,
        panel: &mut Panel,
        layout: &FileSourceLayout,
        source: &mut FileSource,
        keyframe: usize,
    ) -> Option<FrameEdit> {
        use nice_plug_egui::file_drop::{self, DropTarget};

        let model = panel.host.model();
        let extensions = model.audio_file_extensions();
        self.file_hovering = false;
        if extensions.is_empty() {
            return None;
        }
        let rect = panel.pixels.rect(layout.viewer.into());
        file_drop::add_drop_target(panel.ui.ctx(), DropTarget { rect, extensions });
        let (hovered, dropped, pointer) = panel.ui.input(|input| {
            let hovered: Vec<PathBuf> = input
                .raw
                .hovered_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect();
            let dropped: Vec<PathBuf> = input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_owned())
                .collect();
            (hovered, dropped, input.pointer.latest_pos())
        });
        let target = DropTarget { rect, extensions };
        if pointer.is_some_and(|pos| target.takes(pos, &dropped)) {
            let samples = model.read_audio_file(&dropped[0], MAX_FILE_SAMPLES);
            return self.load_file(samples, source, keyframe);
        }
        self.file_hovering = pointer.is_some_and(|pos| target.takes(pos, &hovered));
        None
    }

    /// `loadFile`: the audio, read as `samples`, loads into the
    /// source at the viewer's sample rate (0), its window becomes the
    /// detected period, the start position is clamped and applied, and the
    /// edit commits. Unreadable or too-short audio loads nothing (D33).
    pub fn load_file(
        &mut self,
        samples: Option<Vec<f32>>,
        source: &mut FileSource,
        keyframe: usize,
    ) -> Option<FrameEdit> {
        let samples = samples?;
        if samples.len() < FileSource::PITCH_DETECT_MAX_PERIOD as usize {
            return None;
        }
        source.load_buffer(&samples, 0);
        source.detect_pitch(WAVEFORM_SIZE as i32).ok()?;
        self.clamp_starting_position(source);
        let _ = self.load_starting_position_text(source, keyframe);
        Some(FrameEdit::Done)
    }

    /// The overlay drawn once: the controls background and its titles, the
    /// viewer, the text fields' backgrounds and the sliders. Upstream draws
    /// it twice, as a sub-section and again as the current overlay, so the
    /// section calls this before and after its displays.
    pub fn paint(
        &self,
        panel: &mut Panel,
        layout: &FileSourceLayout,
        source: Option<&FileSource>,
        handle_sliders: bool,
    ) {
        let titles: Vec<(&str, Rect)> = TITLES.iter().copied().zip(layout.titles).collect();
        paint_controls_background(panel, layout.controls, &layout.lines, &titles);
        self.paint_viewer(panel, layout.viewer, source);
        for bounds in [layout.start_position, layout.window_size] {
            paint_text_editor_background(panel, bounds);
        }
        let sliders = &self.sliders;
        for (slider, bounds) in [
            (&sliders.window_fade, layout.window_fade),
            (&sliders.fade_style, layout.fade_style),
            (&sliders.phase_style, layout.phase_style),
        ] {
            if handle_sliders {
                panel.slider(slider, bounds);
            } else {
                let value = slider.host_value(panel.host);
                slider.paint(
                    panel.pixels,
                    &mut panel.layer,
                    &panel.scope,
                    bounds,
                    value,
                    false,
                );
            }
        }
    }

    /// `AudioFileViewer`: the audio's peaks mirrored about the middle
    /// (`setAudioPositions`), brightened inside the window
    /// (`setWindowValues`), and the drop highlight (`dragging_quad_`).
    fn paint_viewer(&self, panel: &mut Panel, bounds: Rect, source: Option<&FileSource>) {
        let scope = &panel.scope;
        let buffer = (bounds.height as f32 * VIEWER_BUFFER) as i32;
        let lines = Rect::new(
            bounds.x,
            bounds.y + buffer,
            bounds.width,
            bounds.height - 2 * buffer,
        );
        let (top, bottom) = viewer_lines(
            source,
            bounds.width as f32,
            lines.height as f32,
            self.window_position,
            self.window_fade,
        );
        let line_color = scope.color(ColorId::WidgetPrimary1);
        // withAlpha(kCenterAlpha): 0.1 of 255, rounded.
        let fill = scope
            .color(ColorId::WidgetSecondary1)
            .with_alpha((CENTER_ALPHA * 255.0).round_ties_even() as u8);
        let boost = scope.value(ValueId::WidgetLineBoost);
        let mut renderers = [top, bottom];
        for line in &mut renderers {
            line.fill = true;
            line.line_width = 3.0;
            line.color = line_color;
            line.set_fill_colors(fill, fill);
            line.boost_amount = boost;
            line.fill_boost_amount = 1.0 / CENTER_ALPHA;
        }
        let area: RectF = lines.into();
        panel.layer.custom(move |pixels| {
            for line in &renderers {
                line.paint(pixels, area, true);
            }
        });
        if self.file_hovering {
            panel.layer.quad(Quad::new(
                QuadKind::RoundedRectangle,
                bounds.into(),
                scope.color(ColorId::OverlayScreen),
            ));
        }
    }
}

/// The viewer's two lines, `width` by `height`: each of the
/// `RESOLUTION` points is the largest sample in its share of the audio, up
/// from the middle on the top line and down on the bottom one, flat with no
/// audio; their left boosts follow the window at `window_position` (a share
/// of the audio), the source's window size long, faded by `window_fade`.
pub fn viewer_lines(
    source: Option<&FileSource>,
    width: f32,
    height: f32,
    window_position: f32,
    window_fade: f32,
) -> (LineRenderer, LineRenderer) {
    let mut top = LineRenderer::new(RESOLUTION, false);
    let mut bottom = LineRenderer::new(RESOLUTION, false);
    let delta = width / (RESOLUTION as f32 - 1.0);
    let center = height * 0.5;
    for i in 0..RESOLUTION {
        top.set_x_at(i, delta * i as f32);
        bottom.set_x_at(i, delta * i as f32);
        top.set_y_at(i, center);
        bottom.set_y_at(i, center);
    }
    let Some(source) = source else {
        return (top, bottom);
    };
    let (Some(data), length) = (
        source.sample_buffer.data.as_ref(),
        source.sample_buffer.size,
    ) else {
        return (top, bottom);
    };
    if length <= 0 {
        return (top, bottom);
    }
    // setAudioPositions reads the padded buffer from its start
    // (`data.get()`, not `getDataBuffer()`), one sample before the audio;
    // the padding after it covers the last share.
    let buffer = &data[..];
    let length = i64::from(length);
    let resolution = RESOLUTION as i64;
    for i in 0..RESOLUTION {
        let at = i as i64;
        let start = (length * at / resolution).min(length) as usize;
        let end = ((length * (at + 1) + resolution - 1) / resolution).min(length) as usize;
        let mut max = buffer[start];
        for &sample in &buffer[start + 1..end.max(start + 1)] {
            max = max.max(sample);
        }
        top.set_y_at(i, center - max * center);
        bottom.set_y_at(i, center + max * center);
    }

    let window_size = (source.window_size / length as f64) as f32;
    let fade_length = (window_fade * window_size).max(1.0 / RESOLUTION as f32);
    let start = window_position - fade_length * 0.5;
    let end = window_position + window_size + fade_length * 0.5;
    for i in 0..RESOLUTION {
        let position = i as f32 / (RESOLUTION as f32 - 1.0);
        let phase = (position - start).min(end - position) / fade_length;
        let phase = phase.clamp(0.0, 1.0) * std::f32::consts::PI;
        let value = 0.5 - phase.cos() * 0.5;
        top.set_boost_left(i, value);
        bottom.set_boost_left(i, value);
    }
    (top, bottom)
}

/// `DefaultLookAndFeel::fillTextEditorBackground`, drawn with the overlay's
/// always-on-top components.
fn paint_text_editor_background(panel: &mut Panel, bounds: Rect) {
    let scope = &panel.scope;
    let rounding = scope.value(ValueId::WidgetRoundedCorner);
    let background = scope.color(ColorId::TextEditorBackground);
    let border = scope.color(ColorId::TextEditorBorder);
    panel.layer.custom(move |pixels| {
        pixels.fill_rounded_rect(bounds, rounding, background);
        pixels.draw_rounded_rect(RectF::from(bounds).reduced(0.5), rounding, 1.0, border);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `windowTextToSize`: numbers clamp to 1–9999.9 (JUCE's
    /// `getFloatValue` reads 0 from text that isn't one); a note name is its
    /// period, 0 at the viewer's sample rate of 0.
    #[test]
    fn window_text_reads_samples_or_a_note() {
        assert_eq!(window_text_to_size(" 512.5 ", 0), 512.5);
        assert_eq!(window_text_to_size("0", 0), 1.0);
        assert_eq!(window_text_to_size("size", 0), 1.0);
        assert_eq!(window_text_to_size("123456", 0), MAX_WINDOW_SIZE);
        assert_eq!(window_text_to_size("C4", 0), 0.0);
        // noteToMidiKey counts A and B below the octave's C: A4 is 220 Hz.
        let a4 = window_text_to_size("A4", 44_100);
        assert!((a4 - 200.454_55).abs() < 1e-3, "{a4}");
    }

    /// `setAudioPositions` and `setWindowValues`: each point is the largest
    /// sample (signed) in its share, mirrored about the middle, and the
    /// boosts rise to 1 inside the window.
    #[test]
    fn the_viewer_draws_peaks_and_the_window() {
        let mut source = FileSource::new();
        let mut audio = vec![0.0f32; 2560];
        audio[5] = 0.5;
        audio[7] = -0.9;
        // Sample 9 is the padded buffer's 10th, so it's in the second share.
        audio[9] = 0.8;
        audio[2559] = -0.25;
        source.load_buffer(&audio, 0);
        source.set_window_size(256.0);
        let (top, bottom) = viewer_lines(Some(&source), 255.0, 100.0, 0.5, 0.0);
        assert_eq!(top.x_at(255), 255.0);
        assert_eq!(top.y_at(0), 25.0);
        assert_eq!(bottom.y_at(0), 75.0);
        assert_eq!(top.y_at(1), 10.0);
        assert_eq!(top.y_at(2), 50.0);
        assert_eq!(top.y_at(255), 50.0);
        // The window spans 0.5 to 0.6 of the audio, faded over 1/256.
        assert_eq!(top.boost_left_at(0), 0.0);
        assert_eq!(top.boost_left_at(140), 1.0);
        assert_eq!(bottom.boost_left_at(140), 1.0);
        assert_eq!(top.boost_left_at(200), 0.0);

        let (empty, _) = viewer_lines(None, 255.0, 100.0, 0.0, 1.0);
        assert!((0..RESOLUTION).all(|i| empty.y_at(i) == 50.0));
    }
}
