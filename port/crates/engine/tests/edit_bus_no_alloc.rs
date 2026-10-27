// SPDX-License-Identifier: GPL-3.0-or-later

//! The audio side of the edit bus never allocates or frees, under a
//! producer thread hammering it with edits while the synth plays notes,
//! pedals, bends and takes parameter changes.
//!
//! Every allocation and free on the audio thread is counted, except two
//! kinds of work that allocate upstream too and are left as ported:
//!
//! - loading a whole restored document (upstream loads a preset with
//!   processing paused);
//! - rewiring the processor graph for a modulation connection
//!   (`Synth::process_modulation_changes`, and the routers' order updates
//!   it triggers in later blocks; upstream's `ProcessorRouter` creates
//!   inputs, feedback nodes and processor copies there too).
//!
//! So [`shapes_wavetables_and_samples_never_allocate`] counts whole blocks
//! on presets without modulation routing, where nothing rewires the graph
//! (the editor's telemetry snapshot included), and
//! [`connection_edits_never_allocate`] counts the bus and the
//! connection bookkeeping `Synth::connect_modulation` /
//! `disconnect_modulation` do on the audio thread, leaving the rewiring
//! uncounted.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sas_common::LineGenerator;
use sas_engine::document::PresetDocument;
use sas_engine::edit_bus::{EditBus, EditError, EditHandle};
use sas_engine::synth::Synth;
use sas_engine::telemetry::Telemetry;
use sas_oscillators::RandomValues;
use sas_preset::{InitContent, PresetState, SampleState, SlotValues};

struct CountingAllocator;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
}
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static SITES: Mutex<BTreeMap<String, usize>> = Mutex::new(BTreeMap::new());

fn note_allocation() {
    if !COUNTING.with(Cell::get) {
        return;
    }
    // Sites are traced for the first allocations only (tracing is slow).
    if ALLOCATIONS.fetch_add(1, Ordering::Relaxed) >= 500 {
        return;
    }
    COUNTING.with(|counting| counting.set(false));
    // Where it happened: the innermost frames in the port's own sources.
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    let site: Vec<&str> = trace
        .lines()
        .map(str::trim)
        .filter(|line| {
            line.starts_with("at ./src/") || line.starts_with("at /workspace/port/crates/")
        })
        .take(3)
        .collect();
    if let Ok(mut sites) = SITES.try_lock() {
        *sites.entry(site.join(" < ")).or_default() += 1;
    }
    COUNTING.with(|counting| counting.set(true));
}

// SAFETY: forwards to the system allocator; the counter has no effect on it.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note_allocation();
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note_allocation();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn counted<R>(f: impl FnOnce() -> R) -> R {
    COUNTING.with(|counting| counting.set(true));
    let result = f();
    COUNTING.with(|counting| counting.set(false));
    result
}

fn corpus(path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../migration/corpus")
        .join(path)
}

fn preset(name: &str) -> PresetState {
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset
        .load_file(&corpus(&format!("presets/{name}.vital")))
        .unwrap();
    preset
}

/// Retries a refused edit the way producers must: back off, never spin.
fn retry<R>(mut edit: impl FnMut() -> Result<R, EditError>) -> R {
    for _ in 0..500 {
        match edit() {
            Err(EditError::QueueFull) => std::thread::sleep(Duration::from_millis(1)),
            result => return result.unwrap(),
        }
    }
    panic!("the queue stayed full");
}

fn drag(count: usize) -> SlotValues {
    SlotValues {
        amount: (count % 9) as f32 / 9.0,
        bipolar: count.is_multiple_of(3),
        stereo: false,
        bypass: false,
    }
}

