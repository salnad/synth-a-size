// SPDX-License-Identifier: GPL-3.0-or-later

//! The edit bus (`sas_engine::edit_bus`): what reaches the audio side, in
//! which order, and that a swapped-in edit plays exactly as the same edit
//! made in place on the engine (upstream's editor path).

use std::sync::Mutex;

use sas_common::LineGenerator;
use sas_dsp_core::ProcessorRef;
use sas_engine::document::PresetDocument;
use sas_engine::edit_bus::{
    EditBus, EditConsumer, EditError, EditHandle, EditTarget, PreparedWavetable,
    EDIT_QUEUE_CAPACITY,
};
use sas_engine::sound_engine::SoundEngine;
use sas_engine::synth::Synth;
use sas_engine::synth_voice_handler::SynthVoiceHandler;
use sas_engine::Tuning;
use sas_oscillators::sas_wavetable::creator::WavetableCreator;
use sas_oscillators::sas_wavetable::{Wavetable, WavetableFrame};
use sas_oscillators::{RandomValues, Sample};
use sas_preset::{InitContent, SampleState, SaveInfo, SlotValues};

/// A drag onto a control without a modifier key, at mid range.
const DRAG: SlotValues = SlotValues {
    amount: 0.5,
    bipolar: false,
    stereo: false,
    bypass: false,
};

/// What the consumer asked of its target, in order.
#[derive(Debug, PartialEq)]
enum Call {
    Connect(String, String),
    Disconnect(String, String),
    ConnectSlot(usize, String, String),
    DisconnectSlot(usize, String, String),
    Lfo(usize, String),
    LineMapping(usize, String),
    Values(usize, SlotValues),
    Wavetable(usize, usize),
    /// A drawn frame: the oscillator, the position and its first sample.
    Frame(usize, usize, f32),
    Sample(String),
    Tuning(String),
    Load(String),
}

#[derive(Default)]
struct Recorder(Vec<Call>);

impl EditTarget for Recorder {
    fn connect_modulation(
        &mut self,
        source: &str,
        destination: &str,
        _values: Option<&SlotValues>,
    ) {
        self.0
            .push(Call::Connect(source.into(), destination.into()));
    }
    fn disconnect_modulation(&mut self, source: &str, destination: &str) {
        self.0
            .push(Call::Disconnect(source.into(), destination.into()));
    }
    fn connect_slot(&mut self, slot: usize, source: &str, destination: &str) {
        self.0
            .push(Call::ConnectSlot(slot, source.into(), destination.into()));
    }
    fn disconnect_slot(&mut self, slot: usize, source: &str, destination: &str) {
        self.0.push(Call::DisconnectSlot(
            slot,
            source.into(),
            destination.into(),
        ));
    }
    fn set_lfo_shape(&mut self, lfo: usize, line: &mut LineGenerator) {
        self.0.push(Call::Lfo(lfo, line.name().to_string()));
    }
    fn set_line_mapping(&mut self, slot: usize, line: &mut LineGenerator) {
        self.0
            .push(Call::LineMapping(slot, line.name().to_string()));
    }
    fn set_connection_values(&mut self, slot: usize, values: &SlotValues) {
        self.0.push(Call::Values(slot, *values));
    }
    fn swap_wavetable(&mut self, oscillator: usize, rendered: &mut Wavetable) {
        self.0
            .push(Call::Wavetable(oscillator, rendered.num_frames()));
    }
    fn set_wavetable_frame(&mut self, oscillator: usize, position: usize, frame: &WavetableFrame) {
        self.0
            .push(Call::Frame(oscillator, position, frame.wave[0]));
    }
    fn swap_sample(&mut self, loaded: &mut Sample) {
        self.0.push(Call::Sample(loaded.name().to_string()));
    }
    fn swap_tuning(&mut self, tuning: &mut Tuning) {
        self.0.push(Call::Tuning(tuning.name()));
    }
    fn load_document(&mut self, document: &PresetDocument) {
        self.0
            .push(Call::Load(document.state.info.preset_name.clone()));
    }
}

fn init_document() -> PresetDocument {
    PresetDocument::init(&InitContent { sample_seed: 0 })
}

fn named_line(name: &str) -> LineGenerator {
    let mut line = LineGenerator::default();
    line.init_sin();
    line.set_name(name);
    line
}

fn named_sample(name: &str) -> SampleState {
    SampleState {
        name: name.to_string(),
        sample_rate: 44100,
        left: (0..512).map(|i| ((i % 64) as f32 / 32.0) - 1.0).collect(),
        right: None,
    }
}

#[test]
fn edits_arrive_in_order() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let connection = edits
        .connect_modulation("lfo_1", "filter_1_cutoff", DRAG)
        .unwrap();
    assert_eq!(connection.slot, 0);
    assert!(connection.created);
    assert_eq!(
        connection.parameter_changes,
        vec![
            ("modulation_1_power", 0.0),
            ("modulation_1_amount", 0.5),
            ("modulation_1_bipolar", 0.0),
            ("modulation_1_stereo", 0.0),
            ("modulation_1_bypass", 0.0),
        ]
    );
    edits.set_lfo_shape(2, named_line("wobble")).unwrap();
    edits.set_sample(named_sample("drum")).unwrap();
    assert_eq!(
        edits.disconnect_modulation("lfo_1", "filter_1_cutoff"),
        Ok(Some(0))
    );
    assert_eq!(
        edits.disconnect_modulation("lfo_1", "filter_1_cutoff"),
        Ok(None)
    );

    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Call::Connect("lfo_1".into(), "filter_1_cutoff".into()),
            Call::LineMapping(0, "Linear".into()),
            Call::Lfo(2, "wobble".into()),
            Call::Sample("drum".into()),
            Call::Disconnect("lfo_1".into(), "filter_1_cutoff".into()),
        ]
    );
    edits.with_document(|document| {
        assert!(document.state.modulations()[0].is_free());
        assert_eq!(document.state.lfos[2].name(), "wobble");
        assert_eq!(document.state.sample.name, "drum");
        assert_eq!(document.state.value("modulation_1_amount"), Some(0.5));
    });
}

