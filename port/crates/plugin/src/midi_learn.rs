// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI learn across the plugin's threads: `SynthBase::armMidiLearn`,
//! `cancelMidiLearn`, `clearMidiLearn` and `isMidiMapped` for the editor, and
//! the user config the learn map is saved to (`LoadSave::saveMidiMapConfig`).
//!
//! The engine's `MidiManager` owns the map and learns on the audio thread.
//! The editor's requests are queued here and applied at the start of the
//! next block; the editor reads a copy of the map the audio thread publishes
//! after each change. Upstream writes the config file where the map changes,
//! on the audio thread when a controller is learned; here the audio thread
//! asks for a background task instead ([`MidiLearn::save_config`]), which
//! writes the same contents.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, TryLockError};

use sas_engine::midi_manager::MidiLearnMap;
use sas_preset::lookup;

use crate::config;
use crate::engine::Engine;

/// An editor request, applied to the engine on the audio thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Arm(&'static str),
    Cancel,
    Clear(&'static str),
}

/// The editor's queue into the engine, and the published learn map.
pub struct MidiLearn {
    commands: Mutex<Vec<Command>>,
    map: Mutex<MidiLearnMap>,
    /// Where the map is saved; `None` keeps it in memory only.
    config_file: Option<PathBuf>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn try_lock<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match mutex.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

impl MidiLearn {
    /// No saved map, and nothing saved.
    pub fn in_memory() -> MidiLearn {
        MidiLearn {
            commands: Mutex::new(Vec::with_capacity(16)),
            map: Mutex::new(MidiLearnMap::new()),
            config_file: None,
        }
    }

    /// The map saved in `config_file` (`LoadSave::loadConfig`), saved back
    /// there on every change.
    pub fn with_config_file(config_file: PathBuf) -> MidiLearn {
        MidiLearn {
            map: Mutex::new(config::load_midi_map_config(&config_file)),
            config_file: Some(config_file),
            ..MidiLearn::in_memory()
        }
    }

    /// The user config file, if there is one.
    pub fn config_file(&self) -> Option<&std::path::Path> {
        self.config_file.as_deref()
    }

    /// The map as last published (initially the loaded one).
    pub fn map(&self) -> MidiLearnMap {
        lock(&self.map).clone()
    }

    /// `armMidiLearn(name)`: the next MIDI controller the engine receives
    /// learns `name`. Unknown names are ignored.
    pub fn arm(&self, name: &str) {
        if let Some(details) = lookup(name) {
            lock(&self.commands).push(Command::Arm(details.name.as_str()));
        }
    }

    /// `cancelMidiLearn()`.
    pub fn cancel(&self) {
        lock(&self.commands).push(Command::Cancel);
    }

    /// `clearMidiLearn(name)`: forgets every controller learned for `name`
    /// and, if there was one, saves the config now, as upstream does on the
    /// calling (editor) thread.
    pub fn clear(&self, name: &str) {
        let Some(details) = lookup(name) else {
            return;
        };
        let name = details.name.as_str();
        lock(&self.commands).push(Command::Clear(name));
        let mut map = lock(&self.map);
        let mut cleared = false;
        for controls in map.values_mut() {
            cleared |= controls.remove(name).is_some();
        }
        if cleared {
            self.write_config(&map);
        }
    }

    /// `isMidiMapped(name)`.
    pub fn is_midi_mapped(&self, name: &str) -> bool {
        lock(&self.map)
            .values()
            .any(|controls| controls.contains_key(name))
    }

    /// The controllers `name` is learned on, ascending.
    pub fn learned_ccs(&self, name: &str) -> Vec<i32> {
        lock(&self.map)
            .iter()
            .filter(|(_, controls)| controls.contains_key(name))
            .map(|(cc, _)| *cc)
            .collect()
    }

    /// Audio thread, before the block's MIDI: applies the editor's queued
    /// requests. Skipped if the editor holds the queue; they wait a block.
    /// Returns whether the engine's map changed.
    pub(crate) fn apply_commands<E: Engine>(&self, engine: &mut E) -> bool {
        let Some(mut commands) = try_lock(&self.commands) else {
            return false;
        };
        let mut changed = false;
        for command in commands.drain(..) {
            match command {
                Command::Arm(name) => engine.arm_midi_learn(name),
                Command::Cancel => engine.cancel_midi_learn(),
                Command::Clear(name) => changed |= engine.clear_midi_learn(name),
            }
        }
        changed
    }

    /// Audio thread, after the engine's map changed: publishes it. `None` if
    /// the editor holds the published map (the caller tries again next
    /// block), else whether the published map changed and so needs saving.
    pub(crate) fn publish<E: Engine>(&self, engine: &E) -> Option<bool> {
        let engine_map = engine.midi_learn_map()?;
        let mut map = try_lock(&self.map)?;
        if *map == *engine_map {
            return Some(false);
        }
        map.clone_from(engine_map);
        Some(true)
    }

    /// Writes the published map to the config file (the background task the
    /// audio thread requests after [`publish`](Self::publish)).
    pub fn save_config(&self) {
        let map = lock(&self.map);
        self.write_config(&map);
    }

    fn write_config(&self, map: &MidiLearnMap) {
        if let Some(path) = &self.config_file {
            // Upstream ignores a failed write too (`File::replaceWithText`).
            let _ = config::save_midi_map_config(path, map);
        }
    }
}

impl Default for MidiLearn {
    fn default() -> Self {
        MidiLearn::in_memory()
    }
}
