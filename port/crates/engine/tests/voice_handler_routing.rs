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