#[test]
fn unknown_names_and_indices_change_nothing() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let revision = edits.revision();
    assert!(matches!(
        edits.connect_modulation("lfo_9", "filter_1_cutoff", DRAG),
        Err(EditError::UnknownSource(_))
    ));
    assert!(matches!(
        edits.connect_modulation("lfo_1", "filter_9_cutoff", DRAG),
        Err(EditError::UnknownDestination(_))
    ));
    assert_eq!(
        edits.set_lfo_shape(8, named_line("x")),
        Err(EditError::BadIndex(8))
    );
    assert_eq!(
        edits.set_line_mapping(64, named_line("x")),
        Err(EditError::BadIndex(64))
    );
    assert_eq!(edits.revision(), revision);
    assert_eq!(edits.capacity_remaining(), EDIT_QUEUE_CAPACITY);
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert!(recorder.0.is_empty());
}

#[test]
fn shape_edits_coalesce_newest_wins() {
    let (edits, mut consumer) = EditBus::new(init_document());
    for i in 0..100 {
        edits
            .set_lfo_shape(0, named_line(&format!("drag {i}")))
            .unwrap();
    }
    edits.set_line_mapping(5, named_line("curve")).unwrap();
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Call::Lfo(0, "drag 99".into()),
            Call::LineMapping(5, "curve".into())
        ]
    );
}

#[test]
fn connection_values_coalesce_and_update_the_mirror() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let connection = edits
        .connect_modulation("lfo_1", "filter_1_cutoff", DRAG)
        .unwrap();
    let bipolar = |amount: f32| SlotValues {
        amount,
        bipolar: true,
        stereo: false,
        bypass: false,
    };
    for i in 0..100 {
        let slot = edits
            .set_connection_values("lfo_1", "filter_1_cutoff", bipolar(i as f32 / 100.0))
            .unwrap();
        assert_eq!(slot, Some(connection.slot));
    }
    assert_eq!(
        edits.set_connection_values("lfo_1", "no_such_control", DRAG),
        Err(EditError::UnknownDestination("no_such_control".into()))
    );
    let mirror = edits.with_document(|document| {
        ["amount", "bipolar", "stereo", "bypass"]
            .map(|suffix| document.state.value(&format!("modulation_1_{suffix}")))
    });
    assert_eq!(mirror, [Some(0.99), Some(1.0), Some(0.0), Some(0.0)]);
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Call::Connect("lfo_1".into(), "filter_1_cutoff".into()),
            Call::LineMapping(0, "Linear".into()),
            Call::Values(0, bipolar(0.99)),
        ]
    );
}

/// `setModulationValues` finds the connection when called: for two that
/// aren't connected (any more) nothing changes, not even the freed slot.
#[test]
fn connection_values_for_a_disconnected_route_change_nothing() {
    let (edits, mut consumer) = EditBus::new(init_document());
    edits
        .connect_modulation("lfo_1", "filter_1_cutoff", DRAG)
        .unwrap();
    edits
        .disconnect_modulation("lfo_1", "filter_1_cutoff")
        .unwrap();
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    let slot_values = |edits: &sas_engine::edit_bus::EditHandle| {
        edits.with_document(|document| {
            ["amount", "bipolar", "stereo", "bypass"]
                .map(|suffix| document.state.value(&format!("modulation_1_{suffix}")))
        })
    };
    let before = slot_values(&edits);
    let bipolar = SlotValues {
        amount: 0.9,
        bipolar: true,
        stereo: true,
        bypass: false,
    };
    assert_eq!(
        edits.set_connection_values("lfo_1", "filter_1_cutoff", bipolar),
        Ok(None)
    );
    // Another route in the freed slot is not the one asked for.
    edits
        .connect_modulation("env_2", "osc_1_level", DRAG)
        .unwrap();
    assert_eq!(
        edits.set_connection_values("lfo_1", "filter_1_cutoff", bipolar),
        Ok(None)
    );
    assert_eq!(slot_values(&edits), before);
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Call::Connect("env_2".into(), "osc_1_level".into()),
            Call::LineMapping(0, "Linear".into()),
        ]
    );
}

#[test]
fn a_curve_set_after_a_connection_lands_after_its_linear_reset() {
    let (edits, mut consumer) = EditBus::new(init_document());
    edits.set_line_mapping(0, named_line("before")).unwrap();
    edits
        .connect_modulation("env_2", "osc_1_level", DRAG)
        .unwrap();
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Call::Connect("env_2".into(), "osc_1_level".into()),
            Call::LineMapping(0, "Linear".into()),
        ]
    );

    edits.disconnect_modulation("env_2", "osc_1_level").unwrap();
    edits
        .connect_modulation("env_2", "osc_1_level", DRAG)
        .unwrap();
    edits.set_line_mapping(0, named_line("after")).unwrap();
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Call::Disconnect("env_2".into(), "osc_1_level".into()),
            Call::Connect("env_2".into(), "osc_1_level".into()),
            Call::LineMapping(0, "after".into()),
        ]
    );
}

