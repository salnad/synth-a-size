// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The whole editor: upstream's `FullInterface`.
//!
//! Ports `vital/src/interface/editor_sections/full_interface.cpp` and
//! `header_section.cpp` as far as the editor goes: the background, the
//! header (logo area, tab selector, preset selector), the Voice and
//! Portamento sections, the plain bodies of the Bend and Keyboard sections,
//! and the popups. The other sections (oscillators, filters, modulators,
//! effects, matrix, keyboard) arrive in later milestones; until then their
//! areas show the background, as upstream's do before their children paint.
//!
//! Upstream's logo is replaced by the product name (migration/AGENTS.md
//! rule 2).

use egui::{Sense, Ui, UiBuilder};
use sas_engine::telemetry::Telemetry;
use sas_engine::Tuning;

use crate::colour::Colour;
use crate::controls::format;
use crate::controls::oscilloscope;
use crate::fonts::Face;
use crate::host::{Host, HostParam};
use crate::layout::{FullLayout, Rect};
use crate::paint::{PixelPainter, RectF};
use crate::popup::MenuOutcome;
use crate::scope::Scope;
use crate::sections::bend::{self, BendLayout, BendSection};
use crate::sections::keyboard::{self, KeyboardLayout, PlayingNote};
use crate::sections::portamento::{self, PortamentoLayout, PortamentoSection};
use crate::sections::voice::{self, VoiceLayout, VoiceSection};
use crate::sections::volume::{PeakLevels, VolumeLayout, VolumeSection};
use crate::sections::{
    MenuOwner, Overlay, Panel, TextEntry, MENU_DEFAULT_VALUE, MENU_MANUAL_ENTRY,
};
use crate::skin::{ColorId, Section, Skin, ValueId};

/// What the editor shows of the synth's state.
pub trait EditorModel: Send + Sync + 'static {
    /// `SynthBase::getPresetName`: the loaded preset's name, empty for the
    /// init preset.
    fn preset_name(&self) -> String;

    /// The host parameter for synth control `name` (its registry name).
    /// The pointer must stay valid as long as the model does.
    fn param(&self, name: &str) -> Option<HostParam> {
        let _ = name;
        None
    }

    /// The on-screen keyboard pressed `note` with `velocity` (0 to 1), on
    /// channel 1 (`MidiKeyboardState::noteOn`).
    fn note_on(&self, note: u8, velocity: f32) {
        let _ = (note, velocity);
    }

    /// The on-screen keyboard released `note` (`MidiKeyboardState::noteOff`).
    fn note_off(&self, note: u8) {
        let _ = note;
    }

    /// The notes held on any channel, from the keyboard or host MIDI: bit n
    /// is note n (`isNoteOnForChannels(0xffff, n)`).
    fn held_notes(&self) -> u128 {
        0
    }

    /// What the playing engine publishes for the editor (meters, the
    /// oscilloscope, modulation values). None draws them at rest.
    fn telemetry(&self) -> Option<Telemetry> {
        None
    }

    /// `SynthBase::getTuningName`: the name of the tuning the synth plays.
    fn tuning_name(&self) -> String {
        String::new()
    }

    /// `SynthBase::loadTuningFile` / `setTuning`: play with `tuning` from
    /// now on.
    fn load_tuning(&self, tuning: Tuning) {
        let _ = tuning;
    }
}

/// What the editor copies from the telemetry snapshot each frame.
struct Live {
    peaks: PeakLevels,
    oscilloscope: Vec<[f32; 2]>,
}

impl Live {
    fn read(model: &dyn EditorModel) -> Option<Live> {
        let telemetry = model.telemetry()?;
        Some(telemetry.read(|snapshot| {
            let peak = snapshot.status_by_name("peak_meter").unwrap_or_default();
            let memory = snapshot
                .status_by_name("peak_meter_memory")
                .unwrap_or_default();
            Live {
                peaks: PeakLevels {
                    peak: [peak[0], peak[1]],
                    memory: [memory[0], memory[1]],
                },
                oscilloscope: snapshot.oscilloscope().to_vec(),
            }
        }))
    }
}

/// The product name, drawn where upstream draws its logo.
pub const PRODUCT_NAME: &str = "synth-a-size";

