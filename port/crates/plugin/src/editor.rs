// SPDX-License-Identifier: GPL-3.0-or-later

//! What the `sas-gui` editor reads from the plugin.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use nice_plug::context::gui::GuiContext;
use sas_common::json;
use sas_common::line_generator::LineGenerator;
use sas_engine::document::PresetDocument;
use sas_engine::keyboard::KeyboardHandle;
use sas_engine::sas_oscillators::sas_wavetable::creator::{AudioFileLoadStyle, WavetableCreator};
use sas_engine::sas_oscillators::sas_wavetable::Wavetable;
use sas_engine::telemetry::Telemetry;
use sas_engine::Tuning;
use sas_gui::{ChatHost, EditorModel, HostParam, Route};
use sas_preset::user_folders::Kind;
use sas_preset::{GuiConnection, InitContent, PresetState, SampleState, SaveInfo, SlotValues};

use crate::audio_file;
use crate::chat::PluginChat;
use crate::midi_learn::MidiLearn;
use crate::params::SynthParams;
use crate::resynthesis::{Job, Resynthesizer};
use crate::state;

/// The editor's view of one plugin instance.
pub struct PluginModel {
    params: Arc<SynthParams>,
    keyboard: KeyboardHandle,
    telemetry: Telemetry,
    midi_learn: Arc<MidiLearn>,
    /// The preset's save info (name, author, comments) and the edit bus
    /// revision it was read at, so the document mirror is read only after it
    /// changes.
    info: Mutex<(u64, SaveInfo)>,
    /// The connected modulation slots, read at an edit bus revision too.
    modulations: Mutex<(u64, Arc<[Route]>)>,
    chat: Option<Arc<PluginChat>>,
    /// The frequency display setting, read from the config file once.
    hz_frequency: AtomicBool,
    /// The config's data folder, read once.
    data_directory: Mutex<Option<PathBuf>>,
    /// `SynthBase::active_file_`.
    active_file: Mutex<Option<PathBuf>>,
    /// The host connection, once the editor has opened.
    gui: Mutex<Option<GuiContext>>,
    resynthesizer: Resynthesizer,
}

impl PluginModel {
    pub fn new(
        params: Arc<SynthParams>,
        telemetry: Telemetry,
        keyboard: KeyboardHandle,
        midi_learn: Arc<MidiLearn>,
        chat: Option<Arc<PluginChat>>,
    ) -> PluginModel {
        let edits = params.edit_handle();
        let revision = edits.revision();
        let info = edits.with_document(|document| document.state.info.clone());
        let hz_frequency = midi_learn
            .config_file()
            .is_some_and(crate::config::display_hz_frequency);
        let modulations = edits.with_document(routes);
        let data_directory = midi_learn
            .config_file()
            .and_then(crate::config::data_directory);
        let resynthesizer = Resynthesizer::new(edits.clone());
        PluginModel {
            params,
            keyboard,
            telemetry,
            midi_learn,
            info: Mutex::new((revision, info)),
            modulations: Mutex::new((revision, modulations)),
            chat,
            hz_frequency: AtomicBool::new(hz_frequency),
            data_directory: Mutex::new(data_directory),
            active_file: Mutex::new(None),
            gui: Mutex::new(None),
            resynthesizer,
        }
    }

    /// Waits until every resynthesis started has loaded its wavetable
    /// (tests).
    pub fn wait_for_resynthesis(&self) {
        self.resynthesizer.wait_idle();
    }
}

/// The document's connected modulation slots.
fn routes(document: &PresetDocument) -> Arc<[Route]> {
    let state = &document.state;
    state
        .connection_order()
        .iter()
        .map(|&slot| {
            let connection = &state.modulations()[slot];
            Route {
                slot,
                source: connection.source.clone(),
                destination: connection.destination.clone(),
            }
        })
        .collect()
}

