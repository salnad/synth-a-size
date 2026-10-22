// SPDX-License-Identifier: GPL-3.0-or-later

//! The `VoiceHandler` router overrides (`voice_handler.h:366-379`): mono
//! processors go to the global router (`getMonoRouter()` is
//! `&global_router_`), polyphonic adds are forwarded to the voice router,
//! and only the voice router's processors are polyphonic.

use sas_dsp_core::Value;
use sas_engine::voice_handler::{with_voice_handler, VoiceHandler, MAX_POLYPHONY};

#[test]
fn mono_processors_go_to_the_global_router() {
    let node = VoiceHandler::create(0, MAX_POLYPHONY, false);
    let (voice_router, global_router) =
        with_voice_handler::<VoiceHandler, _>(&node, |vh, _| vh.routers());

    assert!(node.get_mono_router().is(&global_router));
    assert!(node.get_poly_router().is(&voice_router));

    let mono = Value::create_control_rate(1.0);
    node.add_mono_processor(&mono, true);
    assert!(mono.router().unwrap().is(&global_router));
    assert!(global_router.global_order().iter().any(|p| p.is(&mono)));
    assert!(!voice_router.global_order().iter().any(|p| p.is(&mono)));
    assert!(!mono.is_polyphonic());

    let idle_mono = Value::create_control_rate(2.0);
    node.add_idle_mono_processor(&idle_mono);
    assert!(idle_mono.router().unwrap().is(&global_router));
    assert!(!idle_mono.is_polyphonic());

    let poly = Value::create_control_rate(3.0);
    node.add_processor(&poly);
    assert!(poly.router().unwrap().is(&voice_router));
    assert!(voice_router.global_order().iter().any(|p| p.is(&poly)));
    assert!(poly.is_polyphonic());
    assert!(voice_router.is_polyphonic());
    assert!(!global_router.is_polyphonic());

    node.remove_processor(&poly);
    assert!(!voice_router.global_order().iter().any(|p| p.is(&poly)));
}

/// #26, idle half: an idle add on the handler is forwarded to the voice
/// router (`VoiceHandler::addIdleProcessor`), so each voice owns a copy.
/// The render cases cannot observe this in the port (their output is
/// bit-identical with the forward reverted), so ownership is checked here.
#[test]
fn idle_processors_are_forwarded_to_the_voice_router() {
    let node = VoiceHandler::create(0, MAX_POLYPHONY, false);
    let (voice_router, _) = with_voice_handler::<VoiceHandler, _>(&node, |vh, _| vh.routers());

    let idle = Value::create_control_rate(1.0);
    node.add_idle_processor(&idle);
    assert!(idle.router().unwrap().is(&voice_router));
    assert!(voice_router
        .get_context(Some(&idle))
        .is_some_and(|context| context.is(&idle)));
}

/// #26, sample-rate half: a forwarded add takes the handler's sample rate
/// before it reaches the voice router, as upstream's override does.
#[test]
fn forwarded_processors_take_the_handlers_sample_rate() {
    let node = VoiceHandler::create(0, MAX_POLYPHONY, false);
    node.set_sample_rate(96000);

    let poly = Value::create_control_rate(1.0);
    node.add_processor(&poly);
    assert_eq!(poly.sample_rate(), 96000);

    let idle = Value::create_control_rate(2.0);
    node.add_idle_processor(&idle);
    assert_eq!(idle.sample_rate(), 96000);
}

/// End to end on the synth's voice handler: every poly modulation control's
/// switch (`createPolyModControl`'s idle `ValueSwitch`) lives inside the
/// voice router (directly for the handler's own controls, through a
/// sub-module for the modulators' and filters'), so connected poly
/// modulations are switched per voice.
#[test]
fn poly_modulation_switches_belong_to_the_voice_router() {
    use sas_engine::synth_voice_handler::SynthVoiceHandler;

    sas_engine::parameters::install();
    let beats_per_second = Value::create_control_rate(2.0);
    let node = SynthVoiceHandler::create(&beats_per_second.output(0));
    let (voice_router, global_router) =
        with_voice_handler::<SynthVoiceHandler, _>(&node, |vh, _| vh.routers());
    node.init();

    let destinations = node.get_poly_modulation_destinations();
    assert!(
        destinations.len() > 50,
        "{} poly destinations",
        destinations.len()
    );
    for name in destinations.keys() {
        let switch = node
            .get_poly_modulation_switch(name)
            .unwrap_or_else(|| panic!("{name}: no poly switch"));
        assert!(
            voice_router.get_context(Some(&switch)).is_some(),
            "{name}: switch outside the voice router"
        );
        assert!(global_router.get_context(Some(&switch)).is_none());
    }
    let own = node
        .get_poly_modulation_switch("voice_tune")
        .expect("voice_tune switch");
    assert!(own.router().unwrap().is(&voice_router));
}