/// The header's tabs (`HeaderSection`'s `tab_selector_` names).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Voice,
    Effects,
    Matrix,
    Advanced,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Voice, Tab::Effects, Tab::Matrix, Tab::Advanced];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Voice => "VOICE",
            Tab::Effects => "EFFECTS",
            Tab::Matrix => "MATRIX",
            Tab::Advanced => "ADVANCED",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The editor's own state: the skin, what is selected and the open popups.
pub struct Interface {
    skin: Skin,
    tab: Tab,
    voice: VoiceSection,
    portamento: PortamentoSection,
    volume: VolumeSection,
    bend: BendSection,
    playing_note: PlayingNote,
    overlay: Overlay,
}

impl Default for Interface {
    fn default() -> Interface {
        Interface::new(Skin::default())
    }
}

impl Interface {
    pub fn new(skin: Skin) -> Interface {
        Interface {
            skin,
            tab: Tab::Voice,
            voice: VoiceSection::default(),
            portamento: PortamentoSection::default(),
            volume: VolumeSection::default(),
            bend: BendSection::default(),
            playing_note: PlayingNote::default(),
            overlay: Overlay::default(),
        }
    }

    pub fn skin(&self) -> &Skin {
        &self.skin
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    /// The open popups.
    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// The note the on-screen keyboard is playing, for releasing it when the
    /// window closes.
    pub fn playing_note(&self) -> PlayingNote {
        self.playing_note.clone()
    }

    /// Draws the editor into `ui`'s whole area and handles its input.
    pub fn show(&mut self, ui: &mut Ui, host: &Host) {
        let area = ui.max_rect();
        self.show_in(ui, area, host);
    }

    /// Draws the editor into `area` (points) and handles its input.
    pub fn show_in(&mut self, ui: &mut Ui, area: egui::Rect, host: &Host) {
        let pixels_per_point = ui.ctx().pixels_per_point();
        let width = (area.width() * pixels_per_point).round() as i32;
        let height = (area.height() * pixels_per_point).round() as i32;
        let layout = FullLayout::new(&self.skin, width, height);
        let painter = ui.painter_at(area);
        let pixels = PixelPainter::new(&painter, area, pixels_per_point);

        self.paint_background(&pixels, &layout, width, height);
        self.paint_header(&pixels, &layout, host.model());
        self.handle_tabs(ui, &pixels, &layout);
        let live = Live::read(host.model());
        if live.is_some() {
            // The meters and oscilloscope move with the audio.
            ui.ctx().request_repaint();
        }
        self.show_header(ui, host, &pixels, &layout, live.as_ref());
        self.show_tab(ui, host, &pixels, &layout);
        self.show_sections(ui, host, &pixels, &layout);
        self.show_overlay(ui, host, &pixels, &layout, Rect::new(0, 0, width, height));
    }

    /// The header's live parts: the volume section and the oscilloscope.
    fn show_header(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layout: &FullLayout,
        live: Option<&Live>,
    ) {
        let header = Scope::top(&self.skin, layout.size_ratio).within(&[Section::Header]);

        let oscilloscope = layout.header.oscilloscope;
        oscilloscope::paint_background(pixels, &header, oscilloscope);
        oscilloscope::paint(
            pixels,
            &header,
            oscilloscope,
            live.map(|live| live.oscilloscope.as_slice()),
            header.color(ColorId::Background),
        );

        let volume_layout = VolumeLayout::new(layout.header.volume);
        self.volume
            .paint_background(pixels, &header, &volume_layout);
        let mut panel = Panel::new(
            ui,
            host,
            pixels,
            header,
            &mut self.overlay,
            layout.header.volume,
        );
        self.volume
            .show(&mut panel, &volume_layout, live.map(|live| live.peaks));
        panel.finish();
    }

    /// The selected tab's contents (`FullInterface::tabSelected` shows one
    /// of the synthesis, effects, matrix and master controls interfaces).
    /// The sections below them show on every tab.
    fn show_tab(&mut self, ui: &mut Ui, host: &Host, pixels: &PixelPainter, layout: &FullLayout) {
        let _ = (ui, host, pixels, layout);
        match self.tab {
            // G3b: oscillators, sample and filters.
            Tab::Voice => {}
            // G6: the effects interface, in `layout.effects`.
            Tab::Effects => {}
            // G4: the modulation matrix.
            Tab::Matrix => {}
            // G6: the master controls interface, in `layout.main`.
            Tab::Advanced => {}
        }
    }

    /// The Bend, Keyboard, Voice and Portamento sections.
    fn show_sections(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layout: &FullLayout,
    ) {
        let top = Scope::top(&self.skin, layout.size_ratio);

        let scope = top.within(voice::SKIN);
        let voice_layout = VoiceLayout::new(&scope, layout.voice);
        self.voice.paint_background(pixels, &scope, &voice_layout);
        let mut panel = Panel::new(ui, host, pixels, scope, &mut self.overlay, layout.voice);
        self.voice.show(&mut panel, &voice_layout);
        panel.finish();

        let scope = top.within(bend::SKIN);
        let bend_layout = BendLayout::new(&scope, layout.bend);
        self.bend.paint_background(pixels, &scope, &bend_layout);
        let mut panel = Panel::new(ui, host, pixels, scope, &mut self.overlay, layout.bend);
        self.bend.show(&mut panel, &bend_layout);
        panel.finish();

        let scope = top.within(keyboard::SKIN);
        let keyboard_layout = KeyboardLayout::new(layout.keyboard);
        keyboard::paint_background(pixels, &scope, &keyboard_layout);
        let mut panel = Panel::new(ui, host, pixels, scope, &mut self.overlay, layout.keyboard);
        let held = host.model().held_notes();
        keyboard::show(&mut panel, &keyboard_layout, held, &self.playing_note);
        panel.finish();

        let scope = top.within(portamento::SKIN);
        let portamento_layout = PortamentoLayout::new(&scope, layout.portamento);
        self.portamento
            .paint_background(pixels, &scope, &portamento_layout);
        let mut panel = Panel::new(
            ui,
            host,
            pixels,
            scope,
            &mut self.overlay,
            layout.portamento,
        );
        self.portamento.show(&mut panel, &portamento_layout);
        panel.finish();
    }

    /// The value popup, the selection menu and a slider's text entry, over
    /// everything else.
    fn show_overlay(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layout: &FullLayout,
        area: Rect,
    ) {
        let top = Scope::top(&self.skin, layout.size_ratio);
        if let Some((_, popup)) = &self.overlay.value_popup {
            popup.paint(pixels, &top);
        }

        if let Some(mut menu) = self.overlay.menu.take() {
            match menu.show(ui, pixels, &top, area) {
                MenuOutcome::Open => self.overlay.menu = Some(menu),
                MenuOutcome::Closed => self.overlay.menu = None,
                MenuOutcome::Selected(id) => {
                    self.overlay.menu = None;
                    match menu.owner {
                        MenuOwner::Slider(slider, _) if id == MENU_DEFAULT_VALUE => {
                            host.set_once(slider.name, slider.info.default_value);
                        }
                        MenuOwner::Slider(slider, entry_bounds) if id == MENU_MANUAL_ENTRY => {
                            self.overlay.text_entry =
                                Some(TextEntry::new(host, &slider, entry_bounds));
                        }
                        MenuOwner::Slider(..) => {}
                        MenuOwner::StereoMode => host.set_once("stereo_mode", id as f32),
                    }
                }
            }
        }

        show_text_entry(&mut self.overlay, ui, host, pixels, &top);
    }

    fn top(&self, layout: &FullLayout) -> Scope<'_> {
        Scope::top(&self.skin, layout.size_ratio)
    }

