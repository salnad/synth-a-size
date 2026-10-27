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
//!
//! Dropping the [`Resynthesizer`] stops a running render at its next block
//! and skips queued ones. A job that panics loses only its own result.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
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

/// Gives the calling thread a seed counter of its own, restarted at 0, after
/// making sure the process's shared oscillator table (`RandomValues`) exists.
/// The first oscillator built in the process creates that table and takes
/// one seed for it; taking it before the restart means a render on this
/// thread draws the same seeds whichever thread built a synth first, and
/// the process's counter never moves.
pub fn isolate_seeds() {
    use_thread_seed_counter(0);
    RandomValues::instance();
    use_thread_seed_counter(0);
}

/// `renderAudioForResynthesis(data, samples, note)` on a fresh synth playing
/// `state` at `bpm`: a second of running in, then the note for
/// `RESYNTHESIZE_SECONDS`, the left channel normalized to its peak.
/// `stopping` is asked before each block; `None` when it said stop.
pub fn render(
    state: &PresetState,
    sample_rate: i32,
    bpm: f64,
    stopping: &dyn Fn() -> bool,
) -> Option<Vec<f32>> {
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
        if stopping() {
            return None;
        }
        synth.correct_to_time(current_time);
        current_time += BUFFER_SIZE as f64 * sample_time;
        synth.process(&mut left, &mut right);
    }

    synth.note_on(RESYNTHESIZE_NOTE, VELOCITY, 0, 0);
    let mut data = vec![0.0f32; samples];
    let mut max_value = 0.01f32;
    for start in (0..samples).step_by(BUFFER_SIZE) {
        if stopping() {
            return None;
        }
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
    Some(data)
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

/// One resynthesis: the patch, where its result goes, and how to render it.
pub struct Job {
    pub oscillator: usize,
    pub state: PresetState,
    pub sample_rate: i32,
    pub bpm: f64,
}

/// What the worker does with a job, given the stop check.
type Runner = Box<dyn Fn(Job, &dyn Fn() -> bool) + Send>;

struct Worker {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

/// The thread resyntheses run on, started by the first one.
pub struct Resynthesizer {
    edits: EditHandle,
    worker: Mutex<Option<Worker>>,
    /// Set on drop: a running render stops at its next block, queued jobs
    /// are skipped.
    stop: Arc<AtomicBool>,
    /// Jobs queued or running, for [`wait_idle`](Self::wait_idle).
    busy: Arc<(Mutex<usize>, Condvar)>,
    /// Replaces the real job in tests.
    runner: Mutex<Option<Runner>>,
}

impl Resynthesizer {
    pub fn new(edits: EditHandle) -> Resynthesizer {
        Resynthesizer {
            edits,
            worker: Mutex::new(None),
            stop: Arc::new(AtomicBool::new(false)),
            busy: Arc::new((Mutex::new(0), Condvar::new())),
            runner: Mutex::new(None),
        }
    }

    /// A resynthesizer whose worker runs `runner` for each job instead of
    /// the render (tests of the worker itself).
    pub fn with_runner(
        edits: EditHandle,
        runner: impl Fn(Job, &dyn Fn() -> bool) + Send + 'static,
    ) -> Resynthesizer {
        let resynthesizer = Resynthesizer::new(edits);
        *resynthesizer.runner.lock().unwrap() = Some(Box::new(runner));
        resynthesizer
    }

    /// Renders `job.state` and loads the result into `job.oscillator`, on
    /// the worker thread. Returns at once; the oscillator changes when the
    /// render is done (about half a second of work), through the edit bus.
    pub fn start(&self, job: Job) {
        let mut worker = self.worker.lock().unwrap();
        let worker = worker.get_or_insert_with(|| self.spawn());
        *self.busy.0.lock().unwrap() += 1;
        let sent = worker
            .jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send(job).is_ok());
        if !sent {
            self.finished();
        }
    }

    fn spawn(&self) -> Worker {
        let (jobs, receiver) = mpsc::channel::<Job>();
        let edits = self.edits.clone();
        let stop = self.stop.clone();
        let busy = self.busy.clone();
        let runner = self.runner.lock().unwrap().take();
        let thread = std::thread::Builder::new()
            .name("synth-a-size resynthesis".to_string())
            .spawn(move || {
                let stopping = || stop.load(Ordering::Relaxed);
                for job in receiver {
                    if !stopping() {
                        // A panicking job loses its own result only; the
                        // worker goes on to the next.
                        let _ = catch_unwind(AssertUnwindSafe(|| match &runner {
                            Some(runner) => runner(job, &stopping),
                            None => resynthesize(&edits, job, &stopping),
                        }));
                    }
                    let (count, done) = &*busy;
                    *count.lock().unwrap() -= 1;
                    done.notify_all();
                }
            })
            .expect("spawn the resynthesis thread");
        Worker {
            jobs: Some(jobs),
            thread: Some(thread),
        }
    }

    fn finished(&self) {
        let (count, done) = &*self.busy;
        *count.lock().unwrap() -= 1;
        done.notify_all();
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
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut worker) = self.worker.get_mut().unwrap().take() {
            worker.jobs = None;
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

/// One job, on the worker thread.
fn resynthesize(edits: &EditHandle, job: Job, stopping: &dyn Fn() -> bool) {
    isolate_seeds();
    let Some(audio) = render(&job.state, job.sample_rate, job.bpm, stopping) else {
        return;
    };
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
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, Instant};

    use super::*;
    use sas_engine::document::PresetDocument;
    use sas_engine::edit_bus::EditBus;
    use sas_preset::InitContent;

    fn init_state() -> PresetState {
        PresetState::new(&InitContent { sample_seed: 0 })
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn the_render_is_four_seconds_normalized_to_its_peak() {
        let state = init_state();
        isolate_seeds();
        let audio = render(&state, 44_100, 120.0, &never).unwrap();
        assert_eq!(audio.len(), 176_400);
        let peak = audio
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!((peak - 1.0).abs() < 1e-6, "{peak}");
        // The init patch's saw sounds from the note on.
        assert!(audio[..4410].iter().any(|sample| sample.abs() > 0.1));
        // The same patch from the same seeds renders the same, as each job
        // on the worker does.
        isolate_seeds();
        assert_eq!(render(&state, 44_100, 120.0, &never).unwrap(), audio);
    }

    #[test]
    fn the_creator_is_one_pitched_file_source_named_resynthesize() {
        let state = init_state();
        isolate_seeds();
        let audio = render(&state, 44_100, 120.0, &never).unwrap();
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

    #[test]
    fn a_render_stops_at_the_next_block() {
        isolate_seeds();
        let blocks = AtomicUsize::new(0);
        let stop_after_ten = || blocks.fetch_add(1, Ordering::Relaxed) >= 10;
        assert!(render(&init_state(), 44_100, 120.0, &stop_after_ten).is_none());
        assert_eq!(blocks.load(Ordering::Relaxed), 11);
    }

    fn edits() -> EditHandle {
        let (edits, _consumer) =
            EditBus::new(PresetDocument::init(&InitContent { sample_seed: 0 }));
        edits
    }

    fn job(oscillator: usize) -> Job {
        Job {
            oscillator,
            state: init_state(),
            sample_rate: 44_100,
            bpm: 120.0,
        }
    }

    #[test]
    fn a_panicking_job_leaves_the_worker_running() {
        let ran = Arc::new(Mutex::new(Vec::new()));
        let log = ran.clone();
        let resynthesizer = Resynthesizer::with_runner(edits(), move |job, _| {
            if job.oscillator == 0 {
                panic!("a job failed");
            }
            log.lock().unwrap().push(job.oscillator);
        });
        resynthesizer.start(job(0));
        resynthesizer.start(job(1));
        resynthesizer.wait_idle();
        assert_eq!(*ran.lock().unwrap(), [1]);
    }

    #[test]
    fn dropping_stops_the_running_job_and_skips_queued_ones() {
        let ran = Arc::new(AtomicUsize::new(0));
        let count = ran.clone();
        let resynthesizer = Resynthesizer::with_runner(edits(), move |_, stopping| {
            count.fetch_add(1, Ordering::Relaxed);
            // A long render that only ends when told to stop.
            while !stopping() {
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        for oscillator in 0..3 {
            resynthesizer.start(job(oscillator));
        }
        while ran.load(Ordering::Relaxed) == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        let dropped = Instant::now();
        drop(resynthesizer);
        assert!(dropped.elapsed() < Duration::from_secs(2));
        // The running job stopped; the two queued never started.
        assert_eq!(ran.load(Ordering::Relaxed), 1);
    }
}
