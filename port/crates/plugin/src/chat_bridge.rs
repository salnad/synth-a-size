// SPDX-License-Identifier: GPL-3.0-or-later

//! How the chat worker reaches the [`LiveSession`](crate::live::LiveSession)
//! on the GUI thread.
//!
//! The model and the tool loop run on a worker thread (never the audio or
//! GUI thread), but the session uses nice-plug's `GuiContext`, which only
//! works on the GUI thread. [`RemoteSession`] is the worker's
//! `SynthSession`: each call is queued on the [`ChatBridge`] and run by the
//! editor's frame loop ([`ChatBridge::run_pending`]), and the worker waits
//! for the result.
//!
//! Each call has one atomic state: queued, running, cancelled or done. The
//! GUI thread runs a call only if it moves it from queued to running; a
//! worker whose wait times out cancels it only if it is still queued. So a
//! call either runs and its result reaches the worker (which waits for a
//! call already running), or never runs and the worker gets an error. An
//! edit can't be applied while the worker reports it failed.
//!
//! [`ChatBridge::close`] ends a bridge for good (the worker retired or the
//! plugin closing): queued calls are dropped, their waiting worker wakes at
//! once with an error, and later calls fail without queueing.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sas_llm_control::{Applied, Edit, Error, OutsideChange, SynthSession};
use sas_preset::PresetState;

const QUEUED: u8 = 0;
const RUNNING: u8 = 1;
const CANCELLED: u8 = 2;
const DONE: u8 = 3;

/// Retries of an edit the synth refused as busy (a full edit queue), and the
/// wait between them, on the worker. The queue only fills while the audio
/// thread is not processing.
const BUSY_RETRIES: usize = 50;
const BUSY_RETRY_DELAY: Duration = Duration::from_millis(10);

type Run = Box<dyn FnOnce(&mut dyn SynthSession) + Send>;

struct Job {
    state: Arc<AtomicU8>,
    run: Run,
}

/// Calls from the chat worker waiting for the GUI thread.
#[derive(Default)]
pub struct ChatBridge {
    jobs: Mutex<VecDeque<Job>>,
    /// Asks the editor for a frame (egui's `request_repaint`), so a queued
    /// call runs promptly.
    waker: Mutex<Option<Box<dyn Fn() + Send>>>,
    /// Set under the `jobs` lock, so no call is queued after `close`.
    closed: AtomicBool,
}

fn closed_error() -> Error {
    Error::Invalid("the chat was closed".to_string())
}

impl ChatBridge {
    pub fn new() -> Arc<ChatBridge> {
        Arc::new(ChatBridge::default())
    }

    /// Sets how to wake the GUI thread when a call is queued.
    pub fn set_waker(&self, waker: impl Fn() + Send + 'static) {
        *self.waker.lock().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(waker));
    }

    /// Ends the bridge: drops the queued calls (their workers wake at once
    /// with an error) and refuses later ones.
    pub fn close(&self) {
        let jobs: Vec<Job> = {
            let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            self.closed.store(true, Ordering::Release);
            jobs.drain(..).collect()
        };
        for job in jobs {
            let _ =
                job.state
                    .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Runs the queued calls on `session` (the GUI thread, once per frame).
    /// Calls their worker gave up on are dropped. Returns the calls run.
    pub fn run_pending(&self, session: &mut dyn SynthSession) -> usize {
        let jobs: Vec<Job> = self
            .jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain(..)
            .collect();
        let mut ran = 0;
        for job in jobs {
            if job
                .state
                .compare_exchange(QUEUED, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                (job.run)(session);
                job.state.store(DONE, Ordering::Release);
                ran += 1;
            }
        }
        ran
    }

    /// The worker's session, whose calls fail if the GUI thread has not
    /// started them within `timeout`.
    pub fn remote(self: &Arc<Self>, timeout: Duration) -> RemoteSession {
        RemoteSession {
            bridge: self.clone(),
            timeout,
        }
    }

    fn call<R: Send + 'static>(
        &self,
        timeout: Duration,
        run: impl FnOnce(&mut dyn SynthSession) -> R + Send + 'static,
    ) -> Result<R, Error> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let state = Arc::new(AtomicU8::new(QUEUED));
        {
            let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            if self.is_closed() {
                return Err(closed_error());
            }
            jobs.push_back(Job {
                state: state.clone(),
                run: Box::new(move |session| {
                    let _ = sender.send(run(session));
                }),
            });
        }
        if let Some(wake) = &*self.waker.lock().unwrap_or_else(|p| p.into_inner()) {
            wake();
        }
        match receiver.recv_timeout(timeout) {
            Ok(result) => Ok(result),
            // The call was dropped unrun: the bridge closed.
            Err(RecvTimeoutError::Disconnected) => Err(closed_error()),
            Err(RecvTimeoutError::Timeout) => {
                if state
                    .compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    return Err(Error::Invalid(
                        "the synth did not answer; is the plugin window open?".to_string(),
                    ));
                }
                // Already running (or just done): its result is coming.
                receiver.recv().map_err(|_| {
                    Error::Invalid("the synth stopped while running the edit".to_string())
                })
            }
        }
    }

    /// [`call`](Self::call), retrying on the worker while the synth reports
    /// [`Error::Busy`].
    fn call_retrying<R: Send + 'static>(
        &self,
        timeout: Duration,
        run: impl Fn(&mut dyn SynthSession) -> Result<R, Error> + Send + Sync + 'static,
    ) -> Result<R, Error> {
        let run = Arc::new(run);
        for _ in 0..BUSY_RETRIES {
            let attempt = run.clone();
            match self.call(timeout, move |session| attempt(session))? {
                Err(Error::Busy(_)) => std::thread::sleep(BUSY_RETRY_DELAY),
                result => return result,
            }
        }
        Err(Error::Busy(
            "the synth is not processing audio, so the edit cannot reach it".to_string(),
        ))
    }
}

/// The chat worker's session: every call runs on the GUI thread's
/// [`LiveSession`](crate::live::LiveSession).
pub struct RemoteSession {
    bridge: Arc<ChatBridge>,
    timeout: Duration,
}

impl SynthSession for RemoteSession {
    fn snapshot(&self) -> Result<PresetState, Error> {
        self.bridge
            .call(self.timeout, |session| session.snapshot())?
    }

    fn control(&self, name: &str) -> Option<f32> {
        let name = name.to_string();
        self.bridge
            .call(self.timeout, move |session| session.control(&name))
            .ok()
            .flatten()
    }

    fn apply(&mut self, edit: &Edit) -> Result<Applied, Error> {
        let edit = edit.clone();
        self.bridge
            .call_retrying(self.timeout, move |session| session.apply(&edit))
    }

    fn restore(&mut self, state: &PresetState) -> Result<(), Error> {
        let state = state.clone();
        self.bridge
            .call_retrying(self.timeout, move |session| session.restore(&state))
    }

    fn take_outside_changes(&mut self) -> Vec<OutsideChange> {
        self.bridge
            .call(self.timeout, |session| session.take_outside_changes())
            .unwrap_or_default()
    }

    fn take_replaced(&mut self) -> bool {
        self.bridge
            .call(self.timeout, |session| session.take_replaced())
            .unwrap_or(false)
    }
}
