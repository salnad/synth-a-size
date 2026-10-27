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
//! - one pending slot per target (8 LFOs, 64 remap curves, 3 wavetables, a
//!   drawn frame of each of the 3 wavetables, the sample, the tuning, and 64
//!   connections' slot values), newest wins: a push replaces a payload the
//!   audio thread has not taken yet, and a heap-free marker in the queue
//!   keeps it ordered against connections;
//! - a generation tag on every entry: a host state restore or preset load
//!   ([`EditHandle::begin_restore`]) bumps it, and entries queued before it
//!   are dropped;
//! - the producers' mirror of the [`PresetDocument`], updated with each edit,
//!   so the plugin saves the edited state without a reload. The audio thread
//!   never takes its lock.
//!
//! Controls are host parameters and don't travel here, with two exceptions:
//! the power, amount, bipolar, stereo and bypass a new modulation slot takes
//! travel with its connection, and a connection's amount, bipolar, stereo
//! and bypass can be set ([`EditHandle::set_connection_values`]), because
//! upstream writes them to the engine right away (`synth_->valueChanged`)
//! while the editor tells the host only when the user's action ends. They
//! are host parameters as well, and the plugin applies the bus before the
//! block's parameter changes, so a host change in the same block wins.

use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crossbeam_queue::ArrayQueue;
use sas_common::LineGenerator;
use sas_dsp_core::constants::MAX_MODULATION_CONNECTIONS;
use sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_oscillators::sas_wavetable::{Wavetable, WavetableFrame, NUM_OSCILLATOR_WAVE_FRAMES};
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

/// Entries kept free for [`EditHandle::end_audition`], one per oscillator:
/// every other edit leaves them, so ending a wavetable drag never meets a
/// full queue, however long the audio thread stops draining it.
pub const END_AUDITION_RESERVE: usize = NUM_OSCILLATORS;

const LFO_SLOTS: usize = 0;
const LINE_MAPPING_SLOTS: usize = LFO_SLOTS + NUM_LFOS;
const WAVETABLE_SLOTS: usize = LINE_MAPPING_SLOTS + MAX_MODULATION_CONNECTIONS;
const WAVETABLE_FRAME_SLOTS: usize = WAVETABLE_SLOTS + NUM_OSCILLATORS;
const SAMPLE_SLOT: usize = WAVETABLE_FRAME_SLOTS + NUM_OSCILLATORS;
const TUNING_SLOT: usize = SAMPLE_SLOT + 1;
const CONNECTION_VALUE_SLOTS: usize = TUNING_SLOT + 1;
const NUM_SLOTS: usize = CONNECTION_VALUE_SLOTS + MAX_MODULATION_CONNECTIONS;

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

/// Which oscillator wavetable an audition belongs to: the state's
/// generation and the oscillator's wavetable commits and ended auditions
/// when the drag began ([`EditHandle::audition_token`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditionToken {
    generation: u64,
    commit: u64,
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
    WavetableFrame(usize),
    Sample,
    Tuning,
    ConnectionValues(usize),
}

impl Target {
    fn slot(self) -> usize {
        match self {
            Target::Lfo(lfo) => LFO_SLOTS + lfo,
            Target::LineMapping(slot) => LINE_MAPPING_SLOTS + slot,
            Target::Wavetable(oscillator) => WAVETABLE_SLOTS + oscillator,
            Target::WavetableFrame(oscillator) => WAVETABLE_FRAME_SLOTS + oscillator,
            Target::Sample => SAMPLE_SLOT,
            Target::Tuning => TUNING_SLOT,
            Target::ConnectionValues(slot) => CONNECTION_VALUE_SLOTS + slot,
        }
    }

    fn of_slot(slot: usize) -> Target {
        match slot {
            s if s < LINE_MAPPING_SLOTS => Target::Lfo(s - LFO_SLOTS),
            s if s < WAVETABLE_SLOTS => Target::LineMapping(s - LINE_MAPPING_SLOTS),
            s if s < WAVETABLE_FRAME_SLOTS => Target::Wavetable(s - WAVETABLE_SLOTS),
            s if s < SAMPLE_SLOT => Target::WavetableFrame(s - WAVETABLE_FRAME_SLOTS),
            SAMPLE_SLOT => Target::Sample,
            TUNING_SLOT => Target::Tuning,
            s => Target::ConnectionValues(s - CONNECTION_VALUE_SLOTS),
        }
    }
}