#[test]
fn a_restore_drops_older_edits_and_holds_newer_ones_until_loaded() {
    let (edits, mut consumer) = EditBus::new(init_document());
    edits.set_lfo_shape(0, named_line("stale")).unwrap();
    edits.connect_modulation("lfo_1", "volume", DRAG).unwrap();
    assert_eq!(edits.generation(), 0);

    let mut restored = init_document();
    restored.state.info.preset_name = "restored".into();
    assert_eq!(edits.begin_restore(restored), 1);
    edits.with_document(|document| {
        assert_eq!(document.state.info.preset_name, "restored");
        assert!(document.state.modulation_index("lfo_1", "volume").is_none());
    });
    edits.set_lfo_shape(1, named_line("fresh")).unwrap();

    // Before the restore is taken, the fresh edit waits; the stale ones go.
    let mut recorder = Recorder::default();
    consumer.apply_edits(&mut recorder);
    assert!(recorder.0.is_empty());

    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![Call::Load("restored".into()), Call::Lfo(1, "fresh".into())]
    );
    assert_eq!(consumer.loaded_generation(), 1);
}

#[test]
fn batches_are_all_or_nothing() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let revision = edits.revision();
    let result = edits.batch(|batch| {
        batch.connect_modulation("lfo_1", "filter_1_cutoff", DRAG)?;
        batch.set_lfo_shape(0, named_line("batched"))?;
        batch.set_sample(named_sample("batched"))?;
        batch.connect_modulation("nothing", "filter_1_cutoff", DRAG)
    });
    assert!(matches!(result, Err(EditError::UnknownSource(_))));
    assert_eq!(edits.revision(), revision);
    edits.with_document(|document| {
        assert!(document.state.modulations()[0].is_free());
        assert_eq!(document.state.lfos[0].name(), "Triangle");
        assert_eq!(document.state.sample.name, "White Noise");
        assert_eq!(document.state.wavetables.len(), 3);
    });

    edits
        .batch(|batch| {
            batch.connect_modulation("lfo_1", "filter_1_cutoff", DRAG)?;
            batch.set_lfo_shape(0, named_line("batched"))
        })
        .unwrap();
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(
        recorder.0,
        vec![
            Call::Connect("lfo_1".into(), "filter_1_cutoff".into()),
            Call::LineMapping(0, "Linear".into()),
            Call::Lfo(0, "batched".into()),
        ]
    );
}

fn named_info(name: &str) -> SaveInfo {
    let mut info = SaveInfo {
        preset_name: name.to_string(),
        author: "someone".to_string(),
        comments: "a comment".to_string(),
        style: "Bass".to_string(),
        ..SaveInfo::default()
    };
    info.macros[2] = "WOBBLE".to_string();
    info
}

#[test]
fn save_info_changes_only_the_mirror() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let generation = edits.generation();
    // Nothing is queued, so a full queue doesn't refuse it.
    while edits
        .connect_modulation("lfo_1", "filter_1_cutoff", DRAG)
        .and_then(|_| edits.disconnect_modulation("lfo_1", "filter_1_cutoff"))
        .is_ok()
    {}
    let remaining = edits.capacity_remaining();
    let revision = edits.revision();
    edits.set_info(named_info("renamed"));
    assert_eq!(edits.capacity_remaining(), remaining);
    assert_eq!(edits.revision(), revision + 1);
    assert_eq!(edits.generation(), generation);
    edits.with_document(|document| assert_eq!(document.state.info, named_info("renamed")));
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert!(recorder.0.iter().all(|call| matches!(
        call,
        Call::Connect(..) | Call::Disconnect(..) | Call::LineMapping(..)
    )));

    // In a batch: kept when it commits, rolled back when it fails.
    let revision = edits.revision();
    edits
        .batch(|batch| {
            batch.set_info(named_info("batched"));
            Ok(())
        })
        .unwrap();
    assert_eq!(edits.revision(), revision + 1);
    edits.with_document(|document| assert_eq!(document.state.info, named_info("batched")));
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert!(recorder.0.is_empty());

    let result = edits.batch(|batch| {
        batch.set_info(named_info("failed"));
        batch.connect_modulation("nothing", "filter_1_cutoff", DRAG)
    });
    assert!(matches!(result, Err(EditError::UnknownSource(_))));
    assert_eq!(edits.revision(), revision + 1);
    edits.with_document(|document| assert_eq!(document.state.info, named_info("batched")));
}

#[test]
fn a_full_queue_refuses_without_changing_the_mirror() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let lfo = |i: usize| named_line(&format!("lfo {i}"));
    for i in 0..EDIT_QUEUE_CAPACITY - 1 {
        edits.set_lfo_shape(i % 8, lfo(i)).unwrap();
    }
    assert_eq!(edits.capacity_remaining(), 1);
    assert_eq!(edits.set_lfo_shape(0, lfo(999)), Err(EditError::QueueFull));
    let result = edits.batch(|batch| {
        batch.set_sample(named_sample("too much"))?;
        batch.set_lfo_shape(1, lfo(1000))
    });
    assert_eq!(result, Err(EditError::QueueFull));
    edits.with_document(|document| {
        assert_eq!(document.state.sample.name, "White Noise");
        let last = (0..EDIT_QUEUE_CAPACITY - 1).filter(|i| i % 8 == 1).max();
        assert_eq!(
            document.state.lfos[1].name(),
            format!("lfo {}", last.unwrap())
        );
    });

    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(recorder.0.len(), 8, "one shape per LFO: {:?}", recorder.0);
    assert_eq!(edits.capacity_remaining(), EDIT_QUEUE_CAPACITY);
}