    /// `FullInterface::paintBackground`, for the sections ported so far.
    fn paint_background(
        &self,
        pixels: &PixelPainter,
        layout: &FullLayout,
        width: i32,
        height: i32,
    ) {
        let top = self.top(layout);
        pixels.fill_rect(
            Rect::new(0, 0, width, height),
            top.color(ColorId::Background),
        );

        let header_body = top.within(&[Section::Header]).color(ColorId::Body);
        for bar in layout.bars {
            pixels.fill_rect(bar, header_body);
        }
    }

    /// `HeaderSection::paintBackground` and its children's paint: the logo
    /// area, the tab selector and the preset selector.
    fn paint_header(&self, pixels: &PixelPainter, layout: &FullLayout, model: &dyn EditorModel) {
        let header = self.top(layout).within(&[Section::Header]);
        let bounds = layout.header.bounds;
        let logo_section_width = (header.standard_knob_size() + header.padding()) as i32;
        pixels.fill_rect(
            Rect::new(bounds.x, bounds.y, logo_section_width, bounds.height),
            header.color(ColorId::Body),
        );

        self.paint_logo(pixels, layout);
        self.paint_tabs(pixels, layout);
        self.paint_preset_selector(pixels, layout, model);
    }

    /// The product name in the logo area, sized to fit it.
    fn paint_logo(&self, pixels: &PixelPainter, layout: &FullLayout) {
        let logo = self.top(layout).within(&[Section::Logo, Section::Header]);
        let bounds = layout.header.logo;
        // The name spans the logo button's width (`LogoSection::resized`:
        // a square as tall as the header less 2 * kLogoPaddingY).
        let point_height = bounds.width as f32 * 0.19;
        pixels.text_centred(
            PRODUCT_NAME,
            bounds,
            Face::TitleRegular,
            point_height,
            logo.color(ColorId::WidgetPrimary1),
        );
    }