// Payloads only ever live boxed (in a `Pending`), so the variants' sizes
// cost nothing.
#[allow(clippy::large_enum_variant)]
enum Payload {
    Line(LineGenerator),
    Wavetable(Wavetable),
    /// One rendered frame and where it goes.
    Frame(usize, WavetableFrame),
    Sample(Sample),
    Tuning(Tuning),
    Values(SlotValues),
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
    /// A matrix row connecting its own slot (values and curve unchanged).
    ConnectSlot {
        slot: usize,
        source: &'static str,
        destination: &'static str,
    },
    /// A matrix row freeing its own slot, which connects `source` to
    /// `destination`.
    DisconnectSlot {
        slot: usize,
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
    /// Each oscillator's wavetable commits ([`Batch::set_prepared_wavetable`])
    /// and ended auditions ([`EditHandle::end_audition`]), which make older
    /// auditions stale.
    wavetable_commits: [u64; NUM_OSCILLATORS],
    /// Whether a drawn frame has reached each oscillator's tables since its
    /// last commit, end of audition or restore, so ending the drag has a
    /// table to put back.
    auditioned: [bool; NUM_OSCILLATORS],
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
                wavetable_commits: [0; NUM_OSCILLATORS],
                auditioned: [false; NUM_OSCILLATORS],
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
    replaced_wavetables: [Option<ReplacedWavetable>; NUM_OSCILLATORS],
}

/// An oscillator's wavetable state before a batch replaced it: its creator,
/// shared table, commit count and audition flag.
type ReplacedWavetable = (WavetableCreator, Option<Arc<Wavetable>>, u64, bool);

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

