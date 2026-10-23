// SPDX-License-Identifier: GPL-3.0-or-later

//! The preview engine: a second [`Synth`], separate from the one the host
//! plays, on a thread of its own.
//!
//! - The thread starts, and builds its synth, at the first audition, and
//!   keeps both for later ones. Building takes seeds from the process's
//!   seed counter, as opening another instance does upstream; nothing here
//!   ever sets the counter (`set_next_seed`), which would change the sound
//!   of every instance built afterwards.
//! - It never runs on the audio thread, and the synth is never shared with
//!   it. The caller (the chat worker) waits for the reply.
//! - CPU is bounded: a request is at most [`MAX_HOLD_SECONDS`] +
//!   [`MAX_RELEASE_SECONDS`] of audio, rendered at 44.1 kHz in 128-sample
//!   blocks, and a render that takes longer than the budget stops there and
//!   is analysed as far as it got. Dropping the engine stops a render
//!   within one block.
//!
//! [`MAX_HOLD_SECONDS`]: sas_llm_control::listen::MAX_HOLD_SECONDS
//! [`MAX_RELEASE_SECONDS`]: sas_llm_control::listen::MAX_RELEASE_SECONDS

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sas_engine::synth::Synth;
use sas_llm_control::{AuditionRequest, Auditioner};
use sas_preset::PresetState;
use serde_json::{json, Value};

use crate::analysis::analyze;

/// The preview's sample rate.
pub const SAMPLE_RATE: i32 = 44_100;
/// Frames per rendered block (the engine's largest sub-block).
const BLOCK: usize = 128;
/// The wall-clock time one render may take.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(3);

struct Job {
    state: PresetState,
    request: AuditionRequest,
    reply: mpsc::Sender<Result<Value, String>>,
}

struct Worker {
    jobs: Option<mpsc::Sender<Job>>,
    thread: Option<JoinHandle<()>>,
}

/// Renders previews on a separate engine; see the module docs.
pub struct PreviewEngine {
    budget: Duration,
    stop: Arc<AtomicBool>,
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
            worker: None,
        }
    }

    /// Also stops on `stop` (the chat worker's shutdown flag), so a render in
    /// progress ends within one block when its owner shuts down.
    pub fn stopped_by(mut self, stop: Arc<AtomicBool>) -> PreviewEngine {
        self.stop = stop;
        self
    }

    /// Whether the preview thread (and its synth) exists yet.
    pub fn is_started(&self) -> bool {
        self.worker.is_some()
    }

    fn worker(&mut self) -> &mut Worker {
        let (budget, stop) = (self.budget, self.stop.clone());
        self.worker.get_or_insert_with(|| {
            let (jobs, receiver) = mpsc::channel::<Job>();
            let thread = std::thread::Builder::new()
                .name("synth-a-size preview".to_string())
                .spawn(move || {
                    let mut synth: Option<Synth> = None;
                    for job in receiver {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let synth = synth.get_or_insert_with(Synth::new);
                        let result = render(synth, &job.state, &job.request, budget, &stop);
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
        let sent = worker.jobs.as_ref().is_some_and(|jobs| jobs.send(job).is_ok());
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
    stop: &AtomicBool,
) -> Result<Value, String> {
    let started = Instant::now();
    // Loading resets voices and effect tails (`allSoundsOff`).
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
        if stop.load(Ordering::Relaxed) {
            synth.all_sounds_off();
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
    synth.all_sounds_off();

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
