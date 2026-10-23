// SPDX-License-Identifier: GPL-3.0-or-later

//! The edit bus: how the editor and the chat worker change the synth while
//! it plays, without reloading the whole preset.
//!
//! Upstream's editor changes the `SynthBase` it shares with the audio
//! thread: `SynthGuiInterface::connectModulation` queues a modulation change
//! that `processModulationChanges` applies before the next block, the LFO
//! and remap-curve editors edit a `LineGenerator` in place and re-render it,
//! and a wavetable or sample is loaded into the engine's own object. Here the
//! [`Synth`] belongs to the audio thread, so those edits travel as
//! prepared values:
//!
//! - one bounded, lock-free, multi-producer queue ([`EDIT_QUEUE_CAPACITY`]
//!   entries), drained at the start of each audio block, in order;
//! - shapes, wavetables and samples are built on the producer's thread
//!   (a wavetable is rendered there) and swapped into the engine, and the
//!   replaced values travel back on a return queue, so the audio thread
//!   never allocates or frees;
//! - one pending slot per shape target (8 LFOs, 64 remap curves, 3
//!   wavetables, the sample), newest wins: a push replaces a payload the
//!   audio thread has not taken yet, and a heap-free marker in the queue
//!   keeps it ordered against connections;
//! - a generation tag on every entry: a host state restore or preset load
//!   ([`EditHandle::begin_restore`]) bumps it, and entries queued before it
//!   are dropped;
//! - the producers' mirror of the [`PresetDocument`], updated with each edit,
//!   so the plugin saves the edited state without a reload. The audio thread
//!   never takes its lock.
//!
//! Controls are host parameters and don't travel here, with one exception:
//! the power, amount, bipolar, stereo and bypass a new modulation slot takes
//! travel with its connection, because upstream writes them to the engine
//! right away. They are host parameters as well, and the plugin applies the
//! bus before the block's parameter changes, so a host change in the same
//! block wins.

use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crossbeam_queue::ArrayQueue;
use sas_common::LineGenerator;
use sas_dsp_core::constants::MAX_MODULATION_CONNECTIONS;
use sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_oscillators::sas_wavetable::{Wavetable, NUM_OSCILLATOR_WAVE_FRAMES};
use sas_oscillators::Sample;
use sas_preset::{GuiConnection, SampleState, SaveInfo, SlotValues};

use crate::document::PresetDocument;
use crate::synth::{Synth, NUM_LFOS, NUM_OSCILLATORS};
use crate::tuning::Tuning;

/// Entries the edit queue holds. A producer gets [`EditError::QueueFull`]
/// rather than block; the queue only fills while the audio thread is not
/// processing.
pub const EDIT_QUEUE_CAPACITY: usize = 256;

/// Retired payloads the return queue holds: every entry the edit queue can
/// hold retires at most one, and restores add a few more.
const RETIRED_CAPACITY: usize = 2 * EDIT_QUEUE_CAPACITY + 8;

/// Retirements the audio thread can hold back while the return queue is
/// full. It checks for room before each entry, so only a restore can land
/// here.
const OVERFLOW_CAPACITY: usize = 4;

/// Entries one edit outside a batch can need (a created connection queues
/// its linear remap curve too).
const MAX_ENTRIES_PER_EDIT: usize = 2;

const LFO_SLOTS: usize = 0;
const LINE_MAPPING_SLOTS: usize = LFO_SLOTS + NUM_LFOS;
const WAVETABLE_SLOTS: usize = LINE_MAPPING_SLOTS + MAX_MODULATION_CONNECTIONS;
const SAMPLE_SLOT: usize = WAVETABLE_SLOTS + NUM_OSCILLATORS;
const TUNING_SLOT: usize = SAMPLE_SLOT + 1;
const NUM_SLOTS: usize = TUNING_SLOT + 1;

/// What a modulation connection edit returns: the slot, whether it was
/// created, and the host parameter changes the caller must also make (see
/// [`sas_preset::PresetState::connect_modulation_from_gui`]).
pub type Connection = GuiConnection;

/// Why an edit was refused. A refused edit changes nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum EditError {
    UnknownSource(String),
    UnknownDestination(String),
    /// Every modulation slot is in use.
    NoFreeSlot,
    /// An LFO, remap curve or oscillator index out of range.
    BadIndex(usize),
    /// The queue has no room: the audio thread is not draining it (the
    /// plugin is inactive or the host stalled). Retry after at least 10 ms,
    /// a bounded number of times; never spin.
    QueueFull,
    /// The wavetable creator cannot render.
    Render(String),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::UnknownSource(name) => write!(f, "unknown modulation source {name}"),
            EditError::UnknownDestination(name) => {
                write!(f, "unknown modulation destination {name}")
            }
            EditError::NoFreeSlot => write!(f, "no free modulation slot"),
            EditError::BadIndex(index) => write!(f, "index {index} out of range"),
            EditError::QueueFull => write!(f, "the edit queue is full"),
            EditError::Render(error) => write!(f, "the wavetable cannot render: {error}"),
        }
    }
}

