// SPDX-License-Identifier: GPL-3.0-or-later

//! How the chat worker reaches the [`LiveSession`](crate::live::LiveSession)
//! on the GUI thread.
//!
//! The model and the tool loop run on a worker thread (never the audio or
//! GUI thread), but the session uses nice-plug's `GuiContext`, which only
//! works on the GUI thread. [`RemoteSession`] is the worker's
//! `SynthSession`: each call is queued on the [`ChatBridge`] and run by the
//! editor's frame loop ([`ChatBridge::run_pending`]), and the worker waits
//! for the result. A call not run within its deadline (the editor closed)
//! fails, and is dropped rather than run late.

use std::collections::VecDeque;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sas_llm_control::{Applied, Edit, Error, OutsideChange, SynthSession};
use sas_preset::PresetState;

type Run = Box<dyn FnOnce(&mut dyn SynthSession) + Send>;

struct Job {
    deadline: Instant,
    run: Run,
}

/// Calls from the chat worker waiting for the GUI thread.
#[derive(Default)]
pub struct ChatBridge {
    jobs: Mutex<VecDeque<Job>>,
    /// Asks the editor for a frame (egui's `request_repaint`), so a queued
    /// call runs promptly.
    waker: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl ChatBridge {
    pub fn new() -> Arc<ChatBridge> {
        Arc::new(ChatBridge::default())
    }

    /// Sets how to wake the GUI thread when a call is queued.
    pub fn set_waker(&self, waker: impl Fn() + Send + 'static) {
        *self.waker.lock().unwrap_or_else(|p| p.into_inner()) = Some(Box::new(waker));
    }

    /// Runs the queued calls on `session` (the GUI thread, once per frame).
    /// Calls past their deadline are dropped. Returns the calls run.
    pub fn run_pending(&self, session: &mut dyn SynthSession) -> usize {
        let jobs: Vec<Job> = self
            .jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain(..)
            .collect();
        let now = Instant::now();
        let mut ran = 0;
        for job in jobs {
            if job.deadline >= now {
                (job.run)(session);
                ran += 1;
            }
        }
        ran
    }

    /// The worker's session, whose calls fail after `timeout`.
    pub fn remote(self: &Arc<Self>, timeout: Duration) -> RemoteSession {
        RemoteSession {
            bridge: self.clone(),
            timeout,
            last_snapshot: Mutex::new(None),
        }
    }

    fn call<R: Send + 'static>(
        &self,
        timeout: Duration,
        run: impl FnOnce(&mut dyn SynthSession) -> R + Send + 'static,
    ) -> Result<R, Error> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let deadline = Instant::now() + timeout;
        self.jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push_back(Job {
                deadline,
                run: Box::new(move |session| {
                    let _ = sender.send(run(session));
                }),
            });
        if let Some(wake) = &*self.waker.lock().unwrap_or_else(|p| p.into_inner()) {
            wake();
        }
        receiver.recv_timeout(timeout).map_err(|_| {
            Error::Invalid("the synth did not answer; is the plugin window open?".to_string())
        })
    }
}

/// The chat worker's session: every call runs on the GUI thread's
/// [`LiveSession`](crate::live::LiveSession).
pub struct RemoteSession {
    bridge: Arc<ChatBridge>,
    timeout: Duration,
    /// The last snapshot read, returned if a later read cannot be answered.
    last_snapshot: Mutex<Option<PresetState>>,
}

impl SynthSession for RemoteSession {
    fn snapshot(&self) -> PresetState {
        let mut last = self.last_snapshot.lock().unwrap_or_else(|p| p.into_inner());
        if let Ok(state) = self.bridge.call(self.timeout, |session| session.snapshot()) {
            *last = Some(state);
        }
        last.clone()
            .unwrap_or_else(|| PresetState::new(&sas_preset::InitContent { sample_seed: 0 }))
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
            .call(self.timeout, move |session| session.apply(&edit))?
    }

    fn restore(&mut self, state: &PresetState) -> Result<(), Error> {
        let state = state.clone();
        self.bridge
            .call(self.timeout, move |session| session.restore(&state))?
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