fn producer(edits: EditHandle, stop: Arc<AtomicBool>, routing: bool) -> usize {
    let wavetables = preset("line_wavetable_fold").wavetables;
    let septimal = sas_engine::Tuning::from_file(&corpus("tunings/corpus_septimal.scl")).unwrap();
    let shapes: [fn(&mut LineGenerator); 4] = [
        LineGenerator::init_sin,
        LineGenerator::init_square,
        LineGenerator::init_saw_up,
        LineGenerator::init_triangle,
    ];
    let routes = [
        ("lfo_2", "filter_1_cutoff"),
        ("env_2", "osc_1_wave_frame"),
        ("random_1", "chorus_dry_wet"),
        ("macro_control_1", "reverb_size"),
    ];
    let mut count = 0;
    while !stop.load(Ordering::Relaxed) {
        let mut line = LineGenerator::default();
        shapes[count % shapes.len()](&mut line);
        line.set_name(&format!("shape {count}"));
        retry(|| edits.set_lfo_shape(count % 8, line.clone()));
        retry(|| edits.set_line_mapping(count % 64, line.clone()));
        retry(|| edits.set_connection_values("lfo_2", "filter_1_cutoff", drag(count)));
        if routing {
            let (source, destination) = routes[count % routes.len()];
            if count % 2 == 0 {
                retry(|| edits.connect_modulation(source, destination, drag(count)));
            } else {
                retry(|| edits.disconnect_modulation(source, destination));
            }
            if count % 25 == 0 {
                retry(|| {
                    edits.batch(|batch| {
                        batch.connect_modulation("lfo_3", "osc_2_level", drag(count))?;
                        batch.set_lfo_shape(2, line.clone())?;
                        batch.disconnect_modulation("lfo_3", "osc_2_level")
                    })
                });
            }
        } else if count % 25 == 0 {
            retry(|| {
                edits.batch(|batch| {
                    batch.set_lfo_shape(2, line.clone())?;
                    batch.set_line_mapping(3, line.clone())
                })
            });
        }
        if count % 40 == 0 {
            let creator = wavetables[(count / 40) % wavetables.len()].clone();
            retry(|| edits.set_wavetable((count / 40) % 3, creator.clone()));
        }
        if count % 4 == 1 {
            // Drawing on a wavetable: one frame at a time.
            let creator = wavetables[count % wavetables.len()].clone();
            retry(|| edits.set_wavetable_frame(count % 3, creator.clone(), (count * 7) % 257));
        }
        if count % 40 == 20 {
            // The drag's whole-table auditions between its frames.
            let oscillator = (count / 40) % 3;
            let creator = wavetables[(count / 20) % wavetables.len()].clone();
            retry(|| {
                let token = edits.audition_token(oscillator)?;
                edits.audition_wavetable(oscillator, creator.clone(), token)
            });
        }
        if count % 60 == 0 {
            let sample = SampleState {
                name: format!("sample {count}"),
                sample_rate: 44100,
                left: (0..2048)
                    .map(|i| ((i * (count + 1)) % 97) as f32 / 97.0 - 0.5)
                    .collect(),
                right: (count % 120 == 0).then(|| vec![0.25; 2048]),
            };
            retry(|| edits.set_sample(sample.clone()));
        }
        if count % 30 == 15 {
            let tuning = if count % 60 == 15 {
                septimal.clone()
            } else {
                sas_engine::Tuning::new()
            };
            retry(|| edits.set_tuning(tuning.clone()));
        }
        if count % 150 == 149 {
            let name = ["unison_pad_fx", "init_saw"][(count / 150) % 2];
            edits.begin_restore(PresetDocument {
                state: preset(name),
                tuning: sas_engine::Tuning::new(),
            });
        }
        count += 1;
        std::thread::sleep(Duration::from_micros(200));
    }
    count
}

/// Plays `blocks` blocks while a producer edits; `routing` adds connection
/// edits. Returns the producer's rounds and the restores loaded.
fn play_while_editing(routing: bool, blocks: usize) -> (usize, usize) {
    sas_common::random::set_next_seed(3);
    RandomValues::reset_process_state();
    let mut synth = Synth::new();
    let document = PresetDocument {
        state: preset("unison_pad_fx"),
        tuning: sas_engine::Tuning::new(),
    };
    synth.load_preset(&document.state);
    synth.prepare(44100, 120.0);
    let (edits, mut consumer) = EditBus::new(document);
    let (mut telemetry, _reader) = Telemetry::new(synth.audio_memory());

    let (mut left, mut right) = (vec![0.0f32; 256], vec![0.0f32; 256]);
    let mut seconds = 0.0;
    // Warm up: every voice and effect has run once, and CC 20 learns
    // macro_control_1 (learning grows the learn map).
    synth.arm_midi_learn("macro_control_1");
    synth.render_block(
        &mut left,
        &mut right,
        &[(0, [0xB0, 20, 0])],
        &mut seconds,
        44100,
    );
    assert!(synth.is_midi_mapped("macro_control_1"));
    for block in 0..200 {
        let note = 36 + (block % 37) as u8;
        let midi = [(0, [0x90, note, 100]), (10, [0x80, note, 0])];
        synth.render_block(&mut left, &mut right, &midi, &mut seconds, 44100);
    }

    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let edits = edits.clone();
        let stop = stop.clone();
        std::thread::spawn(move || producer(edits, stop, routing))
    };

    let mut loaded = 0;
    for block in 0..blocks {
        if let Some(restore) = consumer.take_restore() {
            synth.load_preset(&restore.document.state);
            consumer.retire_restore(restore);
            loaded += 1;
        }
        counted(|| consumer.apply_edits(&mut synth));
        let note = 36 + (block % 37) as u8;
        let midi = [
            (5, [0x90, note, 90]),
            (40, [0xB0, 64, if block % 16 < 8 { 127 } else { 0 }]),
            (100, [0x80, 36 + ((block + 30) % 37) as u8, 0]),
            (180, [0xE0, 0, (block % 128) as u8]),
            (200, [0xD0, (block % 100) as u8, 0]),
            (220, [0xB0, 20, (block % 128) as u8]),
        ];
        let size = [256, 64, 129, 17][block % 4];
        let mut render = || {
            synth.process_modulation_changes();
            synth.set_control("filter_1_cutoff", 40.0 + (block % 80) as f32);
            synth.set_control("modulation_1_amount", (block % 10) as f32 / 10.0);
            synth.set_control("mod_wheel", (block % 7) as f32 / 7.0);
            let midi = &midi[..if size == 256 { 6 } else { 1 }];
            synth.render_block(
                &mut left[..size],
                &mut right[..size],
                midi,
                &mut seconds,
                44100,
            );
            synth.write_telemetry(&mut telemetry);
        };
        if routing {
            render();
        } else {
            counted(render);
        }
    }
    stop.store(true, Ordering::Relaxed);
    let produced = worker.join().unwrap();
    counted(|| consumer.apply_edits(&mut synth));
    edits.collect_retired();
    assert!(left.iter().chain(&right).all(|s| s.is_finite()));
    (produced, loaded)
}

