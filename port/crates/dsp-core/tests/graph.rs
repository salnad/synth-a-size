// SPDX-License-Identifier: GPL-3.0-or-later

//! Processor-graph mechanics: ordering, feedback insertion, voice copies,
//! operator enabling, value switches and module controls.

use std::rc::Rc;

use sas_dsp_core::operators::{self, Add, Negate, VariableAdd};
use sas_dsp_core::synth_module::{set_parameter_lookup, ParameterDetails, ValueScale};
use sas_dsp_core::utilities::value_switch::ValueSwitch;
use sas_dsp_core::{
    Feedback, Processor, ProcessorImpl, ProcessorRef, ProcessorRouter, RouterHooks, SynthModule,
    Value,
};
use sas_simd::PolyFloat;

fn index_of(order: &[ProcessorRef], p: &ProcessorRef) -> usize {
    order
        .iter()
        .position(|q| Rc::ptr_eq(q, p))
        .expect("not in order")
}

/// A stateful counter: output = previous output + input (per block, sample 0).
#[derive(Clone, Default)]
struct Accumulate {
    total: PolyFloat,
}

impl ProcessorImpl for Accumulate {
    sas_dsp_core::processor_boilerplate!();

    fn process(&mut self, this: &Processor, num_samples: usize) {
        let input = this.input_buffer(0);
        let out = this.output_buffer(0);
        for i in 0..num_samples {
            self.total += input[i].get();
            out[i].set(self.total);
        }
    }
}

fn accumulate() -> ProcessorRef {
    Processor::new(Accumulate::default(), 1, 1, false, 1)
}

#[test]
fn reorder_updates_the_global_order_and_copies() {
    let source = Value::create(1.0, false);
    let router = ProcessorRouter::create(0, 0, false);

    let a = Negate::create();
    let b = Negate::create();
    let c = Add::create();
    // Added in reverse order, then wired.
    router.add_processor(&c);
    router.add_processor(&b);
    router.add_processor(&a);
    a.plug(&source, 0);
    b.plug(&a, 0);
    c.plug(&b, 0);
    c.plug(&source, 1);

    let global = router.global_order();
    assert!(index_of(&global, &a) < index_of(&global, &b));
    assert!(index_of(&global, &b) < index_of(&global, &c));

    // Upstream's reorder bumps the router's own change counter too, so the
    // router it runs on keeps its insertion order; copies rebuild from the
    // global order.
    let local = router.local_order();
    assert!(Rc::ptr_eq(&local[0], &c) && Rc::ptr_eq(&local[2], &a));
    router.process(16);
    assert_eq!(
        c.output(0).at(15).to_array(),
        [1.0; 4],
        "c ran before a and b"
    );
    router.process(16);
    assert_eq!(c.output(0).at(15).to_array(), [1.0; 4], "b has run once");
    router.process(16);
    assert_eq!(c.output(0).at(15).to_array(), [2.0; 4], "steady state");

    let voice = router.clone_processor();
    voice.process(16);
    let voice_order = voice.local_order();
    let local_a = voice.get_local_processor(&a).unwrap();
    let local_c = voice.get_local_processor(&c).unwrap();
    assert!(Rc::ptr_eq(&voice_order[0], &local_a) && Rc::ptr_eq(&voice_order[2], &local_c));
}

#[test]
fn cycle_inserts_one_block_feedback() {
    let source = Value::create_control_rate(1.0);
    let router = ProcessorRouter::create(0, 0, true);
    let add = operators::cr::Add::create();
    let scale = operators::cr::Multiply::create();
    let half = Value::create_control_rate(0.5);
    router.add_processor(&add);
    router.add_processor(&scale);

    add.plug(&source, 0);
    scale.plug(&add, 0);
    scale.plug(&half, 1);
    // Closing the loop: add reads scale, which reads add.
    add.plug(&scale, 1);

    assert_eq!(router.local_feedback_order().len(), 1);
    let feedback = &router.local_feedback_order()[0];
    assert!(feedback.is_kernel::<Feedback>());
    assert!(feedback.is_control_rate(), "upstream inserts cr::Feedback");

    // Block 1: feedback output 0 -> add = 1, scale = 0.5.
    router.process(8);
    assert_eq!(add.output(0).at(0).get(0), 1.0);
    // Block 2: feedback carries 0.5 -> add = 1.5.
    router.process(8);
    assert_eq!(add.output(0).at(0).get(0), 1.5);
    router.process(8);
    assert_eq!(add.output(0).at(0).get(0), 1.75);
}

