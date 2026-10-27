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

use egui::{Ui, UiBuilder};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sas_common::line_generator::LineGenerator;
use sas_engine::sas_oscillators::sas_wavetable::creator::AudioFileLoadStyle;
use sas_engine::sas_oscillators::sas_wavetable::Wavetable;
use sas_engine::telemetry::Telemetry;
use sas_engine::Tuning;
use sas_preset::user_folders::{self, Kind};
use sas_preset::{GuiConnection, SampleState, SlotValues};

use crate::chat::{ChatPlacement, ChatStyle};
use crate::colour::Colour;
use crate::controls::format;
use crate::controls::oscilloscope;
use crate::controls::paths;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::tab_selector::{self, TabSelector};
use crate::fonts::Face;
use crate::host::{Host, HostParam};
use crate::layout::{FullLayout, Rect};
use crate::local::LocalValues;
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::sections::advanced::{self, AdvancedLayout, AdvancedTab};
use crate::sections::bend::{self, BendLayout, BendSection};
use crate::sections::delete_section::{DeleteOutcome, DeleteSection};
use crate::sections::effects::EffectsTab;
use crate::sections::extra_mod::{self, ExtraModLayout, ExtraModSection};
use crate::sections::keyboard::{self, KeyboardLayout, PlayingNote};
use crate::sections::modulation::ModulationInterface;
use crate::sections::portamento::{self, PortamentoLayout, PortamentoSection};
use crate::sections::preset_browser::{self, PresetBrowser, PresetBrowserLayout};
use crate::sections::preset_selector;
use crate::sections::synthesis::SynthesisInterface;
use crate::sections::voice::{self, VoiceLayout, VoiceSection};
use crate::sections::volume::{PeakLevels, VolumeLayout, VolumeSection};
use crate::sections::{Overlay, Panel};
use crate::skin::{ColorId, Section, Skin, ValueId};
use crate::text::{Justify, Overflow};

/// A connected modulation slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    /// The bank slot (0-based; its controls are `modulation_{slot + 1}_*`).
    pub slot: usize,
    pub source: String,
    pub destination: String,
}

