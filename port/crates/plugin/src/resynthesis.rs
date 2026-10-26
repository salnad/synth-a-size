// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! "Resynthesize Preset to Wavetable": the current patch rendered and turned
//! into a wavetable for one oscillator.
//!
//! Ports `SynthBase::renderAudioForResynthesis`
//! (`vital/src/common/synth_base.cpp`) and
//! `WavetableEditSection::resynthesizeToWavetable`
//! (`vital/src/interface/editor_sections/wavetable_edit_section.cpp`).
//!
//! Upstream locks the playing engine and renders on the GUI thread. Here a
//! fresh synth built from the patch renders on a thread of its own, with a
//! seed counter of its own (`use_thread_seed_counter`), so neither the
//! audio thread nor the process's seed counter is touched. The result
//! reaches the oscillator through the edit bus like any wavetable edit
//! (D22 records how the fresh synth's render can differ).

use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use sas_common::random::use_thread_seed_counter;
use sas_engine::edit_bus::EditHandle;
use sas_engine::sas_dsp_core::sas_simd::utils;
use sas_engine::sas_oscillators::sas_wavetable::creator::{
    AudioFileLoadStyle, WavetableComponent, WavetableCreator,
};
use sas_engine::sas_oscillators::RandomValues;
use sas_engine::synth::Synth;
use sas_preset::PresetState;

/// `kResynthesizeTime`, in seconds.
pub const RESYNTHESIZE_SECONDS: f32 = 4.0;
/// `kResynthesizeNote`.
pub const RESYNTHESIZE_NOTE: i32 = 16;
/// `renderAudioForResynthesis`'s note velocity.
const VELOCITY: f32 = 0.7;
/// `kPreProcessSamples`: the engine runs this long before the note.
const PRE_PROCESS_SAMPLES: usize = 44_100;
/// `kBufferSize`.
const BUFFER_SIZE: usize = 64;

/// `renderAudioForResynthesis(data, samples, note)` on a fresh synth playing
/// `state` at `bpm`: a second of running in, then the note for
/// `RESYNTHESIZE_SECONDS`, the left channel normalized to its peak.
pub fn render(state: &PresetState, sample_rate: i32, bpm: f64) -> Vec<f32> {
    let samples = (sample_rate as f32 * RESYNTHESIZE_SECONDS) as usize;
    let mut synth = Synth::new();
    synth.load_preset(state);
    synth.prepare(sample_rate, bpm);

    let sample_time = 1.0 / sample_rate as f64;
    let mut current_time = -(PRE_PROCESS_SAMPLES as f64) * sample_time;
    let mut left = [0.0f32; BUFFER_SIZE];
    let mut right = [0.0f32; BUFFER_SIZE];

    synth.all_sounds_off();
    for _ in (0..PRE_PROCESS_SAMPLES).step_by(BUFFER_SIZE) {
        synth.correct_to_time(current_time);
        current_time += BUFFER_SIZE as f64 * sample_time;
        synth.process(&mut left, &mut right);
    }

    synth.note_on(RESYNTHESIZE_NOTE, VELOCITY, 0, 0);
    let mut data = vec![0.0f32; samples];
    let mut max_value = 0.01f32;
    for start in (0..samples).step_by(BUFFER_SIZE) {
        let num_samples = (samples - start).min(BUFFER_SIZE);
        synth.correct_to_time(current_time);
        current_time += num_samples as f64 * sample_time;
        synth.process(&mut left[..num_samples], &mut right[..num_samples]);
        for (i, &sample) in left[..num_samples].iter().enumerate() {
            data[start + i] = sample;
            max_value = max_value.max(sample.abs());
        }
    }

    let scale = 1.0 / max_value;
    for sample in &mut data {
        *sample *= scale;
    }
    synth.all_sounds_off();
    data
}

/// `resynthesizeToWavetable`'s creator: `audio` loaded pitched with wave
/// blending, named "Resynthesize", its window one period of the note.
/// `creator` is the oscillator's, cleared and reused as upstream does.
/// `None` when no pitch can be found in the audio.
pub fn creator_from_audio(
    mut creator: WavetableCreator,
    audio: &[f32],
    sample_rate: i32,
) -> Option<WavetableCreator> {
    creator.clear();
    // initFromAudioFile(.., kPitched, kWaveBlend): pitched audio always
    // blends waves.
    creator
        .init_from_audio_file(audio, sample_rate, AudioFileLoadStyle::Pitched, 0)
        .ok()?;
    creator.name = "Resynthesize".to_string();
    let window = sample_rate as f32 / utils::midi_note_to_frequency(RESYNTHESIZE_NOTE as f32);
    if let Some(WavetableComponent::FileSource(file_source)) = creator
        .groups
        .first_mut()
        .and_then(|group| group.components.first_mut())
    {
        file_source.set_window_size(window as f64);
    }
    Some(creator)
}