    /// A modulation matrix row choosing slot `slot`'s route (see
    /// [`PresetState::set_slot_route`](sas_preset::PresetState::set_slot_route)):
    /// the slot's old route is disconnected and `route` connected in the same
    /// slot, with its controls and curve as they are, in one step for the
    /// engine. No host parameter changes. Returns whether the slot is
    /// connected afterwards (a route to its own amount is refused).
    pub fn set_slot_route(
        &mut self,
        slot: usize,
        route_names: Option<(&str, &str)>,
    ) -> Result<bool, EditError> {
        check_index(slot, MAX_MODULATION_CONNECTIONS)?;
        let route_names = route_names
            .map(|(source, destination)| route(source, destination))
            .transpose()?;
        let state = &mut self.mirror.document.state;
        let old = &state.modulations()[slot];
        let old = (!old.is_free())
            .then(|| route(&old.source, &old.destination))
            .transpose()?;
        let connected = state
            .set_slot_route(slot, route_names)
            .map_err(|_| EditError::BadIndex(slot))?;
        if let Some((source, destination)) = old {
            self.staged.push(Staged::Command(Command::DisconnectSlot {
                slot,
                source,
                destination,
            }));
        }
        if let (true, Some((source, destination))) = (connected, route_names) {
            self.staged.push(Staged::Command(Command::ConnectSlot {
                slot,
                source,
                destination,
            }));
        }
        Ok(connected)
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

    /// `SynthGuiInterface::setModulationValues`: the amount, bipolar,
    /// stereo and bypass of the slot connecting `source` to `destination`,
    /// found when called, set in the engine as they are, with nothing
    /// rebuilt. The caller tells the host when the user's action ends.
    /// Returns the slot, or `None` if the two are not connected (nothing
    /// changes then, as upstream's `getConnectionIndex` check returns). A
    /// newer call for the same slot replaces one the audio thread has not
    /// taken yet.
    pub fn set_connection_values(
        &mut self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Result<Option<usize>, EditError> {
        let (source, destination) = route(source, destination)?;
        let Some(slot) = self
            .mirror
            .document
            .state
            .modulation_index(source, destination)
        else {
            return Ok(None);
        };
        let flag = |on: bool| if on { 1.0 } else { 0.0 };
        let number = slot + 1;
        for (suffix, value) in [
            ("amount", values.amount),
            ("bipolar", flag(values.bipolar)),
            ("stereo", flag(values.stereo)),
            ("bypass", flag(values.bypass)),
        ] {
            self.mirror
                .document
                .state
                .set_value(&format!("modulation_{number}_{suffix}"), value)
                .expect("every modulation slot control is in the registry");
        }
        self.staged.push(Staged::Pending(
            Target::ConnectionValues(slot),
            Payload::Values(values),
        ));
        Ok(Some(slot))
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
        let commits = self.mirror.wavetable_commits[oscillator];
        let auditioned = self.mirror.auditioned[oscillator];
        self.mirror.wavetable_commits[oscillator] += 1;
        self.mirror.auditioned[oscillator] = false;
        if self.replaced_wavetables[oscillator].is_none() {
            self.replaced_wavetables[oscillator] = Some((creator, shared, commits, auditioned));
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
            if staged.len() + END_AUDITION_RESERVE <= self.capacity_remaining() {
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
                    if let Some((creator, shared, commits, auditioned)) = replaced {
                        wavetables[oscillator] = creator;
                        mirror.wavetables[oscillator] = shared;
                        // A drag auditioning before the batch still is.
                        mirror.wavetable_commits[oscillator] = commits;
                        mirror.auditioned[oscillator] = auditioned;
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
        if self.capacity_remaining() < MAX_ENTRIES_PER_EDIT + END_AUDITION_RESERVE {
            return Err(EditError::QueueFull);
        }
        let mut batch = Batch::new(&mut mirror);
        let value = edit(&mut batch)?;
        let staged = std::mem::take(&mut batch.staged);
        self.push(&mut mirror, staged);
        Ok(value)
    }

    /// Queues `staged` under the mirror lock (which every producer holds
    /// while pushing, so the room checked under it is still there), and
    /// bumps the revision.
    fn push(&self, mirror: &mut Mirror, staged: Vec<Staged>) {
        self.queue(mirror, staged);
        mirror.revision += 1;
        self.shared
            .revision
            .store(mirror.revision, Ordering::Release);
    }

    /// Queues `staged` under the mirror lock without bumping the revision,
    /// for auditions, which leave the document alone.
    fn queue(&self, mirror: &mut Mirror, staged: Vec<Staged>) {
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

    /// See [`Batch::set_slot_route`].
    pub fn set_slot_route(
        &self,
        slot: usize,
        route: Option<(&str, &str)>,
    ) -> Result<bool, EditError> {
        self.single(|batch| batch.set_slot_route(slot, route))
    }

    /// See [`Batch::disconnect_modulation`].
    pub fn disconnect_modulation(
        &self,
        source: &str,
        destination: &str,
    ) -> Result<Option<usize>, EditError> {
        self.single(|batch| batch.disconnect_modulation(source, destination))
    }

    /// See [`Batch::set_connection_values`].
    pub fn set_connection_values(
        &self,
        source: &str,
        destination: &str,
        values: SlotValues,
    ) -> Result<Option<usize>, EditError> {
        self.single(|batch| batch.set_connection_values(source, destination, values))
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

    /// While the editor draws on oscillator `oscillator`'s wavetable: frame
    /// `position` of `creator` alone is rendered
    /// (`WavetableCreator::render(position)`, without the full render's
    /// normalisation) into the shared table and the engine's, as upstream's
    /// editor renders into the playing table during a drag. An audition:
    /// the document keeps its creator and the revision doesn't move, so the
    /// drag reaches the document (and a host save) once, when the editor
    /// commits the full render with [`Self::set_wavetable`] at its end.
    ///
    /// The frame is rendered here, on the calling thread, under the mirror
    /// lock, and travels as data the audio thread copies into its table; it
    /// comes back on the return queue. A newer frame for the same
    /// oscillator replaces one the audio thread has not taken yet, even at
    /// another position, and a position past the table's last frame
    /// changes nothing, as `loadWaveFrame` ignores it.
    ///
    /// Returns false, changing nothing, when `token` is stale: the
    /// oscillator's wavetable was committed, the state restored or the drag
    /// ended ([`Self::end_audition`]) since `token` was read.
    ///
    /// Drop any table from [`Self::wavetable`] first: while one is held, the
    /// shared table is copied (about 8.5 MB) to draw into.
    pub fn set_wavetable_frame(
        &self,
        oscillator: usize,
        creator: WavetableCreator,
        position: usize,
        token: AuditionToken,
    ) -> Result<bool, EditError> {
        check_index(oscillator, NUM_OSCILLATORS)?;
        check_index(position, NUM_OSCILLATOR_WAVE_FRAMES as usize)?;
        if self.is_stale(&self.lock(), oscillator, token) {
            return Ok(false);
        }
        // The shared table, rendered first if a restore cleared it.
        self.wavetable(oscillator)
            .ok_or_else(|| EditError::Render("the wavetable cannot render".to_string()))?;
        self.collect_retired();
        let mut mirror = self.lock();
        if self.is_stale(&mirror, oscillator, token) {
            return Ok(false);
        }
        if self.capacity_remaining() < 1 + END_AUDITION_RESERVE {
            return Err(EditError::QueueFull);
        }
        let mut creator = creator;
        let frame = match &mut mirror.wavetables[oscillator] {
            Some(shared) => {
                // Copied only while the editor still holds the last one.
                let table = Arc::make_mut(shared);
                creator.render_frame(table, position as i32);
                table.frame(position)
            }
            None => None,
        };
        let staged = frame
            .map(|frame| {
                Staged::Pending(
                    Target::WavetableFrame(oscillator),
                    Payload::Frame(position, frame),
                )
            })
            .into_iter()
            .collect::<Vec<_>>();
        if !staged.is_empty() {
            mirror.auditioned[oscillator] = true;
        }
        self.queue(&mut mirror, staged);
        Ok(true)
    }

    /// Whether `token` predates the state's generation or the oscillator's
    /// last wavetable commit.
    fn is_stale(&self, mirror: &Mirror, oscillator: usize, token: AuditionToken) -> bool {
        mirror.generation != token.generation
            || mirror.wavetable_commits[oscillator] != token.commit
    }

    /// The token a drag on oscillator `oscillator`'s wavetable passes to
    /// [`Self::set_wavetable_frame`] and [`Self::end_audition`]: read it
    /// when the drag starts.
    pub fn audition_token(&self, oscillator: usize) -> Result<AuditionToken, EditError> {
        check_index(oscillator, NUM_OSCILLATORS)?;
        let mirror = self.lock();
        Ok(AuditionToken {
            generation: mirror.generation,
            commit: mirror.wavetable_commits[oscillator],
        })
    }

    /// Ends a drag that won't be committed (cancelled, or the editor closed
    /// or failed during it): renders the document's creator on the calling
    /// thread and swaps it into the shared table and the engine's, so what
    /// plays is again what a save stores. Like the drawn frames, it leaves
    /// the document and the revision alone, and returns false, changing
    /// nothing, when `token` is stale, since a commit or restore has
    /// already replaced the table. It makes `token` stale, so a frame of
    /// the drag still in flight can't reach the table it restored.
    ///
    /// It never meets a full queue: every other edit leaves
    /// [`END_AUDITION_RESERVE`] entries free, and a drag whose frames
    /// reached nothing since the last commit, end or restore queues nothing,
    /// so the free entries never drop below the oscillators with a table to
    /// put back.
    pub fn end_audition(&self, oscillator: usize, token: AuditionToken) -> Result<bool, EditError> {
        check_index(oscillator, NUM_OSCILLATORS)?;
        let creator = {
            let mut mirror = self.lock();
            if self.is_stale(&mirror, oscillator, token) {
                return Ok(false);
            }
            if !mirror.auditioned[oscillator] {
                // The tables are the document's already.
                mirror.wavetable_commits[oscillator] += 1;
                return Ok(true);
            }
            mirror.document.state.wavetables[oscillator].clone()
        };
        let prepared = PreparedWavetable::render(creator)?;
        self.collect_retired();
        let mut mirror = self.lock();
        if self.is_stale(&mirror, oscillator, token) {
            return Ok(false);
        }
        if self.capacity_remaining() < 1 {
            return Err(EditError::QueueFull);
        }
        mirror.wavetable_commits[oscillator] += 1;
        mirror.auditioned[oscillator] = false;
        mirror.wavetables[oscillator] = Some(prepared.shared);
        let staged = vec![Staged::Pending(
            Target::Wavetable(oscillator),
            Payload::Wavetable(prepared.wavetable),
        )];
        self.queue(&mut mirror, staged);
        Ok(true)
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
            // An audition may have stored a table meanwhile; it stays.
            let stored = mirror.wavetables[oscillator].get_or_insert(rendered);
            return Some(stored.clone());
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
        mirror.auditioned = [false; NUM_OSCILLATORS];
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
    /// Connects bank slot `slot` itself (`connectModulation(connection)`).
    fn connect_slot(&mut self, slot: usize, source: &str, destination: &str);
    /// Frees bank slot `slot` itself, which connects `source` to
    /// `destination` (`disconnectModulation(connection)`).
    fn disconnect_slot(&mut self, slot: usize, source: &str, destination: &str);
    fn set_lfo_shape(&mut self, lfo: usize, line: &mut LineGenerator);
    fn set_line_mapping(&mut self, slot: usize, line: &mut LineGenerator);
    /// Modulation slot `slot`'s amount, bipolar, stereo and bypass.
    fn set_connection_values(&mut self, slot: usize, values: &SlotValues);
    fn swap_wavetable(&mut self, oscillator: usize, rendered: &mut Wavetable);
    /// Writes `frame` over frame `position` of oscillator `oscillator`'s
    /// wavetable.
    fn set_wavetable_frame(&mut self, oscillator: usize, position: usize, frame: &WavetableFrame);
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
    fn connect_slot(&mut self, slot: usize, source: &str, destination: &str) {
        Synth::connect_bank_slot(self, slot, source, destination);
    }
    fn disconnect_slot(&mut self, slot: usize, source: &str, destination: &str) {
        Synth::disconnect_bank_slot(self, slot, source, destination);
    }

    fn set_lfo_shape(&mut self, lfo: usize, line: &mut LineGenerator) {
        Synth::set_lfo_shape(self, lfo, line);
    }

    fn set_line_mapping(&mut self, slot: usize, line: &mut LineGenerator) {
        Synth::set_line_mapping(self, slot, line);
    }

    fn set_connection_values(&mut self, slot: usize, values: &SlotValues) {
        Synth::set_connection_values(self, slot, values);
    }

    fn swap_wavetable(&mut self, oscillator: usize, rendered: &mut Wavetable) {
        Synth::swap_wavetable(self, oscillator, rendered);
    }

    fn set_wavetable_frame(&mut self, oscillator: usize, position: usize, frame: &WavetableFrame) {
        Synth::set_wavetable_frame(self, oscillator, position, frame);
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
    fn connect_slot(&mut self, _slot: usize, _source: &str, _destination: &str) {}
    fn disconnect_slot(&mut self, _slot: usize, _source: &str, _destination: &str) {}
    fn set_lfo_shape(&mut self, _lfo: usize, _line: &mut LineGenerator) {}
    fn set_line_mapping(&mut self, _slot: usize, _line: &mut LineGenerator) {}
    fn set_connection_values(&mut self, _slot: usize, _values: &SlotValues) {}
    fn swap_wavetable(&mut self, _oscillator: usize, _rendered: &mut Wavetable) {}
    fn set_wavetable_frame(
        &mut self,
        _oscillator: usize,
        _position: usize,
        _frame: &WavetableFrame,
    ) {
    }
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
            Command::ConnectSlot {
                slot,
                source,
                destination,
            } => target.connect_slot(slot, source, destination),
            Command::DisconnectSlot {
                slot,
                source,
                destination,
            } => target.disconnect_slot(slot, source, destination),
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
                    (Target::WavetableFrame(oscillator), Payload::Frame(position, frame)) => {
                        target.set_wavetable_frame(oscillator, *position, frame)
                    }
                    (Target::Sample, Payload::Sample(sample)) => target.swap_sample(sample),
                    (Target::Tuning, Payload::Tuning(tuning)) => target.swap_tuning(tuning),
                    (Target::ConnectionValues(index), Payload::Values(values)) => {
                        target.set_connection_values(index, values)
                    }
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