/// What the editor shows of the synth's state.
pub trait EditorModel: Send + Sync + 'static {
    /// `SynthBase::getPresetName`: the loaded preset's name, empty for the
    /// init preset.
    fn preset_name(&self) -> String;

    /// `SynthBase::getAuthor`.
    fn preset_author(&self) -> String {
        String::new()
    }

    /// `SynthBase::getComments`.
    fn preset_comments(&self) -> String {
        String::new()
    }

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

    /// LFO `index`'s shape (`SynthBase::getLfoSource`). None draws the
    /// init shape.
    fn lfo_shape(&self, index: usize) -> Option<LineGenerator> {
        let _ = index;
        None
    }

    /// Replaces LFO `index`'s shape. False when the synth can't take it
    /// now (its edit queue is full); the editor tries again next frame.
    fn set_lfo_shape(&self, index: usize, line: LineGenerator) -> bool {
        let _ = (index, line);
        true
    }

    /// `SynthBase::getMacroName`: macro `index`'s (0-based) name, "MACRO n"
    /// when blank.
    fn macro_name(&self, index: usize) -> String {
        format!("MACRO {}", index + 1)
    }

    /// The modulation bank's connected slots, in the order the synth keeps
    /// them (`SynthBase::mod_connections_`: slot order after a load, then
    /// each new connection last).
    fn modulations(&self) -> Arc<[Route]> {
        Arc::from([])
    }

    /// `SynthGuiInterface::connectModulation` with `values` for a new slot
    /// (`setModulationValues`). The engine takes the values with the
    /// connection; the caller sets the returned `parameter_changes` on the
    /// host parameters when the user's action ends. None when the synth
    /// can't connect them now (no free slot, or its edit queue is full).
    fn connect_modulation(
        &self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Option<GuiConnection> {
        let _ = (source, destination, values);
        None
    }

    /// `SynthGuiInterface::setModulationValues` in the engine, for the
    /// slot connecting `source` to `destination` now (nothing if they
    /// aren't connected); the caller tells the host when the user's action
    /// ends. False when the synth can't take it now (its edit queue is
    /// full).
    fn set_connection_values(&self, source: &str, destination: &str, values: SlotValues) -> bool {
        let _ = (source, destination, values);
        true
    }

    /// Changes when a preset load or host state restore replaces the synth's
    /// state (`EditHandle::generation`).
    fn edit_generation(&self) -> u64 {
        0
    }

    /// `SynthGuiInterface::disconnectModulation`; the slot, if they were
    /// connected and the synth took the change.
    fn disconnect_modulation(&self, source: &str, destination: &str) -> Option<usize> {
        let _ = (source, destination);
        None
    }

    /// `LoadSave::displayHzFrequency`: cutoff sliders show hertz rather
    /// than their note value (the "hz_frequency" setting, off by default).
    fn hz_frequency_display(&self) -> bool {
        false
    }

    /// `LoadSave::saveDisplayHzFrequency`: the Advanced tab's frequency
    /// units changed; keep it for every editor from now on.
    fn set_hz_frequency_display(&self, hz_frequency: bool) {
        let _ = hz_frequency;
    }

    /// `SynthBase::getTuningName`: the name of the tuning the synth plays.
    fn tuning_name(&self) -> String {
        String::new()
    }

    /// A copy of the tuning the synth plays (`SynthBase::getTuning`).
    /// Upstream changes its tuning in place, so edit this copy the same way
    /// (a `.kbm` keeps the scale and a scale keeps the keyboard map;
    /// `TuningSelector`'s built-ins `set_name` then load a scale or
    /// `set_default_tuning`) and pass it to [`load_tuning`](Self::load_tuning).
    fn tuning(&self) -> Tuning {
        Tuning::new()
    }

    /// Play with `tuning` from now on (`SynthBase::loadTuningFile`).
    fn load_tuning(&self, tuning: Tuning) {
        let _ = tuning;
    }

    /// The user's chosen data folder (`LoadSave::getDataDirectory`'s
    /// `"data_directory"` setting), if any; see [`user_folder`].
    fn data_directory(&self) -> Option<PathBuf> {
        None
    }

    /// `LoadSave::setDataDirectory`: keep `folder` as the data folder.
    fn set_data_directory(&self, folder: PathBuf) {
        let _ = folder;
    }

    /// `SynthBase::getActiveFile`: the preset file last loaded or saved.
    fn active_preset_file(&self) -> Option<PathBuf> {
        None
    }

    /// `SynthBase::loadFromFile`: plays the preset at `path`. The error is
    /// upstream's reason for the user ("Preset file is corrupted.", "Preset
    /// was created with a newer version.", or empty for a missing file).
    fn load_preset_file(&self, path: &Path) -> Result<(), String> {
        let _ = path;
        Err("there is no synth to load it into".to_string())
    }

    /// `SynthBase::saveToFile`: writes the current preset to `path` with
    /// the preset extension, naming it after the file, and returns the path
    /// written.
    fn save_preset_file(&self, path: &Path) -> Result<PathBuf, String> {
        let _ = path;
        Err("there is no synth to save".to_string())
    }

    /// `SynthBase::loadInitPreset`.
    fn init_preset(&self) {}

    /// `LoadSave::getFavorites`: the favourite presets' full paths.
    fn favourites(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// `LoadSave::addFavorite` / `removeFavorite`.
    fn set_favourite(&self, path: &Path, favourite: bool) {
        let _ = (path, favourite);
    }

    /// Moves on every preset load, init and host state restore (the edit
    /// bus's generation).
    fn preset_generation(&self) -> u64 {
        0
    }

    /// `SynthBase::armMidiLearn(name)`: the next MIDI controller the engine
    /// receives learns parameter `name`.
    fn arm_midi_learn(&self, _name: &str) {}

    /// `SynthBase::cancelMidiLearn()`.
    fn cancel_midi_learn(&self) {}

    /// `SynthBase::clearMidiLearn(name)`: forgets every controller learned
    /// for `name`.
    fn clear_midi_learn(&self, _name: &str) {}

    /// `SynthBase::isMidiMapped(name)`.
    fn is_midi_mapped(&self, _name: &str) -> bool {
        false
    }

    /// The MIDI controllers `name` is learned on, ascending (upstream has
    /// only [`is_midi_mapped`](Self::is_midi_mapped)).
    fn midi_learn_ccs(&self, _name: &str) -> Vec<i32> {
        Vec::new()
    }

    /// The editor window opened (`true`) or closed. While it is open the
    /// model keeps the controls MIDI-learned controllers move, for
    /// [`drain_midi_moved`](Self::drain_midi_moved).
    fn set_editor_open(&self, _open: bool) {}

    /// Calls `f` with each control a MIDI-learned controller moved since the
    /// last call while the editor was open, so the editor can tell the host
    /// (upstream's `valueChangedThroughMidi` calls `setValueNotifyHost`).
    fn drain_midi_moved(&self, _f: &mut dyn FnMut(&'static str)) {}

    /// The editor is about to send the host `normalized` (as nice-plug passes
    /// it) for control `name`, a MIDI-learned move, so the plugin can tell
    /// the host's echo of it from host automation.
    fn midi_move_forwarded(&self, _name: &str, _normalized: f32) {}

    /// The editor window opened: the host context for parameter gestures
    /// and state loads (GUI thread only), and a way to ask for a frame from
    /// another thread.
    fn attach(
        &self,
        _gui: nice_plug::context::gui::GuiContext,
        _wake: Box<dyn Fn() + Send + Sync>,
    ) {
    }

    /// The LLM chat, if the plugin has one.
    fn chat(&self) -> Option<&dyn crate::chat::ChatHost> {
        None
    }

    /// Oscillator `oscillator`'s rendered wavetable, for drawing
    /// (`SynthBase::getWavetable`), shared rather than copied. None for an
    /// index past the oscillators, or without an engine.
    fn wavetable(&self, _oscillator: usize) -> Option<Arc<Wavetable>> {
        None
    }

    /// The name oscillator `oscillator`'s wavetable shows
    /// (`FullInterface::getWavetableName`: its creator's name).
    fn wavetable_name(&self, _oscillator: usize) -> String {
        String::new()
    }

    /// Oscillator `oscillator`'s wavetable creator as upstream's JSON
    /// (`FullInterface::getWavetableJson`), for copying. None past the
    /// oscillators, or without an engine.
    fn wavetable_json(&self, _oscillator: usize) -> Option<String> {
        None
    }

    /// Loads wavetable JSON into oscillator `oscillator`
    /// (`FullInterface::loadWavetable`): a creator's state or a line shape,
    /// as `WavetableCreator::isValidJson` accepts. Returns false, changing
    /// nothing, when `json` isn't one or can't be read or rendered.
    fn load_wavetable_json(&self, _oscillator: usize, _json: &str) -> bool {
        false
    }

    /// Resets oscillator `oscillator`'s wavetable to the default one
    /// (`FullInterface::loadDefaultWavetable`: `WavetableCreator::init`).
    fn load_default_wavetable(&self, _oscillator: usize) {}

    /// "Resynthesize Preset to Wavetable"
    /// (`FullInterface::resynthesizeToWavetable`): renders the current patch
    /// playing a low note and loads the result as oscillator `oscillator`'s
    /// wavetable. Returns at once; the wavetable changes when the render is
    /// done (`content_revision` moves on).
    fn resynthesize_to_wavetable(&self, _oscillator: usize) {}

    /// The audio file extensions a wavetable view takes
    /// (`AudioFileDropSource::getExtensions`): lowercase, without the dot.
    /// Empty takes none.
    fn audio_file_extensions(&self) -> &'static [&'static str] {
        &[]
    }

    /// `FullInterface::loadAudioAsWavetable`: loads the audio file at `path`
    /// into oscillator `oscillator`'s wavetable as `style` (splice, vocode
    /// or pitch splice), named after the file. Returns false, changing
    /// nothing, when the file can't be read.
    fn load_audio_as_wavetable(
        &self,
        _oscillator: usize,
        _path: &std::path::Path,
        _style: AudioFileLoadStyle,
    ) -> bool {
        false
    }

    /// Calls `f` with the sample oscillator's audio, read in place: nothing
    /// is copied. `left` is the mono or left channel `SampleViewer` draws
    /// (`Sample::buffer`, `originalLength`). `f` is not called when there is
    /// no sample (the default, without a plugin).
    ///
    /// The plugin runs `f` holding the edit bus's mirror lock, which is not
    /// reentrant: calling any other `EditorModel` method from inside `f`
    /// deadlocks the GUI thread. Copy out what you need and return.
    fn with_sample(&self, _f: &mut dyn FnMut(&SampleState)) {}

    /// `Sample::getName`.
    fn sample_name(&self) -> String {
        String::new()
    }

    /// Moves on whenever the wavetables, the sample or anything else the
    /// plugin keeps besides its host parameters may have changed (each
    /// edit, preset load and state restore), so drawings of them can be
    /// cached against it. Read it before the content you cache: a change
    /// that lands in between then moves the revision past the one stored,
    /// and the next frame reads the content again.
    fn content_revision(&self) -> u64 {
        0
    }
}

/// The user folder for `kind`: under the model's data folder if it set one
/// that exists, else under the default root.
pub fn user_folder(model: &dyn EditorModel, kind: Kind) -> Option<PathBuf> {
    let root = user_folders::root(model.data_directory().as_deref())?;
    Some(user_folders::folder(&root, kind))
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

/// `HeaderSection`'s tab selector.
const HEADER_TABS: TabSelector = TabSelector {
    names: &["VOICE", "EFFECTS", "MATRIX", "ADVANCED"],
    font_height_percent: tab_selector::DEFAULT_FONT_HEIGHT_PERCENT,
    active: true,
};

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Voice, Tab::Effects, Tab::Matrix, Tab::Advanced];

    pub fn label(self) -> &'static str {
        HEADER_TABS.names[self.index()]
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
    synthesis: SynthesisInterface,
    modulation: ModulationInterface,
    playing_note: PlayingNote,
    extra_mod: ExtraModSection,
    overlay: Overlay,
    locals: Arc<LocalValues>,
    effects: EffectsTab,
    advanced: AdvancedTab,
    preset_browser: PresetBrowser,
    /// `preset_browser_->isVisible()`.
    preset_browser_visible: bool,
    /// The visibility the browser last saw (`visibilityChanged`).
    preset_browser_was_visible: bool,
    /// `delete_section_`, while it asks before deleting a preset.
    delete_section: Option<DeleteSection>,
    preset_modified: preset_selector::Modified,
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
            synthesis: SynthesisInterface::default(),
            modulation: ModulationInterface::default(),
            playing_note: PlayingNote::default(),
            extra_mod: ExtraModSection::default(),
            overlay: Overlay::default(),
            locals: Arc::default(),
            effects: EffectsTab::default(),
            advanced: AdvancedTab::default(),
            preset_browser: PresetBrowser::default(),
            preset_browser_visible: false,
            preset_browser_was_visible: false,
            delete_section: None,
            preset_modified: preset_selector::Modified::default(),
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

    /// Reads the presets again when the browser next needs them, as a new
    /// editor window does.
    pub fn reload_presets(&mut self) {
        self.preset_browser.reload();
    }

    /// The preset the delete overlay asks about, while it shows.
    pub fn deleting(&self) -> Option<&Path> {
        self.delete_section
            .as_ref()
            .map(|section| section.file.as_path())
    }

    /// The preset browser's folder list as it shows it.
    pub fn browser_folders(&self) -> Vec<crate::sections::selection_list::Selection> {
        self.preset_browser.folder_rows().to_vec()
    }

    /// The preset browser's list as it shows it.
    pub fn browser_presets(&self) -> Vec<PathBuf> {
        self.preset_browser.shown()
    }

    /// `PresetBrowser::externalPresetLoaded`: the arrows step through
    /// `file`'s folder until a preset is picked from the list.
    pub fn external_preset_loaded(&mut self, file: Option<PathBuf>) {
        self.preset_browser.external_preset_loaded(file);
    }

    pub fn preset_browser_visible(&self) -> bool {
        self.preset_browser_visible
    }

    /// `FullInterface::setPresetBrowserVisibility`: the browser in place of
    /// the tab, the modulation column and the voice and portamento
    /// sections, with the header's temporary tab naming it.
    pub fn set_preset_browser_visible(&mut self, visible: bool) {
        self.preset_browser_visible = visible;
    }

    /// Scrolls the Effects tab's column down `y` pixels.
    pub fn set_effects_scroll(&mut self, y: f32) {
        self.effects.set_scroll(y);
    }

    /// How far the Effects tab's column is scrolled, in pixels.
    pub fn effects_scroll(&self) -> f32 {
        self.effects.scroll()
    }

    /// The modulation column.
    pub fn modulation(&self) -> &ModulationInterface {
        &self.modulation
    }

    /// The open popups.
    pub fn overlay(&self) -> &Overlay {
        &self.overlay
    }

    /// The connection a drag to modulate has made, for writing to the host
    /// if the window closes mid-drag.
    pub fn pending_connection(&self) -> crate::modulation::PendingConnection {
        self.overlay.modulation.pending_connection()
    }

    /// Whether the header marks the preset modified (`*`).
    pub fn preset_modified(&self) -> bool {
        self.preset_modified.is_modified()
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
        let locals = self.locals.clone();
        let host = &host.with_locals(&locals);
        self.preset_modified.update(host);
        let pixels_per_point = ui.ctx().pixels_per_point();
        let width = (area.width() * pixels_per_point).round() as i32;
        let height = (area.height() * pixels_per_point).round() as i32;
        let layout = FullLayout::new(&self.skin, width, height);
        let painter = ui.painter_at(area);
        let pixels = PixelPainter::new(&painter, area, pixels_per_point);

        let model = host.model();
        if self.preset_browser_visible != self.preset_browser_was_visible {
            self.preset_browser.visibility_changed(model);
            self.preset_browser_was_visible = self.preset_browser_visible;
        }
        self.overlay
            .modulation
            .begin_frame(model.modulations(), model.edit_generation());
        self.overlay.modulation.read_meters(model.telemetry());
        self.paint_background(&pixels, &layout, width, height);
        self.paint_header(&pixels, &layout, host.model());
        self.handle_tabs(ui, &pixels, &layout);
        let live = Live::read(host.model());
        if live.is_some() {
            // The meters and oscilloscope move with the audio.
            ui.ctx().request_repaint();
        }
        self.show_header(ui, host, &pixels, &layout, live.as_ref());
        self.show_tab(ui, host, &pixels, &layout, live.as_ref());
        self.show_sections(ui, host, &pixels, &layout);
        let top = Scope::top(&self.skin, layout.size_ratio);
        self.overlay.modulation.finish_frame(host, &pixels, &top);
        self.show_amount_knobs(ui, host, &pixels, &layout, width, height);
        let top = Scope::top(&self.skin, layout.size_ratio);
        self.overlay.modulation.paint_drag(host, &pixels, &top);
        self.show_overlay(ui, host, &pixels, &layout, Rect::new(0, 0, width, height));
        // The delete overlay, over everything (deleteRequested, fileDeleted).
        if let Some(file) = self.preset_browser.take_delete_request() {
            self.delete_section = Some(DeleteSection { file });
        }
        if let Some(section) = self.delete_section.clone() {
            let top = Scope::top(&self.skin, layout.size_ratio);
            match section.show(ui, &pixels, &top, Rect::new(0, 0, width, height)) {
                DeleteOutcome::Open => {}
                DeleteOutcome::Cancelled => self.delete_section = None,
                DeleteOutcome::Deleted => {
                    self.delete_section = None;
                    self.preset_browser.file_deleted(host.model());
                }
            }
        }
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

        let mut panel = Panel::new(
            ui,
            host,
            pixels,
            header,
            &mut self.overlay,
            layout.header.bounds,
        );
        let browse = preset_selector::show(
            &mut panel,
            &layout.header,
            &mut self.preset_modified,
            &mut self.preset_browser,
            self.preset_browser_visible,
        );
        if self.preset_browser_visible {
            // exit_temporary_button_: clearTemporaryTab.
            let exit = ShapeButton {
                shape: Shape::Icon {
                    key: "header exit",
                    path: paths::exit_x(),
                },
                colours: ShapeColours::icon(&header),
            };
            if panel
                .shape_button("exit temporary", exit, layout.header.exit_temporary)
                .clicked
            {
                self.preset_browser_visible = false;
            }
        }
        panel.finish();
        if let Some(visible) = browse {
            self.preset_browser_visible = visible;
        }
        advanced::poll_tuning_file(host, ui.ctx());
    }

    /// The selected tab's contents (`FullInterface::tabSelected` shows one
    /// of the synthesis, effects, matrix and master controls interfaces).
    /// The sections below them show on every tab.
    fn show_tab(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layout: &FullLayout,
        live: Option<&Live>,
    ) {
        let top = Scope::top(&self.skin, layout.size_ratio);
        if self.preset_browser_visible {
            let browser_layout = PresetBrowserLayout::new(&top, layout.browser);
            preset_browser::paint_background(pixels, &top, &browser_layout);
            let scope = top.within(preset_browser::SKIN);
            let mut panel = Panel::new(ui, host, pixels, scope, &mut self.overlay, layout.browser);
            let hide = self.preset_browser.show(&mut panel, &browser_layout);
            panel.finish();
            if hide {
                self.preset_browser_visible = false;
            }
            return;
        }
        match self.tab {
            Tab::Voice => {
                self.synthesis
                    .show(ui, host, pixels, &top, &mut self.overlay, layout.main)
            }
            Tab::Effects => self
                .effects
                .show(ui, host, pixels, layout, &top, &mut self.overlay),
            // G4: the modulation matrix.
            Tab::Matrix => {}
            Tab::Advanced => {
                let advanced_layout = AdvancedLayout::new(&top, layout.main);
                let scope = top.within(advanced::SKIN);
                let mut panel = Panel::new(ui, host, pixels, scope, &mut self.overlay, layout.main);
                self.advanced.show(
                    &mut panel,
                    &advanced_layout,
                    live.map(|live| live.oscilloscope.as_slice()),
                );
                panel.finish();
            }
        }
    }

    /// The modulation column and the Bend, Keyboard, Voice and Portamento
    /// sections.
    fn show_sections(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layout: &FullLayout,
    ) {
        let top = Scope::top(&self.skin, layout.size_ratio);
        let browsing = self.preset_browser_visible;

        if !browsing {
            self.modulation
                .show(ui, host, pixels, &top, &mut self.overlay, layout.modulation);
        }

        let extra_layout = ExtraModLayout::new(&top, layout.extra_mod);
        self.extra_mod.paint_background(pixels, &top, &extra_layout);
        let mut panel = Panel::new(
            ui,
            host,
            pixels,
            top.within(extra_mod::MACRO_SKIN),
            &mut self.overlay,
            layout.extra_mod,
        );
        self.extra_mod.show(&mut panel, &extra_layout);
        panel.finish();

        if !browsing {
            let scope = top.within(voice::SKIN);
            let voice_layout = VoiceLayout::new(&scope, layout.voice);
            self.voice.paint_background(pixels, &scope, &voice_layout);
            let mut panel = Panel::new(ui, host, pixels, scope, &mut self.overlay, layout.voice);
            self.voice.show(&mut panel, &voice_layout);
            panel.finish();
        }

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

        if browsing {
            return;
        }
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

    /// The modulations' amount knobs (`ModulationManager`'s children), over
    /// the sections.
    fn show_amount_knobs(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        layout: &FullLayout,
        width: i32,
        height: i32,
    ) {
        let top = &Scope::top(&self.skin, layout.size_ratio);
        let pointer = ui
            .input(|input| input.pointer.hover_pos())
            .map(|pos| ((pos - pixels.area().min) * pixels.pixels_per_point()).to_pos2());
        let editor = Rect::new(0, 0, width, height);
        let mut expansion = crate::layer::Layer::new();
        self.overlay
            .modulation
            .show_expansion(ui, pixels, &mut expansion, top, editor);
        expansion.paint(pixels, editor.into());
        let knobs = self
            .overlay
            .modulation
            .amount_knobs(host, top, pointer, width);
        let scope = top.within(&[Section::ModulationDragDrop]);
        let mut panel = Panel::new(
            ui,
            host,
            pixels,
            scope,
            &mut self.overlay,
            Rect::new(0, 0, width, height),
        );
        for knob in &knobs {
            panel.amount_knob(knob);
        }
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

        self.overlay.show_menu(ui, host, pixels, &top, area);

        show_text_entry(&mut self.overlay, ui, host, pixels, &top);
    }

    /// Where the chat goes for an editor drawn in `area` (points): its
    /// toggle in the logo section (D13, D14), the panel inside the editor
    /// below the header, coloured from the Overlay skin section.
    pub fn chat_placement(&self, area: egui::Rect, pixels_per_point: f32) -> ChatPlacement {
        let width = (area.width() * pixels_per_point).round() as i32;
        let height = (area.height() * pixels_per_point).round() as i32;
        let layout = FullLayout::new(&self.skin, width, height);
        let points = |rect: Rect| {
            egui::Rect::from_min_size(
                area.min + egui::vec2(rect.x as f32, rect.y as f32) / pixels_per_point,
                egui::vec2(rect.width as f32, rect.height as f32) / pixels_per_point,
            )
        };
        let overlay = self.top(&layout).within(&[Section::Overlay]);
        ChatPlacement {
            toggle: points(layout.header.logo),
            editor: points(layout.bounds),
            header_bottom: points(layout.header.bounds).max.y,
            scale: layout.size_ratio / pixels_per_point,
            style: ChatStyle::from_scope(&overlay, pixels_per_point),
        }
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
        if self.preset_browser_visible {
            self.paint_temporary_tab(pixels, layout, "PRESET BROWSER");
        } else {
            self.paint_tabs(pixels, layout);
        }
        self.paint_preset_selector(pixels, layout, model);
    }

    /// The product name in the logo area, sized to fit it.
    fn paint_logo(&self, pixels: &PixelPainter, layout: &FullLayout) {
        let logo = self.top(layout).within(&[Section::Logo, Section::Header]);
        let bounds = layout.header.logo;
        // The name spans the logo button's width (`LogoSection::resized`:
        // a square as tall as the header less 2 * kLogoPaddingY).
        let point_height = bounds.width as f32 * 0.19;
        pixels.text_line(
            PRODUCT_NAME,
            bounds,
            (
                Face::TitleRegular,
                point_height,
                logo.color(ColorId::WidgetPrimary1),
            ),
            Justify::Centred,
            Overflow::Fit,
        );
    }

    /// `HeaderSection::setTemporaryTab`: the temporary tab's name
    /// (`temporary_tab_`, white light text) beside its exit button.
    fn paint_temporary_tab(&self, pixels: &PixelPainter, layout: &FullLayout, name: &str) {
        let header = &layout.header;
        pixels.text_line(
            name,
            header.temporary,
            (Face::Light, header.temporary_text_size, Colour(0xffff_ffff)),
            Justify::Left,
            Overflow::Fit,
        );
    }

    /// The header's `TabSelector`.
    fn paint_tabs(&self, pixels: &PixelPainter, layout: &FullLayout) {
        let header = self.top(layout).within(&[Section::Header]);
        HEADER_TABS.paint(pixels, &header, layout.header.tabs, self.tab.index());
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
        if self.preset_modified.is_modified() {
            name.insert(0, '*');
        }
        pixels.text_line(
            &name,
            preset.text,
            (
                Face::Title,
                preset.text_size,
                header.color(ColorId::PresetText),
            ),
            Justify::Centred,
            Overflow::Fit,
        );
    }

    /// `TabSelector::mouseDown` / `mouseDrag` / `mouseUp`: the tab under the
    /// pointer is selected as soon as the button goes down, and follows a
    /// drag.
    fn handle_tabs(&mut self, ui: &mut Ui, pixels: &PixelPainter, layout: &FullLayout) {
        if self.preset_browser_visible {
            return;
        }
        let mut selected = self.tab.index();
        let id = ui.scope_id().with("tabs");
        if HEADER_TABS.interact(ui, pixels, id, layout.header.tabs, &mut selected) {
            self.tab = Tab::ALL[selected];
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