#[test]
fn router_copies_clone_stateful_and_share_stateless() {
    let source = Value::create(1.0, false);
    let router = ProcessorRouter::create(0, 0, false);
    let acc = accumulate();
    let neg = Negate::create();
    acc.plug(&source, 0);
    neg.plug(&acc, 0);
    router.add_processor(&acc);
    router.add_processor(&neg);
    router.process(4);
    assert_eq!(acc.output(0).at(3).get(0), 4.0);

    let voice = router.clone_processor();
    voice.process(4);
    let local = voice.local_order();
    assert_eq!(local.len(), 2);
    // The copy's accumulator was cloned (copy-constructed state: total 4).
    assert!(!Rc::ptr_eq(&local[0], &acc));
    // Outputs are shared scratch: the copy wrote 5..8 into the same buffer.
    assert_eq!(acc.output(0).at(3).get(0), 8.0);

    // A stateless operator is cloned by the copy constructor too, but a
    // later rebuild shares it.
    let extra = Negate::create();
    extra.plug(&neg, 0);
    router.add_processor(&extra);
    voice.process(4);
    let local = voice.local_order();
    assert_eq!(local.len(), 3);
    assert!(Rc::ptr_eq(
        &local[index_of(&router.global_order(), &extra)],
        &extra
    ));
    assert!(Rc::ptr_eq(
        &local[index_of(&router.global_order(), &neg)],
        &neg
    ));
    assert!(!Rc::ptr_eq(
        &local[index_of(&router.global_order(), &acc)],
        &acc
    ));
}

#[test]
fn operator_enables_on_connection() {
    let add = Add::create();
    assert!(!add.enabled(), "no inputs connected");
    let one = Value::create(1.0, false);
    add.plug(&one, 0);
    assert!(add.enabled());
    add.unplug_index(0);
    assert!(!add.enabled());
    assert_eq!(add.output(0).at(0).to_array(), [0.0; 4]);

    add.plug(&one, 0);
    add.enable(false);
    assert!(!add.enabled(), "externally disabled");
    add.enable(true);
    assert!(add.enabled());
}

#[test]
fn variable_add_and_plug_next() {
    let sum = VariableAdd::create(0);
    let a = Value::create(1.0, false);
    let b = Value::create(PolyFloat::new(1.0, 2.0, 3.0, 4.0), false);
    sum.plug_next(&a);
    sum.plug_next(&b);
    assert_eq!(sum.num_inputs(), 2);
    sum.process(4);
    assert_eq!(sum.output(0).at(2).to_array(), [2.0, 3.0, 4.0, 5.0]);
}

#[test]
fn value_trigger_sets_lanes() {
    let value = Value::create(0.0, false);
    let trigger = sas_dsp_core::Output::new_default();
    value.plug(&trigger, sas_dsp_core::value::SET);
    trigger.trigger(
        sas_simd::PolyMask::new(0, 0, u32::MAX, u32::MAX),
        PolyFloat::splat(7.0),
        sas_simd::PolyInt::zero(),
    );
    value.process(2);
    assert_eq!(value.output(0).at(1).to_array(), [0.0, 0.0, 7.0, 7.0]);
    value.set_value(3.0);
    assert_eq!(value.value(), 3.0);
}

#[test]
fn value_switch_redirects_buffer() {
    let switch = ValueSwitch::create(0.0);
    let a = Value::create(1.0, false);
    let b = Value::create(2.0, false);
    switch.plug_next(&a);
    switch.plug_next(&b);
    let gated = Negate::create();
    gated.plug(&b, 0);
    ValueSwitch::add_processor(&switch, &gated);

    switch.set_value(0.0);
    assert_eq!(switch.output(ValueSwitch::SWITCH).at(0).get(0), 1.0);
    assert!(!gated.enabled());
    switch.set_value(1.0);
    assert_eq!(switch.output(ValueSwitch::SWITCH).at(0).get(0), 2.0);
    assert!(gated.enabled());
    assert_eq!(switch.output(ValueSwitch::SWITCH).buffer_size(), 128);
}

