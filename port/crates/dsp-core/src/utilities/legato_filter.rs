// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::LegatoFilter`: passes note triggers through, except note-ons that
//! continue a held note while legato is on.
//!
//! Ports `vital/src/synthesis/utilities/legato_filter.h` and
//! `legato_filter.cpp`.

use sas_simd::common::VoiceEvent;
use sas_simd::poly_utils as utils;
use sas_simd::PolyFloat;

use crate::processor::{Processor, ProcessorImpl, ProcessorRef};

/// `vital::LegatoFilter` (control rate).
#[derive(Clone)]
pub struct LegatoFilter {
    last_value: PolyFloat,
}

impl LegatoFilter {
    pub const LEGATO: usize = 0;
    pub const TRIGGER: usize = 1;
    pub const NUM_INPUTS: usize = 2;

    pub const RETRIGGER: usize = 0;
    pub const NUM_OUTPUTS: usize = 1;

    pub fn create() -> ProcessorRef {
        Processor::new(
            LegatoFilter {
                last_value: PolyFloat::splat(VoiceEvent::VoiceOff.as_f32()),
            },
            Self::NUM_INPUTS,
            Self::NUM_OUTPUTS,
            true,
            1,
        )
    }
}

impl ProcessorImpl for LegatoFilter {
    crate::processor_boilerplate!();

    fn process(&mut self, this: &Processor, _num_samples: usize) {
        let retrigger = this.output(Self::RETRIGGER);
        retrigger.clear_trigger();
        let trigger = this.input(Self::TRIGGER);
        let mut trigger_mask = trigger.trigger_mask();
        if trigger_mask.any_mask() == 0 {
            return;
        }

        let trigger_value = trigger.trigger_value();
        let trigger_offset = trigger.trigger_offset();
        let voice_on = PolyFloat::splat(VoiceEvent::VoiceOn.as_f32());
        let mut legato_mask = PolyFloat::equal(this.input(Self::LEGATO).at(0), PolyFloat::zero());
        legato_mask |= PolyFloat::not_equal(trigger_value, voice_on);
        legato_mask |= PolyFloat::not_equal(self.last_value, voice_on);
        trigger_mask &= legato_mask;
        retrigger.trigger(trigger_mask, trigger_value, trigger_offset);
        self.last_value = utils::mask_load(self.last_value, trigger_value, trigger_mask);
    }
}
