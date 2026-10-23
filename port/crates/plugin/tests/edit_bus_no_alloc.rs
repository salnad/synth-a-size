// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin's whole audio callback (`BlockProcessor::process`) never
//! allocates or frees while an editor thread sends LFO shapes, remap curves,
//! wavetables and samples over the edit bus, another plays the on-screen
//! keyboard, and the host moves parameters, bypasses and plays notes. Restores and modulation connections are covered
//! by `sas-engine`'s `edit_bus_no_alloc` test, which explains why their
//! whole-document load and graph rewiring are left out.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nice_plug::params::InternalParamMut;
use nice_plug::prelude::NoteEvent;
use sas_common::LineGenerator;
use sas_engine::edit_bus::{EditError, EditHandle};
use sas_engine::keyboard::KeyboardHandle;
use sas_engine::sas_oscillators::RandomValues;
use sas_plugin::params::ControlParam;
use sas_plugin::{state, SoundEngineHost, SynthASize, SynthPlugin, Transport};
use sas_preset::{InitContent, PresetState, SampleState};

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

fn retry(mut edit: impl FnMut() -> Result<(), EditError>) {
    for _ in 0..500 {
        match edit() {
            Err(EditError::QueueFull) => std::thread::sleep(Duration::from_millis(1)),
            result => return result.unwrap(),
        }
    }
    panic!("the queue stayed full");
}

fn editor(edits: EditHandle, stop: Arc<AtomicBool>) -> usize {
    let mut preset = PresetState::new(&InitContent { sample_seed: 0 });
    preset
        .load_file(&corpus("presets/line_wavetable_fold.vital"))
        .unwrap();
    let wavetable = preset.wavetables[0].clone();
    let mut count = 0;
    while !stop.load(Ordering::Relaxed) {
        let mut line = LineGenerator::default();
        if count % 2 == 0 {
            line.init_square();
        } else {
            line.init_sin();
        }
        line.set_name(&format!("drag {count}"));
        retry(|| edits.set_lfo_shape(count % 8, line.clone()));
        retry(|| edits.set_line_mapping(count % 64, line.clone()));
        if count % 30 == 0 {
            retry(|| edits.set_wavetable(count % 3, wavetable.clone()));
        }
        if count % 45 == 0 {
            retry(|| {
                edits.set_sample(SampleState {
                    name: format!("sample {count}"),
                    sample_rate: 48000,
                    left: (0..4096)
                        .map(|i| ((i * 7 + count) % 101) as f32 / 101.0 - 0.5)
                        .collect(),
                    right: None,
                })
            });
        }
        count += 1;
        std::thread::sleep(Duration::from_micros(300));
    }
    count
}

fn keyboard(keyboard: KeyboardHandle, stop: Arc<AtomicBool>) -> usize {
    let mut count = 0;
    while !stop.load(Ordering::Relaxed) {
        let note = 50 + (count % 24) as u8;
        keyboard.note_on(note, 0.75);
        std::thread::sleep(Duration::from_micros(700));
        keyboard.note_off(note);
        count += 1;
    }
    count
}

fn set_param(plugin: &SynthASize, name: &str, value: f32) {
    let control = plugin.synth_params().control(name).unwrap();
    // SAFETY: as a host sets a parameter between blocks.
    unsafe {
        match &control.param {
            ControlParam::Float(param) => {
                param._internal_set_plain_value(value);
            }
            ControlParam::Stepped(param) => {
                param._internal_set_plain_value(value.round() as i32);
            }
            ControlParam::Bypass(param) => {
                param._internal_set_plain_value(value != 0.0);
            }
        }
    }
}

#[test]
fn the_audio_callback_never_allocates() {
    sas_common::random::set_next_seed(11);
    RandomValues::reset_process_state();
    let mut plugin: SynthASize = SynthPlugin::new(SoundEngineHost::new());
    plugin.synth_params().restore_state(
        state::plugin_state_from_file(&corpus("presets/unison_pad_fx.vital")).unwrap(),
    );
    let params = plugin.synth_params().clone();
    plugin.processor_mut().activate(&params, 48000.0);
    let transport = Transport {
        tempo: Some(128.0),
        playing: true,
        pos_beats: Some(0.0),
    };
    let mut left = vec![0.0f32; 512];
    let mut right = vec![0.0f32; 512];
    // The first block loads the restored preset and prepares the engine.
    let mut warm_up: Vec<NoteEvent<()>> = vec![NoteEvent::from_midi(0, &[0x90, 48, 100]).unwrap()];
    for _ in 0..100 {
        plugin
            .processor_mut()
            .process(&params, &mut left, &mut right, &transport, || warm_up.pop());
    }

    let stop = Arc::new(AtomicBool::new(false));
    let worker = {
        let edits = plugin.edit_handle();
        let stop = stop.clone();
        std::thread::spawn(move || editor(edits, stop))
    };
    let player = {
        let handle = plugin.keyboard_handle();
        let stop = stop.clone();
        std::thread::spawn(move || keyboard(handle, stop))
    };
    let mut events: Vec<NoteEvent<()>> = Vec::with_capacity(8);
    for block in 0..3000usize {
        set_param(&plugin, "filter_1_cutoff", 30.0 + (block % 90) as f32);
        set_param(&plugin, "reverb_dry_wet", (block % 10) as f32 / 10.0);
        set_param(&plugin, "bypass", if block % 500 < 20 { 1.0 } else { 0.0 });
        let size = [512, 128, 77, 300][block % 4];
        events.clear();
        let note = 40 + (block % 30) as u8;
        events.push(NoteEvent::from_midi(5, &[0x90, note, 80]).unwrap());
        events.push(NoteEvent::from_midi(60, &[0x80, 40 + ((block + 12) % 30) as u8, 0]).unwrap());
        events.reverse();
        counted(|| {
            plugin.processor_mut().process(
                &params,
                &mut left[..size],
                &mut right[..size],
                &transport,
                || events.pop(),
            );
        });
    }
    stop.store(true, Ordering::Relaxed);
    let edits_made = worker.join().unwrap();
    let keys_played = player.join().unwrap();
    assert!(
        keys_played > 100,
        "the keyboard only played {keys_played} notes"
    );
    plugin.edit_handle().collect_retired();
    assert!(
        edits_made > 100,
        "the editor only made {edits_made} rounds of edits"
    );

    let allocations = ALLOCATIONS.load(Ordering::Relaxed);
    let sites = SITES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(
        allocations,
        0,
        "the audio callback allocated or freed {allocations} times, at:\n{}",
        sites
            .iter()
            .map(|(site, count)| format!("{count:8}  {site}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