#[test]
fn the_rendered_wavetable_is_shared_with_the_editor() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let initial = edits.wavetable(0).unwrap();
    assert_eq!(initial.num_frames(), 1);
    assert!(std::sync::Arc::ptr_eq(
        &initial,
        &edits.wavetable(0).unwrap()
    ));

    let mut creator = WavetableCreator::new();
    creator.init();
    let prepared = PreparedWavetable::render(creator).unwrap();
    let rendered = prepared.wavetable().clone();
    edits
        .batch(|batch| batch.set_prepared_wavetable(1, prepared))
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &rendered,
        &edits.wavetable(1).unwrap()
    ));

    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(recorder.0, vec![Call::Wavetable(1, 1)]);

    edits.begin_restore(init_document());
    assert!(!std::sync::Arc::ptr_eq(
        &rendered,
        &edits.wavetable(1).unwrap()
    ));
}

/// The engine RNG's seed counter is process-wide.
static ENGINE_RNG: Mutex<()> = Mutex::new(());

fn fresh_synth() -> Synth {
    sas_common::random::set_next_seed(7);
    RandomValues::reset_process_state();
    let mut synth = Synth::new();
    synth.load_preset(&init_document().state);
    synth.prepare(44100, 120.0);
    synth
}

fn voice_handler(synth: &Synth) -> ProcessorRef {
    synth
        .engine()
        .with::<SoundEngine, _>(|engine, _| engine.voice_handler().clone())
}

fn with_svh<R>(synth: &Synth, f: impl FnOnce(&mut SynthVoiceHandler) -> R) -> R {
    voice_handler(synth).with::<SynthVoiceHandler, _>(|svh, _| f(svh))
}

fn render(synth: &mut Synth, controls: &[(&str, f32)]) -> Vec<f32> {
    for &(name, value) in controls {
        synth.set_control(name, value);
    }
    let mut out = Vec::new();
    let (mut left, mut right) = (vec![0.0; 64], vec![0.0; 64]);
    let mut seconds = 0.0;
    for block in 0..400 {
        let midi: &[(usize, [u8; 3])] = match block {
            0 => &[(3, [0x90, 48, 100]), (3, [0x90, 55, 90])],
            200 => &[(10, [0x80, 48, 0]), (12, [0x90, 60, 110])],
            _ => &[],
        };
        synth.render_block(&mut left, &mut right, midi, &mut seconds, 44100);
        out.extend(left.iter().zip(&right).flat_map(|(l, r)| [*l, *r]));
    }
    out
}

/// A connection, remap curve, LFO shape, wavetable and sample sent over the
/// bus play bit for bit like the same edits made on the engine in place, as
/// upstream's editor makes them (`connectModulation` plus
/// `initModulationValues`, `LineGenerator::jsonToState`,
/// `WavetableCreator::render` into the engine's table, `Sample::loadSample`).
#[test]
fn bus_edits_play_like_in_place_edits() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    let controls = [
        ("modulation_1_amount", 0.7),
        ("modulation_2_amount", -0.4),
        ("modulation_1_power", 0.0),
        ("modulation_2_power", 0.0),
        ("lfo_1_frequency", 2.5),
        ("sample_on", 1.0),
        ("sample_level", 0.5),
        ("osc_1_wave_frame", 40.0),
    ];
    let mut curve = LineGenerator::default();
    curve.init_saw_up();
    let mut lfo = LineGenerator::default();
    lfo.init_square();
    let creator = corpus_wavetable();
    let sample = named_sample("authored");

    // In place, as upstream's editor edits the engine it shares.
    let mut expected_synth = fresh_synth();
    expected_synth.connect_modulation("lfo_1", "filter_fx_cutoff");
    expected_synth.connect_modulation("env_2", "osc_1_wave_frame");
    // initModulationValues, then setModulationValues with the drag's values.
    for number in [1, 2] {
        for (suffix, value) in [
            ("power", 0.0),
            ("amount", DRAG.amount),
            ("bipolar", 0.0),
            ("stereo", 0.0),
            ("bypass", 0.0),
        ] {
            expected_synth.set_control(&format!("modulation_{number}_{suffix}"), value);
        }
    }
    expected_synth.process_modulation_changes();
    let mut in_place_curve = curve.clone();
    expected_synth.set_line_mapping(1, &mut in_place_curve);
    with_svh(&expected_synth, |svh| {
        svh.lfo_source(0)
            .borrow_mut()
            .from_json(&lfo.to_json())
            .unwrap();
        creator
            .clone()
            .render(&mut svh.wavetable(0).borrow_mut())
            .unwrap();
        svh.sample()
            .borrow_mut()
            .load_sample(&sample.left, sample.left.len(), sample.sample_rate);
        svh.sample().borrow_mut().set_name(&sample.name);
    });
    let expected = render(&mut expected_synth, &controls);

    // Over the bus.
    let mut synth = fresh_synth();
    let (edits, mut consumer) = EditBus::new(init_document());
    edits
        .connect_modulation("lfo_1", "filter_fx_cutoff", DRAG)
        .unwrap();
    edits
        .connect_modulation("env_2", "osc_1_wave_frame", DRAG)
        .unwrap();
    edits.set_line_mapping(1, curve).unwrap();
    edits.set_lfo_shape(0, lfo).unwrap();
    edits.set_wavetable(0, creator).unwrap();
    edits.set_sample(sample).unwrap();
    consumer.drain(&mut synth);
    let actual = render(&mut synth, &controls);

    let peak = expected.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "silent render (peak {peak})");
    let first_difference = expected
        .iter()
        .zip(&actual)
        .position(|(a, b)| a.to_bits() != b.to_bits());
    assert_eq!(first_difference, None);
}