impl PluginModel {
    /// The preset's save info, read from the edit bus once per revision.
    fn with_info<T>(&self, read: impl FnOnce(&SaveInfo) -> T) -> T {
        let edits = self.params.edit_handle();
        let revision = edits.revision();
        let mut cached = lock(&self.info);
        if cached.0 != revision {
            *cached = (
                revision,
                edits.with_document(|document| document.state.info.clone()),
            );
        }
        read(&cached.1)
    }
}

impl EditorModel for PluginModel {
    fn hz_frequency_display(&self) -> bool {
        self.hz_frequency.load(Ordering::Relaxed)
    }

    fn set_hz_frequency_display(&self, hz_frequency: bool) {
        self.hz_frequency.store(hz_frequency, Ordering::Relaxed);
        if let Some(path) = self.midi_learn.config_file() {
            let _ = crate::config::save_display_hz_frequency(path, hz_frequency);
        }
    }

    fn preset_name(&self) -> String {
        self.with_info(|info| info.preset_name.clone())
    }

    fn preset_author(&self) -> String {
        self.with_info(|info| info.author.clone())
    }

    fn preset_comments(&self) -> String {
        self.with_info(|info| info.comments.clone())
    }

    fn preset_style(&self) -> String {
        self.with_info(|info| info.style.clone())
    }

    fn set_preset_info(&self, author: &str, comments: &str, style: &str) {
        let edits = self.params.edit_handle();
        let mut info = edits.with_document(|document| document.state.info.clone());
        info.author = author.to_string();
        info.comments = comments.to_string();
        info.style = style.to_string();
        edits.set_info(info);
    }

    fn default_author(&self) -> String {
        self.midi_learn
            .config_file()
            .map(crate::config::author)
            .unwrap_or_default()
    }

    fn save_default_author(&self, author: &str) {
        if let Some(path) = self.midi_learn.config_file() {
            let _ = crate::config::save_author(path, author);
        }
    }

    fn telemetry(&self) -> Option<Telemetry> {
        Some(self.telemetry.clone())
    }

    fn param(&self, name: &str) -> Option<HostParam> {
        self.params
            .control(name)
            .map(|control| control.param.as_ptr())
    }

    fn note_on(&self, note: u8, velocity: f32) {
        self.keyboard.note_on(note, velocity);
    }

    fn note_off(&self, note: u8) {
        self.keyboard.note_off(note);
    }

    fn held_notes(&self) -> u128 {
        self.keyboard.held_notes()
    }

    fn lfo_shape(&self, index: usize) -> Option<LineGenerator> {
        self.params
            .edit_handle()
            .with_document(|document| document.state.lfos.get(index).cloned())
    }

    fn set_lfo_shape(&self, index: usize, line: LineGenerator) -> bool {
        self.params.edit_handle().set_lfo_shape(index, line).is_ok()
    }

    fn macro_name(&self, index: usize) -> String {
        self.params
            .edit_handle()
            .with_document(|document| document.state.info.macro_name(index))
    }

    fn modulations(&self) -> Arc<[Route]> {
        let edits = self.params.edit_handle();
        let revision = edits.revision();
        let mut cached = self
            .modulations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cached.0 != revision {
            *cached = (revision, edits.with_document(routes));
        }
        cached.1.clone()
    }

