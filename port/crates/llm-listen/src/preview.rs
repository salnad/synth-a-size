// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview engine: a second [`Synth`], separate from the one the host
//! plays, on a thread of its own.
//!
//! - The thread starts at the first audition. It never runs on the audio
//!   thread, and its synth is never shared. The caller (the chat worker)
//!   waits for the reply.
//! - Seeds: the thread has a seed counter of its own
//!   (`sas_common::random::use_thread_seed_counter`), so neither building
//!   a synth nor loading a patch on it moves the process's counter, which
//!   would change the seeds of every instance and wavetable built later in
//!   the session. Nothing here sets the global counter.
//! - Each audition builds a fresh synth (about 35 ms) from the thread's
//!   counter restarted at 0, so an audition never depends on earlier ones:
//!   the same patch and request give the same analysis, random modulation
//!   included.
//! - CPU is bounded: a request is at most [`MAX_HOLD_SECONDS`] +
//!   [`MAX_RELEASE_SECONDS`] of audio, rendered at 44.1 kHz in 128-sample
//!   blocks, and a render that takes longer than the budget stops there and
//!   is analysed as far as it got.
//! - Stopping (the engine dropped, or its owner's stop check, such as a
//!   user's Stop or the chat shutting down) ends a render at the next block,
//!   about 3 ms of audio. Building the synth, loading the patch and the
//!   analysis (about 0.1 s together) run to completion.
//!
//! [`MAX_HOLD_SECONDS`]: sas_llm_control::listen::MAX_HOLD_SECONDS
//! [`MAX_RELEASE_SECONDS`]: sas_llm_control::listen::MAX_RELEASE_SECONDS

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sas_common::random::use_thread_seed_counter;
use sas_engine::sas_oscillators::RandomValues;
use sas_engine::synth::Synth;
use sas_llm_control::{AuditionRequest, Auditioner};
use sas_preset::PresetState;
use serde_json::{json, Value};

use crate::analysis::analyze;

/// The preview's sample rate.
pub const SAMPLE_RATE: i32 = 44_100;
/// Frames per rendered block (the engine's largest sub-block).
const BLOCK: usize = 128;
/// The wall-clock time one render may take. A full-length preview of a
/// heavy patch (eight effects reordered) took 2.6 s in testing.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(5);

struct Job {
    state: PresetState,
    request: AuditionRequest,
    reply: mpsc::Sender<Result<Value, String>>,
}

struct Worker {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

/// The owner's stop check, asked between blocks.
type StopCheck = Arc<dyn Fn() -> bool + Send + Sync>;

/// Renders previews on a separate engine; see the module docs.
pub struct PreviewEngine {
    budget: Duration,
    /// Set when the engine is dropped.
    stop: Arc<AtomicBool>,
    stopped_by: Option<StopCheck>,
    worker: Option<Worker>,
}

impl Default for PreviewEngine {
    fn default() -> Self {
        PreviewEngine::new()
    }
}

impl PreviewEngine {
    /// An engine with the default budget. Nothing is built until the first
    /// audition.
    pub fn new() -> PreviewEngine {
        PreviewEngine::with_budget(DEFAULT_BUDGET)
    }

    pub fn with_budget(budget: Duration) -> PreviewEngine {
        PreviewEngine {
            budget,
            stop: Arc::new(AtomicBool::new(false)),
            stopped_by: None,
            worker: None,
        }
    }

    /// Also stops a render when `stop` returns true (asked between blocks):
    /// the owner's Stop button or shutdown. The render then fails with
    /// "stopped"; the engine stays usable.
    pub fn stopped_by(mut self, stop: impl Fn() -> bool + Send + Sync + 'static) -> PreviewEngine {
        self.stopped_by = Some(Arc::new(stop));
        self
    }

    /// Whether the preview thread (and its synth) exists yet.
    pub fn is_started(&self) -> bool {
        self.worker.is_some()
    }