impl std::error::Error for EditError {}

/// A wavetable rendered from its creator on a producer thread, ready for
/// [`Batch::set_prepared_wavetable`]: the creator (for the document), the
/// table for the audio thread, and a shared copy for the editor.
pub struct PreparedWavetable {
    creator: WavetableCreator,
    wavetable: Wavetable,
    shared: Arc<Wavetable>,
}

impl PreparedWavetable {
    /// Renders `creator` into a fresh table (`WavetableCreator::render`).
    /// Expensive: call it on the editor or chat thread.
    pub fn render(mut creator: WavetableCreator) -> Result<PreparedWavetable, EditError> {
        let mut wavetable = Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
        creator
            .render(&mut wavetable)
            .map_err(|error| EditError::Render(format!("{error:?}")))?;
        let shared = Arc::new(wavetable.clone());
        Ok(PreparedWavetable {
            creator,
            wavetable,
            shared,
        })
    }

    /// The rendered table.
    pub fn wavetable(&self) -> &Arc<Wavetable> {
        &self.shared
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Lfo(usize),
    LineMapping(usize),
    Wavetable(usize),
    Sample,
    Tuning,
}

impl Target {
    fn slot(self) -> usize {
        match self {
            Target::Lfo(lfo) => LFO_SLOTS + lfo,
            Target::LineMapping(slot) => LINE_MAPPING_SLOTS + slot,
            Target::Wavetable(oscillator) => WAVETABLE_SLOTS + oscillator,
            Target::Sample => SAMPLE_SLOT,
            Target::Tuning => TUNING_SLOT,
        }
    }

