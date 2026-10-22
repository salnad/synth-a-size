// SPDX-License-Identifier: GPL-3.0-or-later

//! The init state carries the engine's own white noise, as upstream's
//! `getStateInformation` saves the engine's sample, and building the plugin
//! draws nothing from the engine's random seed counter.
//!
//! The seed counter is process-wide, so this runs alone in its own binary.

use sas_common::json;
use sas_plugin::{state, SoundEngineHost, SynthPlugin};

/// The seed counter's next value, without consuming it.
fn peek_next_seed() -> i32 {
    let next = sas_common::random::take_next_seed();
    sas_common::random::set_next_seed(next);
    next
}

#[test]
fn the_init_state_saves_the_engines_own_white_noise() {
    let engine = SoundEngineHost::new();
    let noise = engine.sample_state();
    assert_eq!(noise.name, "White Noise");

    let next_seed = peek_next_seed();
    let plugin = SynthPlugin::new(engine);
    assert_eq!(
        peek_next_seed(),
        next_seed,
        "building the plugin drew a seed"
    );

    let params = plugin.synth_params();
    assert_eq!(params.preset_state().sample, noise);
    let saved = params.capture_state();
    let data = json::parse(&saved.fields[state::PRESET_FIELD]).unwrap();
    assert_eq!(
        data.get("settings").unwrap().get("sample").unwrap().dump(),
        noise.to_json().dump()
    );
}