    fn connect_modulation(
        &self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Option<GuiConnection> {
        self.params
            .edit_handle()
            .connect_modulation(source, destination, values)
            .ok()
    }

    fn set_connection_values(&self, source: &str, destination: &str, values: SlotValues) -> bool {
        self.params
            .edit_handle()
            .set_connection_values(source, destination, values)
            .is_ok()
    }

    fn edit_generation(&self) -> u64 {
        self.params.edit_handle().generation()
    }

    fn disconnect_modulation(&self, source: &str, destination: &str) -> Option<usize> {
        self.params
            .edit_handle()
            .disconnect_modulation(source, destination)
            .ok()
            .flatten()
    }

    fn tuning_name(&self) -> String {
        self.params
            .edit_handle()
            .with_document(|document| document.tuning.name())
    }

    fn tuning(&self) -> Tuning {
        self.params
            .edit_handle()
            .with_document(|document| document.tuning.clone())
    }

    fn wavetable(&self, oscillator: usize) -> Option<Arc<Wavetable>> {
        self.params.edit_handle().wavetable(oscillator)
    }

    fn wavetable_name(&self, oscillator: usize) -> String {
        self.params.edit_handle().with_document(|document| {
            document
                .state
                .wavetables
                .get(oscillator)
                .map(|creator| creator.name.clone())
                .unwrap_or_default()
        })
    }

    fn wavetable_creator(&self, oscillator: usize) -> Option<WavetableCreator> {
        self.params
            .edit_handle()
            .with_document(|document| document.state.wavetables.get(oscillator).cloned())
    }

    fn wavetable_json(&self, oscillator: usize) -> Option<String> {
        self.params.edit_handle().with_document(|document| {
            document
                .state
                .wavetables
                .get(oscillator)
                .map(|creator| creator.to_json().dump())
        })
    }

    fn load_wavetable_json(&self, oscillator: usize, json: &str) -> bool {
        let Ok(data) = json::parse(json) else {
            return false;
        };
        if !WavetableCreator::is_valid_json(&data) {
            return false;
        }
        let edits = self.params.edit_handle();
        let Some(mut creator) =
            edits.with_document(|document| document.state.wavetables.get(oscillator).cloned())
        else {
            return false;
        };
        if creator.read_json(&data).is_err() {
            return false;
        }
        edits.set_wavetable(oscillator, creator).is_ok()
    }

    fn load_default_wavetable(&self, oscillator: usize) {
        let edits = self.params.edit_handle();
        let Some(mut creator) =
            edits.with_document(|document| document.state.wavetables.get(oscillator).cloned())
        else {
            return;
        };
        creator.init();
        // A full queue drops the reset, like the other editor edits.
        let _ = edits.set_wavetable(oscillator, creator);
    }

    fn resynthesize_to_wavetable(&self, oscillator: usize) {
        let state = self.params.preset_state();
        if oscillator >= state.wavetables.len() {
            return;
        }
        self.resynthesizer.start(Job {
            oscillator,
            state,
            sample_rate: self.params.sample_rate().round() as i32,
            bpm: self.params.host_tempo(),
        });
    }

    fn audio_file_extensions(&self) -> &'static [&'static str] {
        audio_file::AUDIO_EXTENSIONS
    }

    fn load_sample_file(&self, path: &Path) -> bool {
        /// `SampleSection::loadFile`'s `kMaxFileSamples`.
        const MAX_FILE_SAMPLES: usize = 17_640_000;
        /// `kMaxSize` of the mono `Sample::loadSample`; the stereo one has
        /// no cap.
        const MAX_MONO_SAMPLES: usize = 1_764_000;
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(audio) = audio_file::read_audio_channels(&bytes, &extension, 2, MAX_FILE_SAMPLES)
        else {
            return false;
        };
        let mut channels = audio.channels.into_iter();
        let Some(mut left) = channels.next() else {
            return false;
        };
        let right = channels.next();
        if right.is_none() {
            left.truncate(MAX_MONO_SAMPLES);
        }
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let sample = SampleState {
            name,
            sample_rate: audio.sample_rate,
            left,
            right,
        };
        self.params.edit_handle().set_sample(sample).is_ok()
    }

    fn load_audio_as_wavetable(
        &self,
        oscillator: usize,
        path: &Path,
        style: AudioFileLoadStyle,
    ) -> bool {
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(audio) = audio_file::read_audio(&bytes, &extension) else {
            return false;
        };
        let text = audio_file::wavetable_data_string(&bytes);
        let edits = self.params.edit_handle();
        let Some(mut creator) =
            edits.with_document(|document| document.state.wavetables.get(oscillator).cloned())
        else {
            return false;
        };
        creator.clear();
        let fade_style = audio_file::fade_style_from_wavetable_string(&text);
        if creator
            .init_from_audio_file(&audio.first_channel, audio.sample_rate, style, fade_style)
            .is_err()
        {
            return false;
        }
        creator.name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        creator.author = audio_file::author_from_wavetable_string(&text);
        edits.set_wavetable(oscillator, creator).is_ok()
    }