/// The first wavetable of an authored corpus preset: several frames, so the
/// swap must move the table version on as rendering in place does.
fn corpus_wavetable() -> WavetableCreator {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../migration/corpus/presets/line_wavetable_fold.vital");
    let mut preset = sas_preset::PresetState::new(&InitContent { sample_seed: 0 });
    preset.load_file(&path).unwrap();
    let creator = preset.wavetables[0].clone();
    let mut probe = Wavetable::new(257);
    creator.clone().render(&mut probe).unwrap();
    assert!(probe.num_frames() > 1, "the corpus wavetable has one frame");
    creator
}

/// A slot freed by a disconnection and taken by a new route gets the drag's
/// values from the edit itself, not the old route's (a stale bypass would
/// leave it silent), before any host parameter change arrives.
#[test]
fn a_reused_slot_does_not_inherit_the_old_route() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    let mut synth = fresh_synth();
    let (edits, mut consumer) = EditBus::new(init_document());
    edits
        .connect_modulation("lfo_1", "filter_1_cutoff", DRAG)
        .unwrap();
    consumer.drain(&mut synth);
    for (name, value) in [
        ("modulation_1_amount", -0.9),
        ("modulation_1_stereo", 1.0),
        ("modulation_1_bypass", 1.0),
        ("modulation_1_power", 3.0),
    ] {
        synth.set_control(name, value);
    }
    edits
        .disconnect_modulation("lfo_1", "filter_1_cutoff")
        .unwrap();
    let connection = edits
        .connect_modulation("env_2", "osc_1_level", DRAG)
        .unwrap();
    assert_eq!((connection.slot, connection.created), (0, true));
    // No host parameter changes yet: the edit alone sets the new slot's
    // values, as upstream writes them before the next block.
    consumer.drain(&mut synth);
    synth.process_modulation_changes();
    let controls = synth.engine().get_controls();
    let value = |suffix: &str| controls[&format!("modulation_1_{suffix}")].value();
    assert_eq!(
        [
            value("amount"),
            value("bipolar"),
            value("stereo"),
            value("bypass"),
            value("power")
        ],
        [0.5, 0.0, 0.0, 0.0, 0.0]
    );
}

/// Setting a running connection's values over the bus plays bit for bit
/// like setting its controls in place (`setModulationValues`'
/// `valueChanged`), with nothing reconnected; and it is heard. The route is
/// one the init patch plays (an LFO on oscillator 1's level), and stereo
/// and bypass differ, so values landing on the wrong controls would show.
#[test]
fn connection_values_change_a_running_connection_in_place() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    const SOURCE: &str = "lfo_1";
    const DESTINATION: &str = "osc_1_level";
    let bipolar = SlotValues {
        amount: 0.8,
        bipolar: true,
        stereo: true,
        bypass: false,
    };
    #[derive(Clone, Copy, PartialEq)]
    enum Change {
        None,
        OverBus,
        InPlace,
    }
    let play = |change: Change| {
        let mut synth = fresh_synth();
        let (edits, mut consumer) = EditBus::new(init_document());
        edits.connect_modulation(SOURCE, DESTINATION, DRAG).unwrap();
        consumer.drain(&mut synth);
        let mut out = Vec::new();
        let (mut left, mut right) = (vec![0.0; 64], vec![0.0; 64]);
        let mut seconds = 0.0;
        for block in 0..300 {
            if block == 150 {
                match change {
                    Change::None => {}
                    Change::OverBus => {
                        let slot = edits
                            .set_connection_values(SOURCE, DESTINATION, bipolar)
                            .unwrap();
                        assert_eq!(slot, Some(0));
                        consumer.drain(&mut synth);
                    }
                    Change::InPlace => {
                        for (suffix, value) in [
                            ("amount", 0.8),
                            ("bipolar", 1.0),
                            ("stereo", 1.0),
                            ("bypass", 0.0),
                        ] {
                            synth.set_control(&format!("modulation_1_{suffix}"), value);
                        }
                    }
                }
            }
            let midi: &[(usize, [u8; 3])] = if block == 0 {
                &[(3, [0x90, 48, 100])]
            } else {
                &[]
            };
            synth.render_block(&mut left, &mut right, midi, &mut seconds, 44100);
            out.extend(left.iter().zip(&right).flat_map(|(l, r)| [*l, *r]));
        }
        out
    };
    let (unchanged, bus, in_place) = (
        play(Change::None),
        play(Change::OverBus),
        play(Change::InPlace),
    );
    assert!(bus.iter().any(|sample| *sample != 0.0));
    assert!(bus != unchanged, "the edit is not heard");
    assert!(bus == in_place, "the bus edit plays differently");
}

/// The corpus's septimal Scala scale.
fn corpus_tuning() -> Tuning {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../migration/corpus/tunings/corpus_septimal.scl");
    let tuning = Tuning::from_file(&path).unwrap();
    assert!(!tuning.is_default());
    tuning
}

#[test]
fn tuning_edits_update_the_mirror_and_coalesce() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let revision = edits.revision();
    let mut renamed = corpus_tuning();
    renamed.set_name("older");
    edits.set_tuning(renamed).unwrap();
    edits.set_tuning(corpus_tuning()).unwrap();
    assert_eq!(edits.revision(), revision + 2);
    let name = corpus_tuning().name();
    edits.with_document(|document| assert_eq!(document.tuning.name(), name));
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    assert_eq!(recorder.0, vec![Call::Tuning(name)]);

    // Rolled back with a failed batch.
    let result = edits.batch(|batch| {
        batch.set_tuning(Tuning::new());
        batch.connect_modulation("nothing", "filter_1_cutoff", DRAG)
    });
    assert!(matches!(result, Err(EditError::UnknownSource(_))));
    edits.with_document(|document| assert!(!document.tuning.is_default()));
}