    /// `TabSelector::paint`.
    fn paint_tabs(&self, pixels: &PixelPainter, layout: &FullLayout) {
        const LIGHT_HEIGHT_PERCENT: f32 = 0.08;
        const FONT_HEIGHT_PERCENT: f32 = 0.26;
        let header = self.top(layout).within(&[Section::Header]);
        let bounds = layout.header.tabs;
        let selected = self.tab.index();
        let light_height = ((bounds.height as f32 * LIGHT_HEIGHT_PERCENT) as i32).max(1);
        let highlight = header.color(ColorId::WidgetPrimary1);

        pixels.fill_rect(
            Rect::new(bounds.x, bounds.y, bounds.width, light_height),
            header.color(ColorId::LightenScreen),
        );
        let from = tab_x(bounds.width, selected);
        let to = tab_x(bounds.width, selected + 1);
        pixels.fill_rect(
            Rect::new(bounds.x + from, bounds.y, to - from, light_height),
            highlight,
        );

        let point_height = bounds.height as f32 * FONT_HEIGHT_PERCENT;
        for tab in Tab::ALL {
            let from = tab_x(bounds.width, tab.index());
            let to = tab_x(bounds.width, tab.index() + 1);
            let colour = if tab == self.tab {
                highlight
            } else {
                header.color(ColorId::TextComponentText)
            };
            let cell = Rect::new(bounds.x + from, bounds.y, to - from, bounds.height);
            pixels.text_centred(tab.label(), cell, Face::Light, point_height, colour);
        }
    }

    /// `PresetSelector::paintBackground` and its text and arrow buttons.
    fn paint_preset_selector(
        &self,
        pixels: &PixelPainter,
        layout: &FullLayout,
        model: &dyn EditorModel,
    ) {
        let header = self.top(layout).within(&[Section::Header]);
        let preset = layout.header.preset;
        pixels.fill_rounded_rect(
            preset.bounds,
            header.value(ValueId::WidgetRoundedCorner),
            header.color(ColorId::PopupSelectorBackground),
        );

        let mut name = model.preset_name();
        if name.is_empty() {
            name = "Init Preset".to_string();
        }
        pixels.text_centred(
            &name,
            preset.text,
            Face::Title,
            preset.text_size,
            header.color(ColorId::PresetText),
        );

        let arrow = header.color(ColorId::IconButtonOff);
        paint_arrow(
            pixels,
            preset.previous,
            &[(0.65, 0.3), (0.35, 0.5), (0.65, 0.7)],
            arrow,
        );
        paint_arrow(
            pixels,
            preset.next,
            &[(0.35, 0.3), (0.65, 0.5), (0.35, 0.7)],
            arrow,
        );
    }

