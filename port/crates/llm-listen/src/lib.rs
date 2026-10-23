// SPDX-License-Identifier: GPL-3.0-or-later

//! The LLM layer's listen loop (L6 of the LLM plan; D13 in
//! `migration/DIVERGENCES.md`): the `audition` tool's renderer.
//!
//! - [`preview`]: [`PreviewEngine`], a second synth on its own thread that
//!   renders a short preview of a patch, never on the audio thread, within
//!   a CPU budget, and without touching the process's seed counter.
//! - [`analysis`]: the compact features the model gets instead of audio.
//! - `fft`: the transform behind the spectra.

pub mod analysis;
mod fft;
pub mod preview;

pub use analysis::{analyze, Analysis};
pub use preview::PreviewEngine;