/// A tuning sent over the bus plays bit for bit like the same tuning set on
/// the engine (`Synth::set_tuning`, as upstream's `TuningSelector` loads it).
#[test]
fn a_bus_tuning_plays_like_one_set_in_place() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    let mut expected_synth = fresh_synth();
    expected_synth.set_tuning(&corpus_tuning());
    let expected = render(&mut expected_synth, &[]);

    let mut synth = fresh_synth();
    let (edits, mut consumer) = EditBus::new(init_document());
    edits.set_tuning(corpus_tuning()).unwrap();
    consumer.drain(&mut synth);
    let actual = render(&mut synth, &[]);
    assert!(expected.iter().any(|&sample| sample != 0.0));
    assert!(
        expected
            .iter()
            .zip(&actual)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "the bus tuning plays differently"
    );

    let default = render(&mut fresh_synth(), &[]);
    assert_ne!(default, expected, "the tuning changes the sound");
}

/// The corpus wavetable with its first keyframe's line made a rising saw, so a
/// frame drawn from it differs from the committed table's.
fn drawn_creator() -> WavetableCreator {
    let mut creator = corpus_wavetable();
    if let sas_oscillators::sas_wavetable::creator::component::WavetableComponent::LineSource(
        line,
    ) = &mut creator.groups[0].components[0]
    {
        line.keyframes.keyframes[0]
            .data
            .line_generator
            .init_saw_up();
    } else {
        panic!("the corpus wavetable starts with a line source");
    }
    creator
}

/// `set_wavetable_frame` renders only the drawn frame (upstream's
/// `render(position)`) into the shared table and sends it to the engine,
/// which writes it over that frame. It is an audition: the document keeps
/// its creator and the revision doesn't move.
#[test]
fn a_drawn_frame_reaches_the_engine_and_the_shared_table() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    let mut synth = fresh_synth();
    let (edits, mut consumer) = EditBus::new(init_document());
    edits.set_wavetable(0, corpus_wavetable()).unwrap();
    consumer.drain(&mut synth);
    let before = edits.wavetable(0).unwrap();

    let creator = drawn_creator();
    let mut expected = (*before).clone();
    creator.clone().render_frame(&mut expected, 3);
    let revision = edits.revision();
    let token = edits.audition_token(0).unwrap();
    assert!(edits
        .set_wavetable_frame(0, creator.clone(), 3, token)
        .unwrap());
    let shared = edits.wavetable(0).unwrap();
    assert_eq!(shared.frame(3), expected.frame(3));
    assert_ne!(shared.frame(3), before.frame(3));
    assert_eq!(shared.frame(2), before.frame(2));
    assert_eq!(shared.num_frames(), before.num_frames());
    assert_eq!(edits.revision(), revision);
    edits.with_document(|document| {
        assert_eq!(
            document.state.wavetables[0].to_json().dump(),
            corpus_wavetable().to_json().dump()
        );
    });

    consumer.drain(&mut synth);
    let engine = with_svh(&synth, |svh| svh.wavetable(0));
    let engine = engine.borrow();
    assert_eq!(engine.frame(3), shared.frame(3));
    assert_eq!(engine.frame(2), before.frame(2));
    drop(engine);
    edits.collect_retired();
    assert_eq!(edits.leaked_payloads(), 0);
}

/// A newer frame replaces one the engine hasn't taken, and frames keep
/// their order against whole tables.
#[test]
fn drawn_frames_coalesce_and_keep_their_order() {
    let (edits, mut consumer) = EditBus::new(init_document());
    edits.set_wavetable(1, corpus_wavetable()).unwrap();
    let mut recorder = Recorder::default();
    consumer.drain(&mut recorder);
    recorder.0.clear();

    let creator = drawn_creator();
    let token = edits.audition_token(1).unwrap();
    assert!(edits
        .set_wavetable_frame(1, creator.clone(), 2, token)
        .unwrap());
    assert!(edits
        .set_wavetable_frame(1, creator.clone(), 5, token)
        .unwrap());
    consumer.drain(&mut recorder);
    let first = edits.wavetable(1).unwrap().frame(5).unwrap().wave[0];
    assert_eq!(recorder.0, vec![Call::Frame(1, 5, first)]);

    recorder.0.clear();
    edits.set_wavetable(1, corpus_wavetable()).unwrap();
    let token = edits.audition_token(1).unwrap();
    assert!(edits
        .set_wavetable_frame(1, creator.clone(), 4, token)
        .unwrap());
    let drawn = edits.wavetable(1).unwrap().frame(4).unwrap().wave[0];
    let frames = edits.wavetable(1).unwrap().num_frames();
    edits.set_wavetable(1, corpus_wavetable()).unwrap();
    consumer.drain(&mut recorder);
    // The newest whole table supersedes the first, but lands after the
    // frame, as the queue has them.
    assert_eq!(
        recorder.0,
        vec![Call::Frame(1, 4, drawn), Call::Wavetable(1, frames)]
    );
}