    fn of_slot(slot: usize) -> Target {
        match slot {
            s if s < LINE_MAPPING_SLOTS => Target::Lfo(s - LFO_SLOTS),
            s if s < WAVETABLE_SLOTS => Target::LineMapping(s - LINE_MAPPING_SLOTS),
            s if s < SAMPLE_SLOT => Target::Wavetable(s - WAVETABLE_SLOTS),
            SAMPLE_SLOT => Target::Sample,
            _ => Target::Tuning,
        }
    }
}

// Payloads only ever live boxed (in a `Pending`), so the variants' sizes
// cost nothing.
#[allow(clippy::large_enum_variant)]
enum Payload {
    Line(LineGenerator),
    Wavetable(Wavetable),
    Sample(Sample),
    Tuning(Tuning),
}

/// The newest payload for one target, with the sequence number of the
/// queue marker that applies it.
struct Pending {
    seq: u64,
    payload: Payload,
}

#[derive(Clone, Copy, Debug)]
enum Command {
    /// The values a created slot takes (`None` when the mirror found the
    /// two already connected).
    Connect {
        source: &'static str,
        destination: &'static str,
        values: Option<SlotValues>,
    },
    Disconnect {
        source: &'static str,
        destination: &'static str,
    },
    /// Apply the pending payload of `slot` if it is still the one pushed
    /// with `seq`.
    Apply { slot: usize, seq: u64 },
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    generation: u64,
    command: Command,
    /// The last entry of a batch (every single edit is one): the audio
    /// thread applies a batch only once it has all of it.
    commit: bool,
}

/// A whole document for the audio thread to load, tagged with its
/// generation (a host state restore or preset load).
pub struct Restore {
    pub generation: u64,
    pub document: PresetDocument,
}

enum Retired {
    Pending(#[allow(dead_code)] Box<Pending>),
    Restore(#[allow(dead_code)] Box<Restore>),
}

/// The producers' state, behind the mirror lock.
struct Mirror {
    document: PresetDocument,
    generation: u64,
    revision: u64,
    next_seq: u64,
    wavetables: [Option<Arc<Wavetable>>; NUM_OSCILLATORS],
}

struct Shared {
    mirror: Mutex<Mirror>,
    generation: AtomicU64,
    revision: AtomicU64,
    queue: ArrayQueue<Entry>,
    retired: ArrayQueue<Retired>,
    slots: [AtomicPtr<Pending>; NUM_SLOTS],
    restore: AtomicPtr<Restore>,
    /// Payloads the audio thread had to leak because the return queue and
    /// its overflow were both full (never expected; see
    /// [`EditHandle::leaked_payloads`]).
    leaked: AtomicUsize,
}

impl Drop for Shared {
    fn drop(&mut self) {
        for slot in &self.slots {
            let pending = slot.swap(std::ptr::null_mut(), Ordering::AcqRel);
            if !pending.is_null() {
                // SAFETY: every non-null slot pointer came from Box::into_raw
                // and is owned by whoever swaps it out.
                drop(unsafe { Box::from_raw(pending) });
            }
        }
        let restore = self.restore.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !restore.is_null() {
            // SAFETY: as above.
            drop(unsafe { Box::from_raw(restore) });
        }
    }
}

/// The registry's modulation sources, with `'static` names.
fn static_source(name: &str) -> Option<&'static str> {
    static SOURCES: OnceLock<Vec<&'static str>> = OnceLock::new();
    SOURCES
        .get_or_init(|| {
            sas_preset::engine::modulation_sources()
                .into_iter()
                .map(|name| &*Box::leak(name.into_boxed_str()))
                .collect()
        })
        .iter()
        .copied()
        .find(|source| *source == name)
}

/// The registry's modulation destinations.
fn static_destination(name: &str) -> Option<&'static str> {
    static DESTINATIONS: OnceLock<Vec<&'static str>> = OnceLock::new();
    DESTINATIONS
        .get_or_init(sas_preset::engine::modulation_destinations)
        .iter()
        .copied()
        .find(|destination| *destination == name)
}

fn route(source: &str, destination: &str) -> Result<(&'static str, &'static str), EditError> {
    let source = static_source(source).ok_or_else(|| EditError::UnknownSource(source.into()))?;
    let destination = static_destination(destination)
        .ok_or_else(|| EditError::UnknownDestination(destination.into()))?;
    Ok((source, destination))
}

fn check_index(index: usize, count: usize) -> Result<(), EditError> {
    if index < count {
        Ok(())
    } else {
        Err(EditError::BadIndex(index))
    }
}

/// The bus: [`EditBus::new`] makes the producer handle and the audio-thread
/// consumer around a document.
pub struct EditBus;

impl EditBus {
    /// A bus whose mirror starts as `document`, the state the engine is in
    /// (generation 0, which the consumer starts with too).
    #[allow(clippy::new_ret_no_self)]
    pub fn new(document: PresetDocument) -> (EditHandle, EditConsumer) {
        let shared = Arc::new(Shared {
            mirror: Mutex::new(Mirror {
                document,
                generation: 0,
                revision: 0,
                next_seq: 0,
                wavetables: Default::default(),
            }),
            generation: AtomicU64::new(0),
            revision: AtomicU64::new(0),
            queue: ArrayQueue::new(EDIT_QUEUE_CAPACITY),
            retired: ArrayQueue::new(RETIRED_CAPACITY),
            slots: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            restore: AtomicPtr::new(std::ptr::null_mut()),
            leaked: AtomicUsize::new(0),
        });
        let consumer = EditConsumer {
            shared: shared.clone(),
            loaded_generation: 0,
            staged: Vec::with_capacity(EDIT_QUEUE_CAPACITY),
            held: None,
            overflow: Vec::with_capacity(OVERFLOW_CAPACITY),
        };
        (EditHandle { shared }, consumer)
    }
}

// Producer-side and short-lived.
#[allow(clippy::large_enum_variant)]
enum Staged {
    Command(Command),
    Pending(Target, Payload),
}

/// The edits of one [`EditHandle::batch`] (or of one single edit). Each
/// method updates the mirror and stages its queue entries; nothing reaches
/// the queue until the batch commits.
pub struct Batch<'a> {
    mirror: &'a mut Mirror,
    staged: Vec<Staged>,
    /// Values replaced in the mirror that a rollback puts back, kept out of
    /// the batch's snapshot because they can be large.
    replaced_sample: Option<SampleState>,
    replaced_wavetables: [Option<(WavetableCreator, Option<Arc<Wavetable>>)>; NUM_OSCILLATORS],
}

