// SPDX-License-Identifier: GPL-3.0-or-later

//! A filter processed with an unconnected cutoff reads zeros from the null
//! source without keeping the thread-local null buffer, so no graph holds it.
//! `DirtyFilter` reads the cutoff through `FilterState::midi_cutoff_at`;
//! `DigitalSvf` and `PhaserFilter` index the transient buffer from
//! `FilterState::midi_cutoff_buffer`.

use std::rc::Rc;

use sas_dsp_core::{Input, Output, ProcessorRef};
use sas_filters::character::phaser_filter::PhaserFilter;
use sas_filters::digital_svf::DigitalSvf;
use sas_filters::dirty_filter::DirtyFilter;
use sas_filters::synth_filter::input;
use sas_simd::common::MAX_BUFFER_SIZE;
use sas_simd::PolyFloat;

const BLOCK: usize = 64;
const BLOCKS: usize = 8;

fn run(filter: &ProcessorRef, audio: &Output) -> Vec<[f32; 4]> {
    filter.set_sample_rate(44100);
    filter.hard_reset();
    let mut out = Vec::new();
    for block in 0..BLOCKS {
        let buffer = audio.buffer();
        for s in 0..BLOCK {
            let t = (block * BLOCK + s) as f32;
            buffer[s].set(PolyFloat::from_array([
                (t * 0.05).sin(),
                (t * 0.11).sin(),
                (t * 0.23).sin(),
                (t * 0.31).sin(),
            ]));
        }
        filter.process(BLOCK);
        let output = filter.output(0);
        out.extend((0..BLOCK).map(|s| output.at(s).to_array()));
    }
    out
}

fn check_unconnected_cutoff(create: fn() -> ProcessorRef) {
    let null_holders = Rc::strong_count(&Input::default().buffer());

    let unconnected = create();
    let audio = Output::new(MAX_BUFFER_SIZE, 1);
    unconnected.plug(&audio, input::AUDIO);
    assert!(unconnected.input(input::MIDI_CUTOFF).is_null_source());
    let from_null = run(&unconnected, &audio);

    assert_eq!(
        Rc::strong_count(&Input::default().buffer()),
        null_holders,
        "the filter keeps a clone of the null buffer"
    );

    let zeros = create();
    let audio = Output::new(MAX_BUFFER_SIZE, 1);
    let cutoff = Output::new(MAX_BUFFER_SIZE, 1);
    zeros.plug(&audio, input::AUDIO);
    zeros.plug(&cutoff, input::MIDI_CUTOFF);
    let from_zeros = run(&zeros, &audio);

    assert!(from_null.iter().flatten().any(|&x| x != 0.0));
    assert!(from_null.iter().flatten().all(|x| x.is_finite()));
    let bits = |frames: &[[f32; 4]]| -> Vec<u32> {
        frames.iter().flatten().map(|x| x.to_bits()).collect()
    };
    assert_eq!(
        bits(&from_null),
        bits(&from_zeros),
        "an unconnected cutoff reads as a zero cutoff"
    );
}

#[test]
fn dirty_filter_with_unconnected_cutoff_holds_no_null_buffer() {
    check_unconnected_cutoff(DirtyFilter::create);
}

#[test]
fn digital_svf_with_unconnected_cutoff_holds_no_null_buffer() {
    check_unconnected_cutoff(DigitalSvf::create);
}

#[test]
fn phaser_filters_with_unconnected_cutoff_hold_no_null_buffer() {
    check_unconnected_cutoff(|| PhaserFilter::create(true));
    check_unconnected_cutoff(|| PhaserFilter::create(false));
}