struct Job {
    oscillator: usize,
    state: PresetState,
    sample_rate: i32,
    bpm: f64,
}

/// The thread resyntheses run on, started by the first one.
pub struct Resynthesizer {
    edits: EditHandle,
    worker: Mutex<Option<(mpsc::Sender<Job>, JoinHandle<()>)>>,
    /// Resyntheses queued or running, for tests to wait on.
    busy: Arc<(Mutex<usize>, std::sync::Condvar)>,
}

impl Resynthesizer {
    pub fn new(edits: EditHandle) -> Resynthesizer {
        Resynthesizer {
            edits,
            worker: Mutex::new(None),
            busy: Arc::new((Mutex::new(0), std::sync::Condvar::new())),
        }
    }

    /// Renders `state` at `sample_rate` and `bpm` (the host's) and loads
    /// the result into `oscillator`, on the worker thread. Returns at once;
    /// the oscillator changes when the render is done (about a second of
    /// work), through the edit bus.
    pub fn start(&self, oscillator: usize, state: PresetState, sample_rate: i32, bpm: f64) {
        let mut worker = self.worker.lock().unwrap();
        let (jobs, _) = worker.get_or_insert_with(|| {
            let (jobs, receiver) = mpsc::channel::<Job>();
            let edits = self.edits.clone();
            let busy = self.busy.clone();
            let thread = std::thread::Builder::new()
                .name("synth-a-size resynthesis".to_string())
                .spawn(move || {
                    for job in receiver {
                        resynthesize(&edits, job);
                        let (count, done) = &*busy;
                        *count.lock().unwrap() -= 1;
                        done.notify_all();
                    }
                })
                .expect("spawn the resynthesis thread");
            (jobs, thread)
        });
        *self.busy.0.lock().unwrap() += 1;
        let job = Job {
            oscillator,
            state,
            sample_rate,
            bpm,
        };
        if jobs.send(job).is_err() {
            *self.busy.0.lock().unwrap() -= 1;
        }
    }

    /// Waits until every started resynthesis has finished.
    pub fn wait_idle(&self) {
        let (count, done) = &*self.busy;
        let mut count = count.lock().unwrap();
        while *count > 0 {
            count = done.wait(count).unwrap();
        }
    }
}

impl Drop for Resynthesizer {
    fn drop(&mut self) {
        if let Some((jobs, thread)) = self.worker.get_mut().unwrap().take() {
            drop(jobs);
            let _ = thread.join();
        }
    }
}

/// One job, on the worker thread.
fn resynthesize(edits: &EditHandle, job: Job) {
    // The first oscillator built in the process takes one seed for a shared
    // table (`RandomValues`); taking it from this thread's counter keeps
    // every build the same. Then each resynthesis starts from 0, so it
    // depends only on the patch, and the process's counter never moves.
    use_thread_seed_counter(0);
    RandomValues::instance();
    use_thread_seed_counter(0);

    let audio = render(&job.state, job.sample_rate, job.bpm);
    let Some(base) = job.state.wavetables.get(job.oscillator).cloned() else {
        return;
    };
    if let Some(creator) = creator_from_audio(base, &audio, job.sample_rate) {
        // A full queue drops the result, like the other editor edits.
        let _ = edits.set_wavetable(job.oscillator, creator);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sas_preset::InitContent;

    #[test]
    fn the_render_is_four_seconds_normalized_to_its_peak() {
        let state = PresetState::new(&InitContent { sample_seed: 0 });
        // As the worker does: the shared table first, then the counter from
        // 0, so the render doesn't depend on which test built a synth first.
        RandomValues::instance();
        use_thread_seed_counter(0);
        let audio = render(&state, 44_100, 120.0);
        assert_eq!(audio.len(), 176_400);
        let peak = audio
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!((peak - 1.0).abs() < 1e-6, "{peak}");
        // The init patch's saw sounds from the note on.
        assert!(audio[..4410].iter().any(|sample| sample.abs() > 0.1));
        // The same patch from the same seeds renders the same, as each job
        // on the worker does.
        use_thread_seed_counter(0);
        assert_eq!(render(&state, 44_100, 120.0), audio);
    }

    #[test]
    fn the_creator_is_one_pitched_file_source_named_resynthesize() {
        let state = PresetState::new(&InitContent { sample_seed: 0 });
        RandomValues::instance();
        use_thread_seed_counter(0);
        let audio = render(&state, 44_100, 120.0);
        let creator = creator_from_audio(state.wavetables[0].clone(), &audio, 44_100).unwrap();
        assert_eq!(creator.name, "Resynthesize");
        assert_eq!(creator.groups.len(), 1);
        let Some(WavetableComponent::FileSource(file_source)) =
            creator.groups[0].components.first()
        else {
            panic!("not a file source");
        };
        let window = 44_100.0 / utils::midi_note_to_frequency(16.0);
        assert_eq!(file_source.window_size, window as f64);
    }
}