    /// `TabSelector::mouseDown` / `mouseDrag` / `mouseUp`: the tab under the
    /// pointer is selected as soon as the button goes down, and follows a
    /// drag.
    fn handle_tabs(&mut self, ui: &mut Ui, pixels: &PixelPainter, layout: &FullLayout) {
        let bounds = layout.header.tabs;
        let response = ui.interact(
            pixels.rect(bounds.into()),
            ui.scope_id().with("tabs"),
            Sense::click_and_drag(),
        );
        if !response.is_pointer_button_down_on() {
            return;
        }
        if let Some(pointer) = response.interact_pointer_pos() {
            let x = (pointer.x - pixels.rect(bounds.into()).min.x) * pixels.pixels_per_point();
            let index = (x * Tab::ALL.len() as f32 / bounds.width as f32) as i32;
            self.tab = Tab::ALL[index.clamp(0, Tab::ALL.len() as i32 - 1) as usize];
        }
    }
}

/// `SynthSlider`'s text entry: a monospace field over the slider. Return
/// or leaving the field sets the value (`setSliderPositionFromText`).
fn show_text_entry(
    overlay: &mut Overlay,
    ui: &mut Ui,
    host: &Host,
    pixels: &PixelPainter,
    top: &Scope,
) {
    let Some(entry) = &mut overlay.text_entry else {
        return;
    };
    // DefaultLookAndFeel::fillTextEditorBackground.
    let rounding = top.value(ValueId::WidgetRoundedCorner);
    pixels.fill_rounded_rect(
        entry.bounds,
        rounding,
        top.color(ColorId::TextEditorBackground),
    );
    pixels.draw_rounded_rect(
        RectF::from(entry.bounds).reduced(0.5),
        rounding,
        1.0,
        top.color(ColorId::TextEditorBorder),
    );
    let rect = pixels.rect(entry.bounds.into());
    let font_size = pixels.points(top.value(ValueId::TextComponentFontSize));
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect));
    child.visuals_mut().selection.bg_fill = top.color(ColorId::TextEditorSelection).to_egui();
    child.visuals_mut().text_cursor.stroke.color = top.color(ColorId::TextEditorCaret).to_egui();
    let edit = egui::TextEdit::singleline(&mut entry.text)
        .frame(egui::Frame::NONE)
        .font(Face::Mono.at(font_size))
        .text_color(top.color(ColorId::BodyText).to_egui())
        .align(egui::Align2::CENTER_CENTER)
        .desired_width(rect.width());
    let response = child.add_sized(rect.size(), edit);
    if !entry.focused {
        // Focus once the press that opened the entry is over, so its
        // release does not count as a click elsewhere.
        if !ui.input(|input| input.pointer.any_down()) {
            response.request_focus();
            // setSelectAllWhenFocused: typing replaces the value.
            let mut state = egui::TextEdit::load_state(ui.ctx(), response.id).unwrap_or_default();
            let all = egui::text_selection::CCursorRange::select_all_str(&entry.text);
            state.cursor.set_char_range(Some(all));
            egui::TextEdit::store_state(ui.ctx(), response.id, state);
            entry.focused = true;
        }
    } else if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        // JUCE's TextEditor keeps focus on Escape; the entry stays open.
        response.request_focus();
    } else if response.lost_focus() {
        let slider = entry.slider;
        if !entry.text.is_empty() {
            let value = format::value_from_text(slider.info, &slider.format, &entry.text);
            host.set_once(slider.name, slider.constrain(value) as f32);
        }
        overlay.text_entry = None;
    }
}

/// `TabSelector::getTabX`.
fn tab_x(width: i32, position: usize) -> i32 {
    let num_types = Tab::ALL.len() as f32;
    ((width + 1) as f32 * position as f32 / num_types).round() as i32
}

/// An `OpenGlShapeButton` arrow: `PresetSelector`'s chevron path (unit
/// coordinates), stroked 0.05 of the button's size with round ends.
fn paint_arrow(pixels: &PixelPainter, bounds: Rect, path: &[(f32, f32)], colour: Colour) {
    let size = bounds.width.min(bounds.height) as f32;
    let points: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| (bounds.x as f32 + x * size, bounds.y as f32 + y * size))
        .collect();
    pixels.polyline(&points, 0.05 * size, colour);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_positions_match_upstream() {
        assert_eq!(
            (0..=4).map(|i| tab_x(395, i)).collect::<Vec<_>>(),
            vec![0, 99, 198, 297, 396]
        );
    }
}