impl Batch<'_> {
    /// A source dropped on a control (see
    /// [`PresetState::connect_modulation_from_gui`](sas_preset::PresetState::connect_modulation_from_gui)):
    /// queues the connection (with `values` for a new slot, which the engine
    /// sets with it) and that slot's linear remap curve. The caller must set
    /// the returned `parameter_changes` on the host parameters too, so the
    /// host saves them: they carry the new slot's power, amount, polarity,
    /// stereo and bypass, which a freed slot would otherwise keep from its
    /// old route. [`SlotValues::dragged_to`] gives upstream's values for a
    /// drag onto a control. Put the slot's final values in `values` (and
    /// still send `parameter_changes`) rather than following the connection
    /// with separate parameter sets, which can land a block apart from it.
    pub fn connect_modulation(
        &mut self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Result<Connection, EditError> {
        let (source, destination) = route(source, destination)?;
        let connection = self
            .mirror
            .document
            .state
            .connect_modulation_from_gui(source, destination, values)
            .map_err(|error| match error {
                sas_preset::Error::NoModulationSlot => EditError::NoFreeSlot,
                sas_preset::Error::UnknownModulationSource(name) => EditError::UnknownSource(name),
                _ => EditError::UnknownDestination(destination.to_string()),
            })?;
        self.staged.push(Staged::Command(Command::Connect {
            source,
            destination,
            values: connection.created.then_some(values),
        }));
        if connection.created {
            self.staged.push(Staged::Pending(
                Target::LineMapping(connection.slot),
                Payload::Line(LineGenerator::default()),
            ));
        }
        Ok(connection)
    }

    /// `SynthGuiInterface::disconnectModulation`: frees the slot (its
    /// amount and curve stay). Returns the slot, or `None` if the two were
    /// not connected (nothing is queued then).
    pub fn disconnect_modulation(
        &mut self,
        source: &str,
        destination: &str,
    ) -> Result<Option<usize>, EditError> {
        let (source, destination) = route(source, destination)?;
        let slot = self
            .mirror
            .document
            .state
            .remove_modulation(source, destination);
        if slot.is_some() {
            self.staged.push(Staged::Command(Command::Disconnect {
                source,
                destination,
            }));
        }
        Ok(slot)
    }

    /// The remap curve of modulation slot `slot` (0-based).
    pub fn set_line_mapping(&mut self, slot: usize, line: LineGenerator) -> Result<(), EditError> {
        check_index(slot, MAX_MODULATION_CONNECTIONS)?;
        *self
            .mirror
            .document
            .state
            .line_mapping_mut(slot)
            .expect("checked above") = line.clone();
        self.staged.push(Staged::Pending(
            Target::LineMapping(slot),
            Payload::Line(line),
        ));
        Ok(())
    }

    /// LFO `lfo`'s shape (0-based).
    pub fn set_lfo_shape(&mut self, lfo: usize, line: LineGenerator) -> Result<(), EditError> {
        check_index(lfo, NUM_LFOS)?;
        self.mirror.document.state.lfos[lfo] = line.clone();
        self.staged
            .push(Staged::Pending(Target::Lfo(lfo), Payload::Line(line)));
        Ok(())
    }

    /// Oscillator `oscillator`'s wavetable, rendered beforehand.
    pub fn set_prepared_wavetable(
        &mut self,
        oscillator: usize,
        prepared: PreparedWavetable,
    ) -> Result<(), EditError> {
        check_index(oscillator, NUM_OSCILLATORS)?;
        let creator = std::mem::replace(
            &mut self.mirror.document.state.wavetables[oscillator],
            prepared.creator,
        );
        let shared = self.mirror.wavetables[oscillator].replace(prepared.shared);
        if self.replaced_wavetables[oscillator].is_none() {
            self.replaced_wavetables[oscillator] = Some((creator, shared));
        }
        self.staged.push(Staged::Pending(
            Target::Wavetable(oscillator),
            Payload::Wavetable(prepared.wavetable),
        ));
        Ok(())
    }

    /// The sample (`Sample::loadSample`).
    pub fn set_sample(&mut self, sample: SampleState) -> Result<(), EditError> {
        let loaded = Sample::from_buffers(
            &sample.name,
            &sample.left,
            sample.right.as_deref(),
            sample.sample_rate,
        );
        let replaced = std::mem::replace(&mut self.mirror.document.state.sample, sample);
        if self.replaced_sample.is_none() {
            self.replaced_sample = Some(replaced);
        }
        self.staged
            .push(Staged::Pending(Target::Sample, Payload::Sample(loaded)));
        Ok(())
    }

    /// The tuning (`TuningSelector::loadTuning`, `SynthBase::loadTuningFile`):
    /// built on the producer's thread and swapped into the engine, which
    /// keeps converting notes through it from the next block.
    pub fn set_tuning(&mut self, tuning: Tuning) {
        self.mirror.document.tuning = tuning.clone();
        self.staged
            .push(Staged::Pending(Target::Tuning, Payload::Tuning(tuning)));
    }

    /// The preset's name, author, comments, style and macro names
    /// (`SynthBase::setPresetName`, `setAuthor`, `setComments`, `setStyle`,
    /// `setMacroName`, which only write `save_info_`). Only the mirror
    /// changes: the engine never reads them, so nothing is queued.
    pub fn set_info(&mut self, info: SaveInfo) {
        self.mirror.document.state.info = info;
    }

    fn new(mirror: &mut Mirror) -> Batch<'_> {
        Batch {
            mirror,
            staged: Vec::new(),
            replaced_sample: None,
            replaced_wavetables: Default::default(),
        }
    }
}

/// The producer side, for the editor and the chat worker: cheap to clone,
/// `Send` and `Sync`. Every method takes only the mirror lock, which the
/// audio thread never takes.
#[derive(Clone)]
pub struct EditHandle {
    shared: Arc<Shared>,
}

impl EditHandle {
    fn lock(&self) -> MutexGuard<'_, Mirror> {
        self.shared
            .mirror
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Drops the payloads the audio thread has handed back. Every other
    /// method does this too; an editor may also call it from a timer.
    pub fn collect_retired(&self) {
        while self.shared.retired.pop().is_some() {}
    }