    fn with_sample(&self, f: &mut dyn FnMut(&SampleState)) {
        self.params
            .edit_handle()
            .with_document(|document| f(&document.state.sample));
    }

    fn sample_name(&self) -> String {
        self.params
            .edit_handle()
            .with_document(|document| document.state.sample.name.clone())
    }

    fn content_revision(&self) -> u64 {
        self.params.edit_handle().revision()
    }

    fn load_tuning(&self, tuning: Tuning) {
        // A full queue drops the load: the queue only fills while the audio
        // thread is stalled, and the name shown stays the old one.
        let _ = self.params.edit_handle().set_tuning(tuning);
    }

    fn data_directory(&self) -> Option<PathBuf> {
        lock(&self.data_directory).clone()
    }

    fn set_data_directory(&self, folder: PathBuf) {
        if let Some(path) = self.midi_learn.config_file() {
            let _ = crate::config::save_data_directory(path, &folder);
        }
        *lock(&self.data_directory) = Some(folder);
    }

    fn favourites(&self) -> Vec<PathBuf> {
        self.midi_learn
            .config_file()
            .map(|config| crate::config::favourites(&crate::config::favourites_file(config)))
            .unwrap_or_default()
    }

    fn additional_folders(&self, kind: Kind) -> Vec<PathBuf> {
        match (self.midi_learn.config_file(), additional_folders_name(kind)) {
            (Some(config), Some(name)) => crate::config::additional_folders(config, name),
            _ => Vec::new(),
        }
    }

    fn set_additional_folders(&self, kind: Kind, folders: &[PathBuf]) {
        if let (Some(config), Some(name)) =
            (self.midi_learn.config_file(), additional_folders_name(kind))
        {
            let _ = crate::config::save_additional_folders(config, name, folders);
        }
    }

    fn set_favourite(&self, path: &Path, favourite: bool) {
        if let Some(config) = self.midi_learn.config_file() {
            let _ = crate::config::save_favourite(
                &crate::config::favourites_file(config),
                path,
                favourite,
            );
        }
    }

    fn active_preset_file(&self) -> Option<PathBuf> {
        lock(&self.active_file).clone()
    }

    fn load_preset_file(&self, path: &Path) -> Result<(), String> {
        // SynthBase::loadFromFile's reasons: none for a missing file.
        if !path.exists() {
            return Err(String::new());
        }
        let state = state::plugin_state_from_file(path).map_err(|error| match error {
            sas_preset::Error::NewerVersion(_) => {
                "Preset was created with a newer version.".to_string()
            }
            _ => "Preset file is corrupted.".to_string(),
        })?;
        let gui = lock(&self.gui)
            .clone()
            .ok_or("the editor isn't connected to the host")?;
        gui.set_state(state);
        *lock(&self.active_file) = Some(path.to_path_buf());
        Ok(())
    }

    fn save_preset_file(&self, path: &Path) -> Result<PathBuf, String> {
        let written = state::save_preset_file(&self.params.preset_state(), path)
            .map_err(|error| error.to_string())?;
        // setPresetName: the synth takes the file's name.
        let edits = self.params.edit_handle();
        let mut info = edits.with_document(|document| document.state.info.clone());
        info.preset_name = written
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        edits.set_info(info);
        *lock(&self.active_file) = Some(written.clone());
        Ok(written)
    }

    fn init_preset(&self) {
        let init = PresetState::new(&InitContent { sample_seed: 0 });
        if let (Some(gui), Ok(state)) = (
            lock(&self.gui).clone(),
            state::plugin_state_from_json(&init.to_json_string()),
        ) {
            gui.set_state(state);
            *lock(&self.active_file) = None;
        }
    }

    fn preset_generation(&self) -> u64 {
        self.params.edit_handle().generation()
    }

    fn arm_midi_learn(&self, name: &str) {
        self.midi_learn.arm(name);
    }