/// A restore drops a frame queued before it; a frame past the table's last
/// changes nothing; bad indices change nothing.
#[test]
fn drawn_frames_respect_restores_and_bounds() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let mut recorder = Recorder::default();
    let token = edits.audition_token(0).unwrap();
    assert!(edits
        .set_wavetable_frame(0, drawn_creator(), 0, token)
        .unwrap());
    edits.begin_restore(init_document());
    consumer.drain(&mut recorder);
    assert_eq!(recorder.0, vec![Call::Load(String::new())]);

    recorder.0.clear();
    let revision = edits.revision();
    let init = edits.with_document(|document| document.state.wavetables[0].to_json().dump());
    // The init table has one frame.
    let token = edits.audition_token(0).unwrap();
    assert!(edits
        .set_wavetable_frame(0, drawn_creator(), 100, token)
        .unwrap());
    assert_eq!(edits.revision(), revision);
    consumer.drain(&mut recorder);
    assert!(recorder.0.is_empty());
    edits.with_document(|document| {
        assert_eq!(document.state.wavetables[0].to_json().dump(), init);
    });

    let revision = edits.revision();
    assert_eq!(
        edits.set_wavetable_frame(3, drawn_creator(), 0, token),
        Err(EditError::BadIndex(3))
    );
    assert_eq!(
        edits.set_wavetable_frame(0, drawn_creator(), 257, token),
        Err(EditError::BadIndex(257))
    );
    assert_eq!(edits.revision(), revision);
}

/// Whether two tables hold the same frames.
fn same_frames(
    a: &sas_oscillators::sas_wavetable::Wavetable,
    b: &sas_oscillators::sas_wavetable::Wavetable,
) -> bool {
    a.num_frames() == b.num_frames() && (0..a.num_frames()).all(|i| a.frame(i) == b.frame(i))
}

/// A drawn frame or end of audition for a drag that has since been
/// committed, or from before a restore, changes nothing, so neither can
/// replace the committed table.
#[test]
fn stale_auditions_change_nothing() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let mut recorder = Recorder::default();
    edits.set_wavetable(2, corpus_wavetable()).unwrap();
    consumer.drain(&mut recorder);
    recorder.0.clear();

    let token = edits.audition_token(2).unwrap();
    edits.set_wavetable(2, corpus_wavetable()).unwrap();
    let committed = edits.wavetable(2).unwrap();
    consumer.drain(&mut recorder);
    recorder.0.clear();
    assert!(!edits
        .set_wavetable_frame(2, drawn_creator(), 3, token)
        .unwrap());
    assert!(!edits.end_audition(2, token).unwrap());
    assert!(std::sync::Arc::ptr_eq(
        &edits.wavetable(2).unwrap(),
        &committed
    ));
    consumer.drain(&mut recorder);
    assert!(recorder.0.is_empty());

    // Another oscillator's commit doesn't make it stale.
    let token = edits.audition_token(2).unwrap();
    edits.set_wavetable(1, corpus_wavetable()).unwrap();
    assert!(edits
        .set_wavetable_frame(2, drawn_creator(), 3, token)
        .unwrap());

    let token = edits.audition_token(2).unwrap();
    edits.begin_restore(init_document());
    consumer.drain(&mut recorder);
    recorder.0.clear();
    let revision = edits.revision();
    assert!(!edits
        .set_wavetable_frame(2, drawn_creator(), 0, token)
        .unwrap());
    assert!(!edits.end_audition(2, token).unwrap());
    consumer.drain(&mut recorder);
    assert!(recorder.0.is_empty());
    // The new preset's table is the document's, drawn and played.
    let init = PreparedWavetable::render(init_document().state.wavetables[2].clone()).unwrap();
    assert!(same_frames(&edits.wavetable(2).unwrap(), init.wavetable()));
    assert_eq!(edits.revision(), revision);

    assert_eq!(edits.audition_token(3), Err(EditError::BadIndex(3)));
}

/// `end_audition` after a drag that isn't committed: the shared and engine
/// tables are the document's creator again, the drag's token goes stale so
/// a late frame changes nothing, and the synth plays bit for bit as one
/// that only ever had the committed table; the document and revision stay.
#[test]
fn ending_an_audition_plays_the_document_again() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    let mut committed = fresh_synth();
    let (edits, mut consumer) = EditBus::new(init_document());
    edits.set_wavetable(0, corpus_wavetable()).unwrap();
    consumer.drain(&mut committed);
    let expected = render(&mut committed, &[]);

    let mut synth = fresh_synth();
    let (edits, mut consumer) = EditBus::new(init_document());
    edits.set_wavetable(0, corpus_wavetable()).unwrap();
    consumer.drain(&mut synth);
    let before = edits.wavetable(0).unwrap();
    let revision = edits.revision();
    let token = edits.audition_token(0).unwrap();
    assert!(edits
        .set_wavetable_frame(0, drawn_creator(), 9, token)
        .unwrap());
    consumer.drain(&mut synth);
    assert!(!same_frames(&edits.wavetable(0).unwrap(), &before));

    assert!(edits.end_audition(0, token).unwrap());
    consumer.drain(&mut synth);
    let shared = edits.wavetable(0).unwrap();
    let engine = with_svh(&synth, |svh| svh.wavetable(0));
    assert_eq!(table_bits(&shared), table_bits(&before));
    assert_eq!(table_bits(&engine.borrow()), table_bits(&before));
    assert_eq!(edits.revision(), revision);
    edits.with_document(|document| {
        assert_eq!(
            document.state.wavetables[0].to_json().dump(),
            corpus_wavetable().to_json().dump()
        );
    });

    // The drag's token is stale now: a late frame from it changes nothing.
    assert!(!edits
        .set_wavetable_frame(0, drawn_creator(), 9, token)
        .unwrap());
    assert!(!edits.end_audition(0, token).unwrap());
    consumer.drain(&mut synth);
    assert_eq!(
        table_bits(&edits.wavetable(0).unwrap()),
        table_bits(&before)
    );
    assert_eq!(table_bits(&engine.borrow()), table_bits(&before));

    let actual = render(&mut synth, &[]);
    assert!(expected.iter().any(|&sample| sample != 0.0));
    assert!(
        expected
            .iter()
            .zip(&actual)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "the ended audition plays differently from the committed table"
    );
    edits.collect_retired();
    assert_eq!(edits.leaked_payloads(), 0);
}

