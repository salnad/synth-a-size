// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for the up- and downsamplers through the probes of
//! `migration/reference/src/modules/filters/oversampling.cpp`:
//! `oversampling_round_trip` (Passthrough → `Decimator` → `Upsampler`) and
//! `fir_halfband_decimator`.

mod support;

use sas_dsp_core::{Processor, ProcessorImpl, ProcessorRef};
use sas_filters::{decimator, Decimator, FirHalfbandDecimator, Upsampler};
use sas_simd::PolyMask;
use support::{check_module, Audio, ModuleDef};

/// `Passthrough` in `oversampling.cpp`.
#[derive(Clone)]
struct Passthrough;

impl ProcessorImpl for Passthrough {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let source = this.input_buffer(0);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            dest[i].set(source[i].get());
        }
    }
}

/// `RoundTripProbe` in `oversampling.cpp`.
#[derive(Clone)]
struct RoundTripProbe {
    decimator: ProcessorRef,
    upsampler: ProcessorRef,
}

impl RoundTripProbe {
    fn create(max_stages: i32) -> ProcessorRef {
        let source = Processor::new(Passthrough, 1, 1, false, 1);
        let decimator = Decimator::create(max_stages);
        let upsampler = Upsampler::create();
        let router = Processor::new_router(
            RoundTripProbe {
                decimator: decimator.clone(),
                upsampler: upsampler.clone(),
            },
            1,
            1,
            false,
        );

        source.use_input(&router.input(0), 0);
        decimator.plug(&source, decimator::AUDIO);
        upsampler.use_output(&router.output(0), 0);

        router.add_processor(&source);
        router.add_processor(&decimator);
        router.add_idle_processor(&upsampler);
        decimator.init();
        router
    }
}

impl ProcessorImpl for RoundTripProbe {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        this.router_process(num_samples);
        let decimated = self.decimator.output_buffer(0);
        self.upsampler
            .process_with_input(&decimated, num_samples / this.oversample_amount() as usize);
    }
}

/// `FirProbe` in `oversampling.cpp`.
#[derive(Clone)]
struct FirProbe {
    fir: ProcessorRef,
}

impl FirProbe {
    fn create() -> ProcessorRef {
        let fir = FirHalfbandDecimator::create();
        let probe = Processor::new(FirProbe { fir: fir.clone() }, 1, 1, false, 1);
        fir.use_input(&probe.input(0), 0);
        probe
    }
}

impl ProcessorImpl for FirProbe {
    sas_dsp_core::processor_boilerplate!();

    fn reset(&mut self, _this: &Processor, reset_mask: PolyMask) {
        self.fir.reset(reset_mask);
    }

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let decimated = num_samples / 2;
        self.fir.process(decimated);
        let fir_out = self.fir.output_buffer(0);
        let dest = this.output_buffer(0);
        for i in 0..num_samples {
            dest[i].set(fir_out[i / 2].get());
        }
    }
}

#[test]
fn oversampling_round_trip_matches_reference() {
    check_module(
        "oversampling_round_trip",
        &ModuleDef {
            create: |params| {
                RoundTripProbe::create(
                    params
                        .get("max_stages")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(3) as i32,
                )
            },
            inputs: vec![("audio", 0, Audio)],
            outputs: vec![("audio", 0)],
        },
    );
}

#[test]
fn fir_halfband_decimator_matches_reference() {
    check_module(
        "fir_halfband_decimator",
        &ModuleDef {
            create: |_| FirProbe::create(),
            inputs: vec![("audio", 0, Audio)],
            outputs: vec![("audio", 0)],
        },
    );
}