#[test]
fn synth_module_controls_and_switches() {
    set_parameter_lookup(|name| match name {
        "level" => ParameterDetails {
            default_value: 0.5,
            value_scale: ValueScale::Quadratic,
            ..Default::default()
        },
        _ => ParameterDetails::default(),
    });

    let module = SynthModule::create(0, 0, false);
    let level = module.create_poly_mod_control("level", false, false, None, None);
    assert!(module.get_controls().contains_key("level"));
    let destination = module.get_poly_modulation_destination("level").unwrap();
    let switch = module.get_poly_modulation_switch("level").unwrap();

    module.process(16);
    // Quadratic scale of the default: 0.25.
    assert_eq!(level.at(0).get(0), 0.25);
    assert_eq!(switch.value(), 0.0);

    // Modulate: the switch flips once something is connected.
    let modulation = Value::create_control_rate(0.2);
    destination.plug_next(&modulation);
    module.update_all_modulation_switches();
    assert_eq!(switch.value(), 1.0);
    module.process(16);
    let expected = (0.5f32 + 0.2) * (0.5 + 0.2);
    assert_eq!(level.at(0).get(0), expected);

    module.enable(false);
    assert!(!module.enabled());
    let _ = operators::cr::Add::create();
}

/// `VoiceHandler`'s router overrides (`voice_handler.cpp`): `addProcessor`
/// and `addIdleProcessor` set the processor's sample rate to the handler's
/// and go to the voice router, `removeProcessor` goes there too, and
/// `isPolyphonic` is true exactly for the voice router.
#[test]
fn voice_handler_hooks_forward_to_the_voice_router() {
    let handler = SynthModule::create(0, 0, false);
    let voice_router = ProcessorRouter::create(0, 0, false);
    voice_router.set_router(Some(&handler));
    handler.set_router_hooks(RouterHooks {
        polyphonic_child: Some(Rc::downgrade(&voice_router)),
        mono_router: Some(Rc::downgrade(&handler)),
        poly_router: Some(Rc::downgrade(&voice_router)),
        forward_add_remove: Some(Rc::downgrade(&voice_router)),
    });
    handler.set_sample_rate(96_000);
    assert_eq!(voice_router.sample_rate(), 44_100);

    let in_voice = |p: &ProcessorRef| p.router().is_some_and(|r| Rc::ptr_eq(&r, &voice_router));

    let processor = accumulate();
    handler.add_processor(&processor);
    assert!(in_voice(&processor));
    assert_eq!(processor.sample_rate(), 96_000);
    assert!(processor.is_polyphonic());

    let idle = Value::create_control_rate(0.0);
    handler.add_idle_processor(&idle);
    assert!(in_voice(&idle));
    assert_eq!(idle.sample_rate(), 96_000);
    assert!(idle.is_polyphonic());

    handler.remove_processor(&processor);
    assert!(processor.router().is_none());

    // SynthModule::createPolyModControl adds its modulation switch with
    // addIdleProcessor, so on a voice handler it is a polyphonic voice
    // processor.
    handler.create_poly_mod_control("voice_handler_level", false, false, None, None);
    let switch = handler
        .get_poly_modulation_switch("voice_handler_level")
        .unwrap();
    assert!(in_voice(&switch));
    assert!(switch.is_polyphonic());

    // Without the hook the same calls stay on the router, which is
    // monophonic here.
    let plain = SynthModule::create(0, 0, false);
    let idle = Value::create_control_rate(0.0);
    plain.add_idle_processor(&idle);
    assert!(idle.router().is_some_and(|r| Rc::ptr_eq(&r, &plain)));
    assert_eq!(idle.sample_rate(), 44_100);
    assert!(!idle.is_polyphonic());
}