    fn cancel_midi_learn(&self) {
        self.midi_learn.cancel();
    }

    fn clear_midi_learn(&self, name: &str) {
        self.midi_learn.clear(name);
    }

    fn is_midi_mapped(&self, name: &str) -> bool {
        self.midi_learn.is_midi_mapped(name)
    }

    fn midi_learn_ccs(&self, name: &str) -> Vec<i32> {
        self.midi_learn.learned_ccs(name)
    }

    fn set_editor_open(&self, open: bool) {
        self.params.set_editor_open(open);
    }

    fn drain_midi_moved(&self, f: &mut dyn FnMut(&'static str)) {
        self.params.drain_midi_moved(f);
    }

    fn midi_move_forwarded(&self, name: &str, normalized: f32) {
        self.params.record_forwarded(name, normalized);
    }

    fn attach(&self, gui: GuiContext, wake: Box<dyn Fn() + Send + Sync>) {
        *lock(&self.gui) = Some(gui.clone());
        if let Some(chat) = &self.chat {
            chat.attach(gui, Arc::from(wake));
        }
    }

    fn chat(&self) -> Option<&dyn ChatHost> {
        self.chat.as_deref().map(|chat| chat as &dyn ChatHost)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The config key holding the folders added to `kind`'s browser
/// (`LoadSave::kAdditionalWavetableFoldersName`, `kAdditionalSampleFoldersName`).
fn additional_folders_name(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Wavetables => Some(crate::config::WAVETABLE_FOLDERS),
        Kind::Samples => Some(crate::config::SAMPLE_FOLDERS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_engine::document::PresetDocument;
    use sas_engine::edit_bus::EditBus;
    use sas_engine::keyboard::keyboard;
    use sas_preset::InitContent;

    #[test]
    fn a_preset_that_doesnt_load_gives_upstreams_reason() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params,
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        let folder = std::env::temp_dir().join(format!("sas-load-errors-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let corrupted = folder.join("corrupted.preset");
        std::fs::write(&corrupted, "{ not json").unwrap();
        let newer = folder.join("newer.preset");
        std::fs::write(&newer, r#"{"synth_version": "99.0.0", "settings": {}}"#).unwrap();
        assert_eq!(
            model.load_preset_file(&folder.join("missing.preset")),
            Err(String::new())
        );
        assert_eq!(
            model.load_preset_file(&corrupted),
            Err("Preset file is corrupted.".to_string())
        );
        assert_eq!(
            model.load_preset_file(&newer),
            Err("Preset was created with a newer version.".to_string())
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// The save section's details reach the saved file and the synth
    /// (`setAuthor`, `setComments`, `setStyle`, then `saveToFile`).
    #[test]
    fn a_saved_preset_carries_its_author_comments_and_style() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params,
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        let folder = std::env::temp_dir().join(format!("sas-save-info-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        model.set_preset_info("Someone", "Warm and slow", "Pad");
        let written = model.save_preset_file(&folder.join("Saved.vital")).unwrap();
        let data = sas_common::json::parse(&std::fs::read_to_string(&written).unwrap()).unwrap();
        assert_eq!(data.get("author").unwrap().to_str().unwrap(), "Someone");
        assert_eq!(
            data.get("comments").unwrap().to_str().unwrap(),
            "Warm and slow"
        );
        assert_eq!(data.get("preset_style").unwrap().to_str().unwrap(), "Pad");
        assert_eq!(model.preset_style(), "Pad");
        assert_eq!(model.preset_name(), "Saved");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    /// A WAV file of 8-bit samples, `channels` interleaved, `frames` long.
    fn wav_8bit(channels: u16, frames: usize, value: u8) -> Vec<u8> {
        let data_length = frames * channels as usize;
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&((36 + data_length) as u32).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16u32.to_le_bytes());
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&channels.to_le_bytes());
        file.extend_from_slice(&44_100u32.to_le_bytes());
        file.extend_from_slice(&(44_100 * channels as u32).to_le_bytes());
        file.extend_from_slice(&channels.to_le_bytes());
        file.extend_from_slice(&8u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&(data_length as u32).to_le_bytes());
        file.resize(file.len() + data_length, value);
        file
    }

    /// `SampleSection::loadFile`: a stereo file keeps both channels and its
    /// length (the stereo `loadSample` has no cap); a mono file stops at
    /// `kMaxSize`; both are named after the file.
    #[test]
    fn a_sample_file_loads_as_upstream_loads_it() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params,
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        let folder = std::env::temp_dir().join(format!("sas-sample-files-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let long = 1_764_000 + 10;

        let stereo = folder.join("Stereo Loop.wav");
        std::fs::write(&stereo, wav_8bit(2, long, 192)).unwrap();
        assert!(model.load_sample_file(&stereo));
        assert_eq!(model.sample_name(), "Stereo Loop");
        let mut lengths = (0, None);
        model.with_sample(&mut |sample| {
            lengths = (sample.left.len(), sample.right.as_ref().map(Vec::len));
            assert_eq!(sample.left[0], 0.5);
        });
        assert_eq!(lengths, (long, Some(long)));

        let mono = folder.join("Mono Hit.wav");
        std::fs::write(&mono, wav_8bit(1, long, 64)).unwrap();
        assert!(model.load_sample_file(&mono));
        assert_eq!(model.sample_name(), "Mono Hit");
        model.with_sample(&mut |sample| {
            lengths = (sample.left.len(), sample.right.as_ref().map(Vec::len));
            assert_eq!(sample.left[0], -0.5);
        });
        assert_eq!(lengths, (1_764_000, None));

        assert!(!model.load_sample_file(&folder.join("missing.wav")));
        assert_eq!(model.sample_name(), "Mono Hit");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_preset_name_follows_renames() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params.clone(),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        assert_eq!(model.preset_name(), "");
        let edits = params.edit_handle();
        let mut info = edits.with_document(|document| document.state.info.clone());
        info.preset_name = "Renamed".to_string();
        edits.set_info(info);
        assert_eq!(model.preset_name(), "Renamed");
    }

    /// The wavetable editor reads a copy of the document's creator, which
    /// follows the document's edits.
    #[test]
    fn the_wavetable_creator_is_the_documents() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params.clone(),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        let creator = model.wavetable_creator(1).unwrap();
        assert_eq!(creator.to_json().dump(), model.wavetable_json(1).unwrap());
        let mut renamed = creator.clone();
        renamed.name = "Renamed".to_string();
        params.edit_handle().set_wavetable(1, renamed).unwrap();
        assert_eq!(model.wavetable_creator(1).unwrap().name, "Renamed");
        assert!(model.wavetable_creator(3).is_none());
    }

    #[test]
    fn a_loaded_tuning_names_the_tuning() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            Arc::new(SynthParams::new(edits)),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        let default_name = model.tuning_name();
        let mut tuning = Tuning::new();
        tuning.set_name("Septimal");
        model.load_tuning(tuning);
        assert_eq!(model.tuning_name(), "Septimal");
        assert_ne!(default_name, "Septimal");
    }

    /// `SynthBase::loadTuningFile` loads into the playing tuning, so a
    /// keyboard map loaded after a scale keeps the scale.
    #[test]
    fn a_keyboard_map_layers_on_the_loaded_scale() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            Arc::new(SynthParams::new(edits)),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        let tunings = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../migration/corpus/tunings");
        let scale = tunings.join("corpus_septimal.scl");
        let map = tunings.join("corpus_septimal.kbm");

        let mut tuning = model.tuning();
        tuning.load_file(&scale).unwrap();
        model.load_tuning(tuning);
        let mut tuning = model.tuning();
        tuning.load_file(&map).unwrap();
        model.load_tuning(tuning);

        let mut expected = Tuning::new();
        expected.load_file(&scale).unwrap();
        expected.load_file(&map).unwrap();
        assert_eq!(model.tuning().state_to_json(), expected.state_to_json());
        assert_eq!(model.tuning_name(), expected.name());

        let mut map_only = Tuning::new();
        map_only.load_file(&map).unwrap();
        assert_ne!(model.tuning().state_to_json(), map_only.state_to_json());
        let mut scale_only = Tuning::new();
        scale_only.load_file(&scale).unwrap();
        assert_ne!(model.tuning().state_to_json(), scale_only.state_to_json());
    }

    #[test]
    fn keyboard_notes_reach_the_keyboard() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let (handle, mut consumer) = keyboard();
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            Arc::new(SynthParams::new(edits)),
            telemetry,
            handle,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        model.note_on(60, 0.5);
        assert_eq!(model.held_notes(), 1 << 60);
        model.note_off(60);
        assert_eq!(model.held_notes(), 0);
        consumer.collect();
        consumer.schedule(64);
        assert_eq!(consumer.pop(), Some((0, [0x90, 60, 64])));
        assert_eq!(
            consumer.pop().map(|(_, message)| message),
            Some([0x80, 60, 0])
        );
    }

    #[test]
    fn wavetables_and_the_sample_follow_edits() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params.clone(),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        assert_eq!(model.wavetable_name(0), "Init");
        let init = model.wavetable(0).unwrap();
        assert!(model.wavetable(3).is_none());
        assert_eq!(model.wavetable_name(3), "");
        // Shared, not rendered again, until it changes.
        assert!(Arc::ptr_eq(&init, &model.wavetable(0).unwrap()));

        let revision = model.content_revision();
        let edits = params.edit_handle();
        let mut creator = edits.with_document(|document| document.state.wavetables[1].clone());
        creator.name = "Saw Stack".to_string();
        edits.set_wavetable(1, creator).unwrap();
        assert_eq!(model.wavetable_name(1), "Saw Stack");
        assert_eq!(model.wavetable_name(0), "Init");
        assert!(model.content_revision() > revision);

        let revision = model.content_revision();
        let sample = SampleState {
            name: "Breath".to_string(),
            sample_rate: 44100,
            left: vec![0.0, 0.5, -0.25, 1.0],
            right: None,
        };
        edits.set_sample(sample).unwrap();
        assert_eq!(model.sample_name(), "Breath");
        let mut left = Vec::new();
        model.with_sample(&mut |sample| left.extend_from_slice(&sample.left));
        assert_eq!(left, [0.0, 0.5, -0.25, 1.0]);
        assert!(model.content_revision() > revision);
    }

    #[test]
    fn wavetables_copy_paste_and_initialize() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params.clone(),
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        let edits = params.edit_handle();
        let mut creator = edits.with_document(|document| document.state.wavetables[0].clone());
        creator.name = "Copied".to_string();
        edits.set_wavetable(0, creator).unwrap();

        // Copy oscillator 1, paste into oscillator 2.
        let json = model.wavetable_json(0).unwrap();
        assert!(model.wavetable_json(3).is_none());
        let revision = model.content_revision();
        assert!(model.load_wavetable_json(1, &json));
        assert_eq!(model.wavetable_name(1), "Copied");
        assert_eq!(model.wavetable_json(1).unwrap(), json);
        assert!(model.content_revision() > revision);

        // Text that isn't wavetable JSON changes nothing.
        let revision = model.content_revision();
        assert!(!model.load_wavetable_json(1, "not json"));
        assert!(!model.load_wavetable_json(1, r#"{"name": "No groups"}"#));
        assert!(!model.load_wavetable_json(7, &json));
        assert_eq!(model.content_revision(), revision);

        // Valid by `isValidJson` but unreadable (a group that isn't an
        // object): upstream leaves the creator cleared and partly loaded,
        // the port leaves the oscillator as it was (D21).
        let unreadable = r#"{"version": "1.0.0", "name": "Broken", "groups": [7]}"#;
        assert!(!model.load_wavetable_json(1, unreadable));
        assert_eq!(model.wavetable_json(1).unwrap(), json);
        assert_eq!(model.wavetable_name(1), "Copied");
        assert_eq!(model.content_revision(), revision);

        // Initialize: the default table again.
        model.load_default_wavetable(1);
        assert_eq!(model.wavetable_name(1), "Init");
        assert_eq!(model.wavetable_name(0), "Copied");
    }

    /// A mono 16-bit WAV of a 441 Hz sine with a `clm ` chunk.
    fn sine_wav(clm: &str) -> Vec<u8> {
        let mut data = Vec::new();
        for i in 0..44_100 {
            let phase = std::f32::consts::TAU * i as f32 / 100.0;
            data.extend_from_slice(&((phase.sin() * 16_000.0) as i16).to_le_bytes());
        }
        let mut fmt = Vec::new();
        for field in [1u16, 1] {
            fmt.extend_from_slice(&field.to_le_bytes());
        }
        fmt.extend_from_slice(&44_100u32.to_le_bytes());
        fmt.extend_from_slice(&88_200u32.to_le_bytes());
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&16u16.to_le_bytes());
        let mut body = b"WAVE".to_vec();
        for (label, chunk) in [
            (b"fmt ", fmt),
            (b"clm ", clm.as_bytes().to_vec()),
            (b"data", data),
        ] {
            body.extend_from_slice(label);
            body.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
            body.extend_from_slice(&chunk);
        }
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&(body.len() as u32).to_le_bytes());
        file.extend_from_slice(&body);
        file
    }

    #[test]
    fn a_dropped_wav_becomes_the_oscillators_wavetable() {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        let params = Arc::new(SynthParams::new(edits));
        let (_writer, telemetry) = Telemetry::new(Arc::default());
        let model = PluginModel::new(
            params,
            telemetry,
            keyboard().0,
            Arc::new(MidiLearn::in_memory()),
            None,
        );
        assert!(model.audio_file_extensions().contains(&"wav"));
        let dir = std::env::temp_dir();
        let path = dir.join(format!("Sine Drop {}.wav", std::process::id()));
        std::fs::write(&path, sine_wav("<!>2048 10000000 wavetable [Someone]")).unwrap();

        for style in [
            AudioFileLoadStyle::WavetableSplice,
            AudioFileLoadStyle::Vocoded,
            AudioFileLoadStyle::Pitched,
        ] {
            let revision = model.content_revision();
            assert!(model.load_audio_as_wavetable(1, &path, style), "{style:?}");
            assert_eq!(
                model.wavetable_name(1),
                format!("Sine Drop {}", std::process::id())
            );
            assert!(model.content_revision() > revision);
            let json = model.wavetable_json(1).unwrap();
            assert!(json.contains("\"author\":\"Someone\""), "{json}");
        }
        assert_eq!(model.wavetable_name(0), "Init");

        // A file that isn't WAV, or isn't there, changes nothing.
        let revision = model.content_revision();
        let text = dir.join(format!("not-audio-{}.wav", std::process::id()));
        std::fs::write(&text, "words").unwrap();
        assert!(!model.load_audio_as_wavetable(1, &text, AudioFileLoadStyle::Pitched));
        assert!(!model.load_audio_as_wavetable(
            1,
            &dir.join("missing.wav"),
            AudioFileLoadStyle::Pitched
        ));
        assert!(!model.load_audio_as_wavetable(9, &path, AudioFileLoadStyle::Pitched));
        // An empty or corrupt file of every offered type, lossy ones included.
        let corrupt: Vec<u8> = (0..4096u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
            .collect();
        for extension in model.audio_file_extensions() {
            for (what, bytes) in [("empty", &[][..]), ("corrupt", &corrupt[..])] {
                let file = dir.join(format!("{what}-{}.{extension}", std::process::id()));
                std::fs::write(&file, bytes).unwrap();
                assert!(
                    !model.load_audio_as_wavetable(1, &file, AudioFileLoadStyle::Pitched),
                    "{what} .{extension}"
                );
                std::fs::remove_file(file).unwrap();
            }
        }
        assert_eq!(model.content_revision(), revision);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(text).unwrap();
    }
}