fn assert_no_allocations() {
    let allocations = ALLOCATIONS.swap(0, Ordering::Relaxed);
    let sites = std::mem::take(
        &mut *SITES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    );
    assert_eq!(
        allocations,
        0,
        "the audio side allocated or freed {allocations} times, at:\n{}",
        sites
            .iter()
            .map(|(site, count)| format!("{count:8}  {site}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The two tests share the counters and the engine's seed counter.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn shapes_wavetables_and_samples_never_allocate() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (produced, loaded) = play_while_editing(false, 4000);
    assert!(
        produced > 100,
        "the producer only made {produced} rounds of edits"
    );
    assert!(loaded > 0, "no restore reached the audio side");
    assert_no_allocations();
}

#[test]
fn connection_edits_never_allocate() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (produced, _) = play_while_editing(true, 4000);
    assert!(
        produced > 100,
        "the producer only made {produced} rounds of edits"
    );
    assert_no_allocations();
}

/// Steady state on presets with modulation routing, with no edits: once
/// every voice has played, whole blocks (notes, pedals, bends, pressure,
/// parameter changes, telemetry) allocate nothing.
#[test]
fn routed_presets_play_without_allocating() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for name in ["random_macro_shapes", "mpe_expression", "osc_stack_serial"] {
        sas_common::random::set_next_seed(5);
        RandomValues::reset_process_state();
        let mut synth = Synth::new();
        synth.load_preset(&preset(name));
        synth.prepare(44100, 120.0);
        let (mut telemetry, _reader) = Telemetry::new(synth.audio_memory());
        let (mut left, mut right) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        let mut seconds = 0.0;
        // Each block starts a note on a rotating channel and ends the one
        // started three blocks earlier, so notes overlap without piling up.
        let key = |block: usize| 30 + (block % 61 * 7 % 61) as u8;
        let channel = |block: usize| (block % 16) as u8;
        let mut play = |block: usize, synth: &mut Synth| {
            let old = block.wrapping_sub(3);
            let midi = [
                (0, [0x90 | channel(block), key(block), 100]),
                (40, [0xB0, 64, if block % 16 < 8 { 127 } else { 0 }]),
                (100, [0x80 | channel(old), key(old), 0]),
                (180, [0xE0 | channel(block), 0, (block % 128) as u8]),
                (200, [0xD0 | channel(block), (block % 100) as u8, 0]),
            ];
            let midi = if block < 3 { &midi[..2] } else { &midi[..] };
            synth.set_control("filter_1_cutoff", 40.0 + (block % 80) as f32);
            synth.set_control("macro_control_1", (block % 10) as f32 / 10.0);
            synth.render_block(&mut left, &mut right, midi, &mut seconds, 44100);
            synth.write_telemetry(&mut telemetry);
        };
        // Every voice plays at least once, so each voice's routers have
        // taken the routing the preset load made.
        for block in 0..600 {
            play(block, &mut synth);
        }
        counted(|| {
            for block in 600..1600 {
                play(block, &mut synth);
            }
        });
        assert_no_allocations();
    }
}