/// Every frame's waveform, as bits.
fn table_bits(table: &sas_oscillators::sas_wavetable::Wavetable) -> Vec<u32> {
    (0..table.num_frames())
        .flat_map(|i| table.frame(i).unwrap().wave.map(f32::to_bits))
        .collect()
}

/// A matrix row choosing its slot's route (`ModulationMatrixRow::
/// sliderValueChanged`): the old route goes and the new one takes the same
/// slot in one batch, with the slot's controls left alone. A route may repeat
/// another slot's; one to the slot's own amount is refused.
#[test]
fn a_matrix_row_rewires_its_own_slot_in_one_step() {
    let (edits, mut consumer) = EditBus::new(init_document());
    let routes = |edits: &EditHandle| {
        edits.with_document(|document| {
            document
                .state
                .modulations()
                .iter()
                .enumerate()
                .filter(|(_, slot)| !slot.is_free())
                .map(|(index, slot)| (index, slot.source.clone(), slot.destination.clone()))
                .collect::<Vec<_>>()
        })
    };
    let amount = |edits: &EditHandle| {
        edits.with_document(|document| document.state.value("modulation_4_amount"))
    };
    let before = amount(&edits);
    let drained = |consumer: &mut EditConsumer| {
        let mut recorder = Recorder::default();
        consumer.drain(&mut recorder);
        recorder.0
    };

    assert_eq!(
        edits.set_slot_route(3, Some(("lfo_1", "osc_1_level"))),
        Ok(true)
    );
    assert_eq!(
        drained(&mut consumer),
        [Call::ConnectSlot(3, "lfo_1".into(), "osc_1_level".into())]
    );
    assert_eq!(
        edits.set_slot_route(3, Some(("lfo_2", "osc_1_level"))),
        Ok(true)
    );
    assert_eq!(
        drained(&mut consumer),
        [
            Call::DisconnectSlot(3, "lfo_1".into(), "osc_1_level".into()),
            Call::ConnectSlot(3, "lfo_2".into(), "osc_1_level".into()),
        ]
    );
    assert_eq!(
        edits.set_slot_route(5, Some(("lfo_2", "osc_1_level"))),
        Ok(true)
    );
    drained(&mut consumer);
    assert_eq!(
        routes(&edits),
        [
            (3, "lfo_2".into(), "osc_1_level".into()),
            (5, "lfo_2".into(), "osc_1_level".into()),
        ]
    );
    assert_eq!(amount(&edits), before);

    assert_eq!(
        edits.set_slot_route(3, Some(("lfo_1", "modulation_4_amount"))),
        Ok(false)
    );
    assert_eq!(
        drained(&mut consumer),
        [Call::DisconnectSlot(
            3,
            "lfo_2".into(),
            "osc_1_level".into()
        )]
    );
    assert_eq!(edits.set_slot_route(5, None), Ok(false));
    assert_eq!(
        drained(&mut consumer),
        [Call::DisconnectSlot(
            5,
            "lfo_2".into(),
            "osc_1_level".into()
        )]
    );
    assert!(routes(&edits).is_empty());
    assert_eq!(edits.set_slot_route(64, None), Err(EditError::BadIndex(64)));
    assert_eq!(
        edits.set_slot_route(0, Some(("no_such_source", "osc_1_level"))),
        Err(EditError::UnknownSource("no_such_source".into()))
    );
}

/// A row's rewire plays as upstream's engine would after
/// `disconnectModulation` and `connectModulation` of the same slot, which
/// the port's `disconnect_modulation` and `connect_modulation` give here (the
/// freed slot is the first free one), with the slot's amount kept.
#[test]
fn a_rewired_slot_plays_like_a_disconnect_and_connect() {
    let _rng = ENGINE_RNG.lock().unwrap_or_else(|p| p.into_inner());
    #[derive(Clone, Copy, PartialEq)]
    enum Change {
        None,
        OverBus,
        InPlace,
    }
    let play = |change: Change| {
        let mut synth = fresh_synth();
        let (edits, mut consumer) = EditBus::new(init_document());
        edits
            .set_slot_route(0, Some(("lfo_1", "osc_1_level")))
            .unwrap();
        consumer.drain(&mut synth);
        synth.set_control("modulation_1_amount", -0.8);
        let mut out = Vec::new();
        let (mut left, mut right) = (vec![0.0; 64], vec![0.0; 64]);
        let mut seconds = 0.0;
        for block in 0..300 {
            if block == 150 {
                match change {
                    Change::None => {}
                    Change::OverBus => {
                        edits
                            .set_slot_route(0, Some(("env_1", "osc_1_level")))
                            .unwrap();
                        consumer.drain(&mut synth);
                    }
                    Change::InPlace => {
                        synth.disconnect_modulation("lfo_1", "osc_1_level");
                        synth.connect_modulation("env_1", "osc_1_level");
                    }
                }
            }
            let midi: &[(usize, [u8; 3])] = if block == 0 {
                &[(3, [0x90, 48, 100])]
            } else {
                &[]
            };
            synth.render_block(&mut left, &mut right, midi, &mut seconds, 44100);
            out.extend(left.iter().zip(&right).flat_map(|(l, r)| [*l, *r]));
        }
        out
    };
    let (unchanged, bus, in_place) = (
        play(Change::None),
        play(Change::OverBus),
        play(Change::InPlace),
    );
    assert!(bus.iter().any(|sample| *sample != 0.0));
    assert!(bus != unchanged, "the rewire is not heard");
    assert!(bus == in_place, "the rewire plays differently");
}
