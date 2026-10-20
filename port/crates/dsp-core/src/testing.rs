// SPDX-License-Identifier: GPL-3.0-or-later

//! Drives a processor the way `ref-module` does, for parity tests.
//!
//! Mirrors `migration/reference/src/module_main.cpp` step by step, because
//! the driving sequence changes the output:
//!
//! 1. One source [`Output`] per input, in registry order: audio inputs get an
//!    audio-rate output (`kMaxBufferSize * oversample`), control and trigger
//!    inputs a control-rate one. Each is plugged into the processor (which
//!    fires `numInputsChanged`, so operators process one sample here).
//! 2. `set_sample_rate`, `set_oversample_amount`, `hard_reset`. No `init`.
//! 3. Per block of `block_size` frames: control-rate sources hold the frame
//!    at the block start (written through the aliased trigger value); trigger
//!    sources then clear their trigger and apply the block's events; audio
//!    sources clear their trigger and copy the block. Then `process(n)`.
//! 4. Outputs are read per frame (sample 0 for control-rate outputs).
//!
//! Use it with `sas-parity`: inputs come from `ModuleCase::input`, trigger
//! events from `ModuleCase::triggers`, and each input's rate is the registry
//! rate from the C++ module file, overridden by the spec's `"rate"` (a
//! `"trigger"` signal is always a trigger input).

use std::rc::Rc;

use sas_simd::{PolyFloat, PolyInt, PolyMask};

use crate::output::Output;
use crate::processor::ProcessorRef;

/// `refmod::Rate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputRate {
    Audio,
    Control,
    Trigger,
}

/// One trigger event (`TriggerEvent` in `module_main.cpp`).
#[derive(Clone, Copy, Debug)]
pub struct TriggerEvent {
    /// Absolute frame index.
    pub sample: usize,
    pub value: f32,
    /// Lanes the trigger applies to.
    pub lanes: [bool; 4],
}

/// One processor input, in registry order.
#[derive(Clone, Debug)]
pub struct DriverInput {
    /// Processor input index (`InputDef::index`).
    pub index: usize,
    pub rate: InputRate,
    /// The captured input frames (`in_<name>.npy`).
    pub frames: Vec<[f32; 4]>,
    /// Events for trigger inputs.
    pub triggers: Vec<TriggerEvent>,
}

/// Rendering settings (internal rate values, as in the spec).
#[derive(Clone, Copy, Debug)]
pub struct DriverSettings {
    pub sample_rate: i32,
    pub oversample: i32,
    pub block_size: usize,
    pub num_samples: usize,
}

/// Runs `processor` and returns the frames of each requested output.
pub fn run_module(
    processor: &ProcessorRef,
    inputs: &[DriverInput],
    settings: &DriverSettings,
    outputs: &[usize],
) -> Vec<Vec<[f32; 4]>> {
    let sources: Vec<Rc<Output>> = inputs
        .iter()
        .map(|input| {
            let source = match input.rate {
                InputRate::Audio => Output::new(
                    sas_simd::common::MAX_BUFFER_SIZE,
                    settings.oversample as usize,
                ),
                InputRate::Control | InputRate::Trigger => Output::new_control_rate(),
            };
            processor.plug(&source, input.index);
            source
        })
        .collect();

    processor.set_sample_rate(settings.sample_rate);
    processor.set_oversample_amount(settings.oversample);
    processor.hard_reset();

    let mut captured: Vec<Vec<[f32; 4]>> =
        vec![Vec::with_capacity(settings.num_samples); outputs.len()];

    let mut block_start = 0;
    while block_start < settings.num_samples {
        let n = settings.block_size.min(settings.num_samples - block_start);

        for (input, source) in inputs.iter().zip(&sources) {
            if source.is_control_rate() {
                source.set_at(0, PolyFloat::from_array(input.frames[block_start]));
                if input.rate == InputRate::Trigger {
                    source.clear_trigger();
                    for event in &input.triggers {
                        if event.sample >= block_start && event.sample < block_start + n {
                            let mask = PolyMask::from_array(event.lanes.map(|on| {
                                if on {
                                    u32::MAX
                                } else {
                                    0
                                }
                            }));
                            source.trigger(
                                mask,
                                PolyFloat::splat(event.value),
                                PolyInt::splat((event.sample - block_start) as u32),
                            );
                        }
                    }
                }
            } else {
                source.clear_trigger();
                let buffer = source.buffer();
                for s in 0..n {
                    buffer[s].set(PolyFloat::from_array(input.frames[block_start + s]));
                }
            }
        }

        processor.process(n);

        for (capture, &index) in captured.iter_mut().zip(outputs) {
            let output = processor.output(index);
            let buffer = output.buffer();
            for s in 0..n {
                let value = if output.is_control_rate() {
                    buffer[0].get()
                } else {
                    buffer[s].get()
                };
                capture.push(value.to_array());
            }
        }
        block_start += n;
    }
    captured
}