    /// Bumped by every host state restore and preset load. Edits made before
    /// the bump are dropped; an undo history from before it is stale.
    pub fn generation(&self) -> u64 {
        self.collect_retired();
        self.shared.generation.load(Ordering::Acquire)
    }

    /// Bumped by every change to the mirror: each edit or batch from any
    /// producer, and each restore or load.
    pub fn revision(&self) -> u64 {
        self.collect_retired();
        self.shared.revision.load(Ordering::Acquire)
    }

    /// Payloads the audio thread leaked because nothing collected the
    /// returned ones (the return queue and its overflow were full). Always 0
    /// while producers make edits, since every edit collects.
    pub fn leaked_payloads(&self) -> usize {
        self.shared.leaked.load(Ordering::Relaxed)
    }

    /// Entries the queue has room for now.
    pub fn capacity_remaining(&self) -> usize {
        EDIT_QUEUE_CAPACITY - self.shared.queue.len()
    }

    /// Reads the mirror. Its control values are stale: the host parameters
    /// hold the live ones.
    pub fn with_document<R>(&self, f: impl FnOnce(&PresetDocument) -> R) -> R {
        self.collect_retired();
        f(&self.lock().document)
    }

    /// Several edits, applied all together or not at all: the mirror is
    /// rolled back and nothing is queued if any of them fails or the queue
    /// lacks room for all of them; the audio thread applies them within one
    /// block. Render wavetables before the batch
    /// ([`PreparedWavetable::render`]), not inside it.
    pub fn batch<R>(
        &self,
        edits: impl FnOnce(&mut Batch<'_>) -> Result<R, EditError>,
    ) -> Result<R, EditError> {
        self.collect_retired();
        let mut mirror = self.lock();
        let snapshot = Self::snapshot(&mut mirror.document);
        let mut batch = Batch::new(&mut mirror);
        let result = edits(&mut batch);
        let Batch {
            staged,
            replaced_sample,
            replaced_wavetables,
            ..
        } = batch;
        let result = result.and_then(|value| {
            if staged.len() <= self.capacity_remaining() {
                Ok(value)
            } else {
                Err(EditError::QueueFull)
            }
        });
        match result {
            Ok(value) => {
                self.push(&mut mirror, staged);
                Ok(value)
            }
            Err(error) => {
                let mirror = &mut *mirror;
                let sample = replaced_sample
                    .unwrap_or_else(|| std::mem::take(&mut mirror.document.state.sample));
                let mut wavetables = std::mem::take(&mut mirror.document.state.wavetables);
                for (oscillator, replaced) in replaced_wavetables.into_iter().enumerate() {
                    if let Some((creator, shared)) = replaced {
                        wavetables[oscillator] = creator;
                        mirror.wavetables[oscillator] = shared;
                    }
                }
                mirror.document = snapshot;
                mirror.document.state.sample = sample;
                mirror.document.state.wavetables = wavetables;
                Err(error)
            }
        }
    }

    /// A copy of `document` without its sample and wavetables (a batch logs
    /// those when it replaces them).
    fn snapshot(document: &mut PresetDocument) -> PresetDocument {
        let sample = std::mem::take(&mut document.state.sample);
        let wavetables = std::mem::take(&mut document.state.wavetables);
        let snapshot = document.clone();
        document.state.sample = sample;
        document.state.wavetables = wavetables;
        snapshot
    }

    /// One edit. Each edit checks its input before changing the mirror, so
    /// a refused one changes nothing.
    fn single<R>(
        &self,
        edit: impl FnOnce(&mut Batch<'_>) -> Result<R, EditError>,
    ) -> Result<R, EditError> {
        self.collect_retired();
        let mut mirror = self.lock();
        if self.capacity_remaining() < MAX_ENTRIES_PER_EDIT {
            return Err(EditError::QueueFull);
        }
        let mut batch = Batch::new(&mut mirror);
        let value = edit(&mut batch)?;
        let staged = std::mem::take(&mut batch.staged);
        self.push(&mut mirror, staged);
        Ok(value)
    }

    /// Queues `staged` under the mirror lock (which every producer holds
    /// while pushing, so the room checked under it is still there).
    fn push(&self, mirror: &mut Mirror, staged: Vec<Staged>) {
        let count = staged.len();
        for (index, staged) in staged.into_iter().enumerate() {
            let command = match staged {
                Staged::Command(command) => command,
                Staged::Pending(target, payload) => {
                    mirror.next_seq += 1;
                    let seq = mirror.next_seq;
                    let slot = target.slot();
                    let pending = Box::into_raw(Box::new(Pending { seq, payload }));
                    let replaced = self.shared.slots[slot].swap(pending, Ordering::AcqRel);
                    if !replaced.is_null() {
                        // Not taken by the audio thread yet: superseded, and
                        // freed here, on the producer's thread.
                        // SAFETY: swapped out, so owned here.
                        drop(unsafe { Box::from_raw(replaced) });
                    }
                    Command::Apply { slot, seq }
                }
            };
            let entry = Entry {
                generation: mirror.generation,
                command,
                commit: index + 1 == count,
            };
            if self.shared.queue.push(entry).is_err() {
                unreachable!("room was checked under the mirror lock");
            }
        }
        mirror.revision += 1;
        self.shared
            .revision
            .store(mirror.revision, Ordering::Release);
    }

    /// See [`Batch::connect_modulation`].
    pub fn connect_modulation(
        &self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Result<Connection, EditError> {
        self.single(|batch| batch.connect_modulation(source, destination, values))
    }

    /// See [`Batch::disconnect_modulation`].
    pub fn disconnect_modulation(
        &self,
        source: &str,
        destination: &str,
    ) -> Result<Option<usize>, EditError> {
        self.single(|batch| batch.disconnect_modulation(source, destination))
    }

    /// See [`Batch::set_line_mapping`].
    pub fn set_line_mapping(&self, slot: usize, line: LineGenerator) -> Result<(), EditError> {
        self.single(|batch| batch.set_line_mapping(slot, line))
    }

    /// See [`Batch::set_lfo_shape`].
    pub fn set_lfo_shape(&self, lfo: usize, line: LineGenerator) -> Result<(), EditError> {
        self.single(|batch| batch.set_lfo_shape(lfo, line))
    }

    /// See [`Batch::set_tuning`].
    pub fn set_tuning(&self, tuning: Tuning) -> Result<(), EditError> {
        self.single(|batch| {
            batch.set_tuning(tuning);
            Ok(())
        })
    }

    /// See [`Batch::set_info`]. Bumps the revision and queues nothing, so
    /// it never fails; read the current info with [`Self::with_document`].
    pub fn set_info(&self, info: SaveInfo) {
        self.collect_retired();
        let mut mirror = self.lock();
        mirror.document.state.info = info;
        self.push(&mut mirror, Vec::new());
    }

    /// Renders `creator` on the calling thread, then sets it as oscillator
    /// `oscillator`'s wavetable.
    pub fn set_wavetable(
        &self,
        oscillator: usize,
        creator: WavetableCreator,
    ) -> Result<(), EditError> {
        check_index(oscillator, NUM_OSCILLATORS)?;
        let prepared = PreparedWavetable::render(creator)?;
        self.single(|batch| batch.set_prepared_wavetable(oscillator, prepared))
    }

    /// See [`Batch::set_sample`].
    pub fn set_sample(&self, sample: SampleState) -> Result<(), EditError> {
        self.single(|batch| batch.set_sample(sample))
    }

    /// Oscillator `oscillator`'s rendered wavetable, for drawing. Cached;
    /// after a restore it is rendered again on the first call, on the
    /// calling thread.
    pub fn wavetable(&self, oscillator: usize) -> Option<Arc<Wavetable>> {
        if oscillator >= NUM_OSCILLATORS {
            return None;
        }
        let (creator, revision) = {
            let mirror = self.lock();
            if let Some(wavetable) = &mirror.wavetables[oscillator] {
                return Some(wavetable.clone());
            }
            (
                mirror.document.state.wavetables[oscillator].clone(),
                mirror.revision,
            )
        };
        let rendered = PreparedWavetable::render(creator).ok()?.shared;
        let mut mirror = self.lock();
        if mirror.revision == revision {
            mirror.wavetables[oscillator] = Some(rendered.clone());
        }
        Some(rendered)
    }

    /// A host state restore or preset load: `document` replaces the mirror,
    /// the generation moves on (so edits queued before are dropped), and the
    /// audio thread is handed its own copy to load at its next block.
    /// Returns the new generation.
    pub fn begin_restore(&self, document: PresetDocument) -> u64 {
        self.collect_retired();
        let mut mirror = self.lock();
        mirror.generation += 1;
        mirror.revision += 1;
        mirror.wavetables = Default::default();
        let restore = Box::new(Restore {
            generation: mirror.generation,
            document: document.clone(),
        });
        mirror.document = document;
        let replaced = self
            .shared
            .restore
            .swap(Box::into_raw(restore), Ordering::AcqRel);
        if !replaced.is_null() {
            // A restore the audio thread never took.
            // SAFETY: swapped out, so owned here.
            drop(unsafe { Box::from_raw(replaced) });
        }
        self.shared
            .generation
            .store(mirror.generation, Ordering::Release);
        self.shared
            .revision
            .store(mirror.revision, Ordering::Release);
        mirror.generation
    }
}

/// What the edits act on: the [`Synth`] on the audio thread, or nothing
/// (`()`, for an engine that has no synth).
pub trait EditTarget {
    /// `values` are the slot's if the connection creates it.
    fn connect_modulation(&mut self, source: &str, destination: &str, values: Option<&SlotValues>);
    fn disconnect_modulation(&mut self, source: &str, destination: &str);
    fn set_lfo_shape(&mut self, lfo: usize, line: &mut LineGenerator);
    fn set_line_mapping(&mut self, slot: usize, line: &mut LineGenerator);
    fn swap_wavetable(&mut self, oscillator: usize, rendered: &mut Wavetable);
    fn swap_sample(&mut self, loaded: &mut Sample);
    /// Takes `tuning` in place of the engine's own, handing the old one back
    /// in `tuning`.
    fn swap_tuning(&mut self, tuning: &mut Tuning);
    /// Loads a whole restored document (controls, routing, shapes,
    /// wavetables, sample, tuning).
    fn load_document(&mut self, document: &PresetDocument);
}

impl EditTarget for Synth {
    fn connect_modulation(&mut self, source: &str, destination: &str, values: Option<&SlotValues>) {
        match values {
            Some(values) => self.gui_connect_modulation(source, destination, values),
            None => {
                Synth::connect_modulation(self, source, destination);
            }
        }
    }

    fn disconnect_modulation(&mut self, source: &str, destination: &str) {
        Synth::disconnect_modulation(self, source, destination);
    }

    fn set_lfo_shape(&mut self, lfo: usize, line: &mut LineGenerator) {
        Synth::set_lfo_shape(self, lfo, line);
    }

    fn set_line_mapping(&mut self, slot: usize, line: &mut LineGenerator) {
        Synth::set_line_mapping(self, slot, line);
    }

    fn swap_wavetable(&mut self, oscillator: usize, rendered: &mut Wavetable) {
        Synth::swap_wavetable(self, oscillator, rendered);
    }

    fn swap_sample(&mut self, loaded: &mut Sample) {
        Synth::swap_sample(self, loaded);
    }

    fn swap_tuning(&mut self, tuning: &mut Tuning) {
        Synth::swap_tuning(self, tuning);
    }

    fn load_document(&mut self, document: &PresetDocument) {
        self.load_preset(&document.state);
        self.set_tuning(&document.tuning);
    }
}

impl EditTarget for () {
    fn connect_modulation(
        &mut self,
        _source: &str,
        _destination: &str,
        _values: Option<&SlotValues>,
    ) {
    }
    fn disconnect_modulation(&mut self, _source: &str, _destination: &str) {}
    fn set_lfo_shape(&mut self, _lfo: usize, _line: &mut LineGenerator) {}
    fn set_line_mapping(&mut self, _slot: usize, _line: &mut LineGenerator) {}
    fn swap_wavetable(&mut self, _oscillator: usize, _rendered: &mut Wavetable) {}
    fn swap_sample(&mut self, _loaded: &mut Sample) {}
    fn swap_tuning(&mut self, _tuning: &mut Tuning) {}
    fn load_document(&mut self, _document: &PresetDocument) {}
}

/// The audio-thread side. Nothing here allocates, frees or blocks, except
/// loading a whole restored document (as upstream's preset load does).
pub struct EditConsumer {
    shared: Arc<Shared>,
    /// The generation of the document the engine has loaded.
    loaded_generation: u64,
    /// Entries of a batch whose commit has not arrived yet.
    staged: Vec<Entry>,
    /// An entry from a newer generation than the loaded one (its restore
    /// has not been taken yet), or one waiting for room on the return queue.
    held: Option<Entry>,
    /// Retired payloads the full return queue could not take yet.
    overflow: Vec<Retired>,
}

impl EditConsumer {
    /// The generation of the document the engine has loaded.
    pub fn loaded_generation(&self) -> u64 {
        self.loaded_generation
    }

    /// The newest generation the producers have started (it may not be
    /// loaded yet).
    pub fn published_generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }

    /// The newest restored document, if one arrived since the last call. The
    /// caller loads it into the engine, then hands it back with
    /// [`retire_restore`](Self::retire_restore) so it is freed off the audio
    /// thread. From now on edits of its generation apply and older ones are
    /// dropped.
    pub fn take_restore(&mut self) -> Option<Box<Restore>> {
        let restore = self
            .shared
            .restore
            .swap(std::ptr::null_mut(), Ordering::AcqRel);
        if restore.is_null() {
            return None;
        }
        // SAFETY: swapped out, so owned here.
        let restore = unsafe { Box::from_raw(restore) };
        self.loaded_generation = restore.generation;
        Some(restore)
    }

    /// Hands a loaded [`Restore`] back to be freed by a producer.
    pub fn retire_restore(&mut self, restore: Box<Restore>) {
        self.retire(Retired::Restore(restore));
    }

    /// Loads any restored document into `target`, then applies the queued
    /// edits: what the plugin does at the start of each block, for hosts-free
    /// use (tests, offline sessions).
    pub fn drain(&mut self, target: &mut impl EditTarget) {
        if let Some(restore) = self.take_restore() {
            target.load_document(&restore.document);
            self.retire_restore(restore);
        }
        self.apply_edits(target);
    }

    /// Applies every queued edit of the loaded generation to `target`, in
    /// order, and drops older ones (including edits older than a restore
    /// that has not been taken yet). Stops early at an edit of a newer
    /// generation (until its restore is taken) or when the return queue has
    /// no room.
    pub fn apply_edits(&mut self, target: &mut impl EditTarget) {
        if !self.flush_overflow() {
            return;
        }
        let current = self.loaded_generation.max(self.published_generation());
        if self
            .staged
            .first()
            .is_some_and(|entry| entry.generation < current)
        {
            for index in 0..self.staged.len() {
                let entry = self.staged[index];
                self.drop_entry(entry);
            }
            self.staged.clear();
        }
        loop {
            // Every entry retires at most one payload.
            if self.retired_room() == 0 {
                return;
            }
            let Some(entry) = self.held.take().or_else(|| self.shared.queue.pop()) else {
                return;
            };
            if entry.generation > self.loaded_generation {
                self.held = Some(entry);
                return;
            }
            // Older than the loaded document, or than a restore waiting to
            // be taken.
            if entry.generation < self.loaded_generation.max(self.published_generation()) {
                self.drop_entry(entry);
                continue;
            }
            if !entry.commit {
                self.staged.push(entry);
                continue;
            }
            // Room to retire every payload of the batch.
            if self.retired_room() < self.staged.len() + 1 {
                self.held = Some(entry);
                return;
            }
            for index in 0..self.staged.len() {
                let staged = self.staged[index];
                self.apply(staged, target);
            }
            self.staged.clear();
            self.apply(entry, target);
        }
    }

    fn apply(&mut self, entry: Entry, target: &mut impl EditTarget) {
        match entry.command {
            Command::Connect {
                source,
                destination,
                values,
            } => target.connect_modulation(source, destination, values.as_ref()),
            Command::Disconnect {
                source,
                destination,
            } => target.disconnect_modulation(source, destination),
            Command::Apply { slot, seq } => {
                let Some(mut pending) = self.take_pending(slot, seq) else {
                    return;
                };
                match (Target::of_slot(slot), &mut pending.payload) {
                    (Target::Lfo(lfo), Payload::Line(line)) => target.set_lfo_shape(lfo, line),
                    (Target::LineMapping(index), Payload::Line(line)) => {
                        target.set_line_mapping(index, line)
                    }
                    (Target::Wavetable(oscillator), Payload::Wavetable(wavetable)) => {
                        target.swap_wavetable(oscillator, wavetable)
                    }
                    (Target::Sample, Payload::Sample(sample)) => target.swap_sample(sample),
                    (Target::Tuning, Payload::Tuning(tuning)) => target.swap_tuning(tuning),
                    _ => unreachable!("each slot only takes its own payload"),
                }
                self.retire(Retired::Pending(pending));
            }
        }
    }

    /// Drops a stale entry, retiring its payload if it is still pending.
    fn drop_entry(&mut self, entry: Entry) {
        if let Command::Apply { slot, seq } = entry.command {
            if let Some(pending) = self.take_pending(slot, seq) {
                self.retire(Retired::Pending(pending));
            }
        }
    }

    /// Takes the payload pending in `slot` if it is the one pushed with
    /// `seq`. A newer one stays for its own, later marker; an older marker
    /// finds nothing.
    fn take_pending(&mut self, slot: usize, seq: u64) -> Option<Box<Pending>> {
        let raw = self.shared.slots[slot].swap(std::ptr::null_mut(), Ordering::AcqRel);
        if raw.is_null() {
            return None;
        }
        // SAFETY: swapped out, so owned here.
        let pending = unsafe { Box::from_raw(raw) };
        if pending.seq == seq {
            return Some(pending);
        }
        // A newer payload than this marker's: put it back unless a producer
        // has pushed an even newer one meanwhile, which supersedes it.
        let raw = Box::into_raw(pending);
        if let Err(_newer) = self.shared.slots[slot].compare_exchange(
            std::ptr::null_mut(),
            raw,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            // SAFETY: never published, so still owned here.
            self.retire(Retired::Pending(unsafe { Box::from_raw(raw) }));
        }
        None
    }

    fn retired_room(&self) -> usize {
        self.shared.retired.capacity() - self.shared.retired.len()
    }

    fn retire(&mut self, retired: Retired) {
        let Err(retired) = self.shared.retired.push(retired) else {
            return;
        };
        if self.overflow.len() < OVERFLOW_CAPACITY {
            self.overflow.push(retired);
        } else {
            // Growing the overflow would allocate and dropping would free on
            // the audio thread: leak instead, and count it.
            std::mem::forget(retired);
            self.shared.leaked.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Moves overflowed retirements to the return queue; false if some are
    /// still waiting.
    fn flush_overflow(&mut self) -> bool {
        while let Some(retired) = self.overflow.pop() {
            if let Err(retired) = self.shared.retired.push(retired) {
                self.overflow.push(retired);
                return false;
            }
        }
        true
    }
}