    fn worker(&mut self) -> &mut Worker {
        let (budget, stop, stopped_by) = (self.budget, self.stop.clone(), self.stopped_by.clone());
        self.worker.get_or_insert_with(|| {
            let (jobs, receiver) = mpsc::channel::<Job>();
            let thread = std::thread::Builder::new()
                .name("synth-a-size preview".to_string())
                .spawn(move || {
                    let stopping = || {
                        stop.load(Ordering::Relaxed)
                            || stopped_by.as_ref().is_some_and(|stop| stop())
                    };
                    for job in receiver {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        // The first oscillator built in the process takes one
                        // seed for a shared table (`RandomValues`). Taking it
                        // here, from this thread's counter, before the counter
                        // restarts means every build starts the same way. (In
                        // the plugin the host's synth has already taken it.)
                        use_thread_seed_counter(0);
                        RandomValues::instance();
                        use_thread_seed_counter(0);
                        let mut synth = Synth::new();
                        let result =
                            render(&mut synth, &job.state, &job.request, budget, &stopping);
                        let _ = job.reply.send(result);
                    }
                })
                .expect("spawn the preview thread");
            Worker {
                jobs: Some(jobs),
                thread: Some(thread),
            }
        })
    }
}

impl Auditioner for PreviewEngine {
    fn audition(
        &mut self,
        state: &PresetState,
        request: &AuditionRequest,
    ) -> Result<Value, String> {
        let (reply, result) = mpsc::channel();
        let job = Job {
            state: state.clone(),
            request: request.clone(),
            reply,
        };
        let worker = self.worker();
        let sent = worker
            .jobs
            .as_ref()
            .is_some_and(|jobs| jobs.send(job).is_ok());
        if !sent {
            return Err("the preview engine stopped".to_string());
        }
        result
            .recv()
            .unwrap_or_else(|_| Err("the preview engine stopped".to_string()))
    }
}

impl Drop for PreviewEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut worker) = self.worker.take() {
            worker.jobs = None;
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

/// Plays `request` on `state` and analyses it.
fn render(
    synth: &mut Synth,
    state: &PresetState,
    request: &AuditionRequest,
    budget: Duration,
    stopping: &dyn Fn() -> bool,
) -> Result<Value, String> {
    let started = Instant::now();
    synth.load_preset(state);
    synth.prepare(SAMPLE_RATE, 120.0);

    let rate = SAMPLE_RATE as f64;
    let released_at = (request.hold * rate).round() as usize;
    let frames = (request.total_seconds() * rate).ceil() as usize;
    let velocity = ((request.velocity * 127.0).round() as u8).clamp(1, 127);
    let mut events: Vec<(usize, [u8; 3])> = Vec::new();
    for &note in &request.notes {
        events.push((0, [0x90, note, velocity]));
    }
    for &note in &request.notes {
        events.push((released_at, [0x80, note, 0]));
    }

    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];
    let mut seconds = 0.0;
    let mut start = 0;
    let mut midi = Vec::new();
    while start < frames {
        if stopping() {
            return Err("the preview was stopped".to_string());
        }
        if started.elapsed() > budget {
            break;
        }
        let n = BLOCK.min(frames - start);
        midi.clear();
        midi.extend(
            events
                .iter()
                .filter(|(at, _)| *at >= start && *at < start + n)
                .map(|&(at, message)| (at - start, message)),
        );
        synth.render_block(
            &mut left[start..start + n],
            &mut right[start..start + n],
            &midi,
            &mut seconds,
            SAMPLE_RATE,
        );
        start += n;
    }

    let analysis = analyze(&left[..start], &right[..start], rate, released_at);
    let mut value = serde_json::to_value(&analysis).map_err(|error| error.to_string())?;
    value["notes"] = json!(request.notes);
    value["velocity"] = json!((request.velocity as f64 * 100.0).round() / 100.0);
    if start < frames {
        value["cut_short"] = json!(format!(
            "rendered {:.2} of {:.2} s within the preview's CPU budget",
            start as f64 / rate,
            frames as f64 / rate
        ));
    }
    Ok(value)
}
