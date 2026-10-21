// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Processor`: graph node plus per-instance DSP kernel.
//!
//! Ports `vital/src/synthesis/framework/processor.h` and `processor.cpp`.
//!
//! Upstream's `Processor` object mixes two kinds of data, and its copy
//! (`clone()`, used to make one instance per voice pair) treats them
//! differently: the state, the input list and the output list are
//! `shared_ptr`s that clones **share**, while subclass members (filter
//! memories, phases, ...) are **copied**. Here that split is explicit:
//!
//! - [`Processor`] is the node: shared state, inputs, outputs, router link and
//!   (for routers) the routing tables. Graph code holds it as
//!   [`ProcessorRef`] (`Rc<Processor>`), and node identity is `Rc` identity
//!   (upstream compares `Processor*`).
//! - [`ProcessorImpl`] is the kernel: the subclass members and the virtual
//!   methods. Every kernel method receives its node as `this`.
//!
//! Inside a kernel method, call the kernel's own virtuals directly
//! (`self.process(this, 1)`), not through the node (`this.process(1)`): the
//! kernel is already borrowed, and re-entering it panics. The one virtual the
//! framework calls back while a kernel may be running, `numInputsChanged`, is
//! deferred until the kernel call returns.

use std::any::Any;
use std::cell::{Cell, RefCell, RefMut};
use std::rc::{Rc, Weak};

use sas_simd::common::{VoiceEvent, DEFAULT_SAMPLE_RATE, MAX_BUFFER_SIZE};
use sas_simd::{PolyFloat, PolyInt, PolyMask};

use crate::output::{Buffer, Input, Output};
use crate::router::RouterState;
use crate::synth_module::ModuleData;
use crate::value::ValueKernel;

/// A reference-counted graph node (`Processor*`).
pub type ProcessorRef = Rc<Processor>;

/// `vital::ProcessorState`, shared by an instance and its clones.
#[derive(Debug)]
pub struct ProcessorState {
    pub sample_rate: Cell<i32>,
    pub oversample_amount: Cell<i32>,
    pub control_rate: Cell<bool>,
    pub enabled: Cell<bool>,
    pub initialized: Cell<bool>,
}

impl Default for ProcessorState {
    fn default() -> Self {
        ProcessorState {
            sample_rate: Cell::new(DEFAULT_SAMPLE_RATE),
            oversample_amount: Cell::new(1),
            control_rate: Cell::new(false),
            enabled: Cell::new(true),
            initialized: Cell::new(false),
        }
    }
}

/// The parts of a node that clones share (`state_`, `inputs_`, `outputs_`).
struct Shared {
    state: ProcessorState,
    inputs: RefCell<Vec<Option<Rc<Input>>>>,
    outputs: RefCell<Vec<Option<Rc<Output>>>>,
}

/// The virtual methods and subclass members of a `vital::Processor`.
///
/// Default methods reproduce the base class (`Processor`, or
/// `ProcessorRouter` for router nodes). An override that needs the base
/// behaviour calls the node's `processor_*` / `router_*` method, the
/// equivalent of `Processor::method(...)` / `ProcessorRouter::method(...)`.
pub trait ProcessorImpl: Any {
    /// `clone()`: copies the kernel (the node is copied by the framework).
    fn clone_box(&self) -> Box<dyn ProcessorImpl>;

    /// `process(num_samples)`. Routers process their children.
    fn process(&mut self, this: &Processor, num_samples: usize) {
        this.default_process(num_samples);
    }

    /// `processWithInput(audio_in, num_samples)`.
    fn process_with_input(&mut self, _this: &Processor, _audio_in: &Buffer, _num_samples: usize) {
        panic!("processWithInput is not implemented for this processor");
    }

    /// `hasState()`: false for stateless processors, which routers share
    /// between voices instead of cloning. Read once, at construction.
    fn has_state(&self) -> bool {
        true
    }

    fn init(&mut self, this: &Processor) {
        this.default_init();
    }

    fn reset(&mut self, _this: &Processor, _reset_mask: PolyMask) {}

    fn hard_reset(&mut self, this: &Processor) {
        self.reset(this, PolyMask::full());
    }

    fn set_sample_rate(&mut self, this: &Processor, sample_rate: i32) {
        this.default_set_sample_rate(sample_rate);
    }

    fn set_oversample_amount(&mut self, this: &Processor, oversample: i32) {
        this.default_set_oversample_amount(oversample);
    }

    /// Default: `SynthModule::enable` for module nodes, otherwise
    /// `Processor::enable`.
    fn enable(&mut self, this: &Processor, enable: bool) {
        this.default_enable(enable);
    }

    fn set_control_rate(&mut self, this: &Processor, control_rate: bool) {
        this.state().control_rate.set(control_rate);
    }

    fn num_inputs_changed(&mut self, _this: &Processor) {}

    /// `SynthModule::correctToTime`.
    fn correct_to_time(&mut self, _this: &Processor, _seconds: f64) {}

    /// `Feedback::refreshOutput`; called by routers on their feedback nodes.
    fn refresh_output(&mut self, _this: &Processor, _num_samples: usize) {}

    /// `ProcessorRouter::resetFeedbacks` (virtual upstream).
    fn reset_feedbacks(&mut self, this: &Processor, reset_mask: PolyMask) {
        this.router_reset_feedbacks(reset_mask);
    }

    /// The `Value` interface of `Value`-derived kernels (`set`, `value`).
    fn as_value(&mut self) -> Option<&mut dyn ValueKernel> {
        None
    }

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Implements [`ProcessorImpl::clone_box`], `as_any` and `as_any_mut` for a
/// `Clone` kernel.
#[macro_export]
macro_rules! processor_boilerplate {
    () => {
        fn clone_box(&self) -> ::std::boxed::Box<dyn $crate::ProcessorImpl> {
            ::std::boxed::Box::new(::std::clone::Clone::clone(self))
        }
        fn as_any(&self) -> &dyn ::std::any::Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn ::std::any::Any {
            self
        }
    };
}

/// `vital::Processor` (and, with [`Processor::new_router`],
/// `vital::ProcessorRouter`).
pub struct Processor {
    shared: Rc<Shared>,
    owned_inputs: RefCell<Vec<Rc<Input>>>,
    owned_outputs: RefCell<Vec<Rc<Output>>>,
    router: RefCell<Weak<Processor>>,
    plugging_start: Cell<usize>,
    pub(crate) router_state: Option<RouterState>,
    pub(crate) module_data: RefCell<Option<Rc<RefCell<ModuleData>>>>,
    kernel: RefCell<Box<dyn ProcessorImpl>>,
    has_state: bool,
    pending_inputs_changed: Cell<bool>,
    self_ref: Weak<Processor>,
}

impl Processor {
    /// `Processor(num_inputs, num_outputs, control_rate, max_oversample)`.
    pub fn new(
        kernel: impl ProcessorImpl,
        num_inputs: usize,
        num_outputs: usize,
        control_rate: bool,
        max_oversample: usize,
    ) -> ProcessorRef {
        Self::build(
            Box::new(kernel),
            num_inputs,
            num_outputs,
            control_rate,
            max_oversample,
            None,
        )
    }

    /// `ProcessorRouter(num_inputs, num_outputs, control_rate)`.
    pub fn new_router(
        kernel: impl ProcessorImpl,
        num_inputs: usize,
        num_outputs: usize,
        control_rate: bool,
    ) -> ProcessorRef {
        Self::build(
            Box::new(kernel),
            num_inputs,
            num_outputs,
            control_rate,
            1,
            Some(RouterState::new()),
        )
    }

    fn build(
        kernel: Box<dyn ProcessorImpl>,
        num_inputs: usize,
        num_outputs: usize,
        control_rate: bool,
        max_oversample: usize,
        router_state: Option<RouterState>,
    ) -> ProcessorRef {
        let has_state = kernel.has_state();
        let state = ProcessorState::default();
        state.oversample_amount.set(max_oversample as i32);
        state.control_rate.set(control_rate);
        let processor = Rc::new_cyclic(|self_ref| Processor {
            shared: Rc::new(Shared {
                state,
                inputs: RefCell::new(Vec::new()),
                outputs: RefCell::new(Vec::new()),
            }),
            owned_inputs: RefCell::new(Vec::new()),
            owned_outputs: RefCell::new(Vec::new()),
            router: RefCell::new(Weak::new()),
            plugging_start: Cell::new(0),
            router_state,
            module_data: RefCell::new(None),
            kernel: RefCell::new(kernel),
            has_state,
            pending_inputs_changed: Cell::new(false),
            self_ref: self_ref.clone(),
        });
        for _ in 0..num_inputs {
            processor.add_input();
        }
        for _ in 0..num_outputs {
            processor.add_output(max_oversample);
        }
        processor
    }

    /// `clone()`: a new instance that shares this one's state, inputs and
    /// outputs, with a copied kernel. Routers also clone their children.
    pub fn clone_processor(&self) -> ProcessorRef {
        let kernel = self.kernel.borrow().clone_box();
        let router_state = self.router_state.as_ref().map(RouterState::clone_for_copy);
        Rc::new_cyclic(|self_ref| Processor {
            shared: self.shared.clone(),
            owned_inputs: RefCell::new(self.owned_inputs.borrow().clone()),
            owned_outputs: RefCell::new(self.owned_outputs.borrow().clone()),
            router: RefCell::new(self.router.borrow().clone()),
            plugging_start: Cell::new(self.plugging_start.get()),
            router_state,
            module_data: RefCell::new(self.module_data.borrow().clone()),
            kernel: RefCell::new(kernel),
            has_state: self.has_state,
            pending_inputs_changed: Cell::new(false),
            self_ref: self_ref.clone(),
        })
    }

    /// This node as a [`ProcessorRef`].
    pub fn this_ref(&self) -> ProcessorRef {
        self.self_ref
            .upgrade()
            .expect("processor is not owned by an Rc")
    }

    /// True when `self` and `other` are the same node.
    #[inline]
    pub fn is(&self, other: &Processor) -> bool {
        std::ptr::eq(self, other)
    }

    pub(crate) fn key(&self) -> usize {
        self as *const Processor as usize
    }

    // ----- kernel access and virtual dispatch -----

    fn with_kernel<R>(&self, f: impl FnOnce(&mut dyn ProcessorImpl) -> R) -> R {
        let result = {
            let mut kernel = self.kernel.try_borrow_mut().unwrap_or_else(|_| {
                panic!(
                    "re-entrant call into a processor kernel; inside a kernel method call \
                     `self.method(this, ..)` instead of `this.method(..)`"
                )
            });
            f(&mut **kernel)
        };
        if self.pending_inputs_changed.replace(false) {
            self.num_inputs_changed();
        }
        result
    }

    /// Runs `f` on the kernel downcast to `T`.
    ///
    /// # Panics
    /// If the kernel is not a `T`, or is already borrowed.
    pub fn with<T: ProcessorImpl, R>(&self, f: impl FnOnce(&mut T, &Processor) -> R) -> R {
        self.with_kernel(|kernel| {
            let kernel = kernel
                .as_any_mut()
                .downcast_mut::<T>()
                .expect("processor kernel has a different type");
            f(kernel, self)
        })
    }

    /// The kernel downcast to `T`, if it is one (borrowed mutably).
    pub fn kernel_mut<T: ProcessorImpl>(&self) -> Option<RefMut<'_, T>> {
        let kernel = self.kernel.try_borrow_mut().ok()?;
        RefMut::filter_map(kernel, |k| k.as_any_mut().downcast_mut::<T>()).ok()
    }

    /// True when the kernel is a `T`.
    pub fn is_kernel<T: ProcessorImpl>(&self) -> bool {
        self.kernel.borrow().as_any().is::<T>()
    }

    pub fn process(&self, num_samples: usize) {
        self.with_kernel(|k| k.process(self, num_samples));
    }

    pub fn process_with_input(&self, audio_in: &Buffer, num_samples: usize) {
        self.with_kernel(|k| k.process_with_input(self, audio_in, num_samples));
    }

    pub fn init(&self) {
        self.with_kernel(|k| k.init(self));
    }

    pub fn reset(&self, reset_mask: PolyMask) {
        self.with_kernel(|k| k.reset(self, reset_mask));
    }

    pub fn hard_reset(&self) {
        self.with_kernel(|k| k.hard_reset(self));
    }

    pub fn set_sample_rate(&self, sample_rate: i32) {
        self.with_kernel(|k| k.set_sample_rate(self, sample_rate));
    }

    pub fn set_oversample_amount(&self, oversample: i32) {
        self.with_kernel(|k| k.set_oversample_amount(self, oversample));
    }

    pub fn enable(&self, enable: bool) {
        self.with_kernel(|k| k.enable(self, enable));
    }

    pub fn set_control_rate(&self, control_rate: bool) {
        self.with_kernel(|k| k.set_control_rate(self, control_rate));
    }

    pub fn correct_to_time(&self, seconds: f64) {
        self.with_kernel(|k| k.correct_to_time(self, seconds));
    }

    pub fn refresh_output(&self, num_samples: usize) {
        self.with_kernel(|k| k.refresh_output(self, num_samples));
    }

    pub fn reset_feedbacks(&self, reset_mask: PolyMask) {
        self.with_kernel(|k| k.reset_feedbacks(self, reset_mask));
    }

    /// `numInputsChanged()`; deferred if the kernel is running.
    pub fn num_inputs_changed(&self) {
        match self.kernel.try_borrow_mut() {
            Ok(mut kernel) => {
                kernel.num_inputs_changed(self);
                drop(kernel);
                if self.pending_inputs_changed.replace(false) {
                    self.num_inputs_changed();
                }
            }
            Err(_) => self.pending_inputs_changed.set(true),
        }
    }

    /// `Value::set` (virtual: `SmoothValue`, `ValueSwitch` override it).
    ///
    /// # Panics
    /// If this is not a `Value`-derived processor.
    pub fn set_value(&self, value: impl Into<PolyFloat>) {
        let value = value.into();
        self.with_kernel(|k| {
            k.as_value()
                .expect("set_value on a processor that is not a Value")
                .set(self, value)
        });
    }

    /// `Value::value()`: lane 0 of the stored value.
    pub fn value(&self) -> f32 {
        self.with_kernel(|k| {
            k.as_value()
                .expect("value() on a processor that is not a Value")
                .value()
        })
    }

    /// `hasState()`.
    pub fn has_state(&self) -> bool {
        self.has_state
    }

    // ----- state -----

    pub fn state(&self) -> &ProcessorState {
        &self.shared.state
    }

    pub fn initialized(&self) -> bool {
        self.state().initialized.get()
    }

    #[inline]
    pub fn enabled(&self) -> bool {
        self.state().enabled.get()
    }

    #[inline]
    pub fn sample_rate(&self) -> i32 {
        self.state().sample_rate.get()
    }

    #[inline]
    pub fn oversample_amount(&self) -> i32 {
        self.state().oversample_amount.get()
    }

    #[inline]
    pub fn is_control_rate(&self) -> bool {
        self.state().control_rate.get()
    }

    pub fn set_plugging_start(&self, start: usize) {
        self.plugging_start.set(start);
    }

    // ----- base (`Processor::`) implementations -----

    /// `Processor::init`.
    pub fn processor_init(&self) {
        debug_assert!(!self.initialized());
        self.state().initialized.set(true);
    }

    /// `Processor::setSampleRate`.
    pub fn processor_set_sample_rate(&self, sample_rate: i32) {
        let state = self.state();
        state
            .sample_rate
            .set(sample_rate * state.oversample_amount.get());
    }

    /// `Processor::setOversampleAmount`.
    pub fn processor_set_oversample_amount(&self, oversample: i32) {
        let state = self.state();
        state
            .sample_rate
            .set(state.sample_rate.get() / state.oversample_amount.get());
        state.oversample_amount.set(oversample);
        state
            .sample_rate
            .set(state.sample_rate.get() * state.oversample_amount.get());

        let size = MAX_BUFFER_SIZE * oversample as usize;
        for output in self.owned_outputs.borrow().iter() {
            output.ensure_buffer_size(size);
        }
        for i in 0..self.num_outputs() {
            self.output(i).ensure_buffer_size(size);
        }
    }

    /// `Processor::enable`.
    pub fn processor_enable(&self, enable: bool) {
        self.state().enabled.set(enable);
    }

    pub(crate) fn default_process(&self, num_samples: usize) {
        if self.router_state.is_some() {
            self.router_process(num_samples);
        } else {
            panic!("processor kernel does not implement process");
        }
    }

    pub(crate) fn default_enable(&self, enable: bool) {
        if self.is_synth_module() {
            self.synth_module_enable(enable);
        } else {
            self.processor_enable(enable);
        }
    }

    pub(crate) fn default_init(&self) {
        if self.router_state.is_some() {
            self.router_init();
        } else {
            self.processor_init();
        }
    }

    pub(crate) fn default_set_sample_rate(&self, sample_rate: i32) {
        if self.router_state.is_some() {
            self.router_set_sample_rate(sample_rate);
        } else {
            self.processor_set_sample_rate(sample_rate);
        }
    }

    pub(crate) fn default_set_oversample_amount(&self, oversample: i32) {
        if self.router_state.is_some() {
            self.router_set_oversample_amount(oversample);
        } else {
            self.processor_set_oversample_amount(oversample);
        }
    }

    // ----- inputs and outputs -----

    #[inline]
    pub fn num_inputs(&self) -> usize {
        self.shared.inputs.borrow().len()
    }

    #[inline]
    pub fn num_outputs(&self) -> usize {
        self.shared.outputs.borrow().len()
    }

    pub fn num_owned_inputs(&self) -> usize {
        self.owned_inputs.borrow().len()
    }

    pub fn num_owned_outputs(&self) -> usize {
        self.owned_outputs.borrow().len()
    }

    /// `input(index)`.
    ///
    /// # Panics
    /// If the slot is out of range or empty.
    #[inline]
    pub fn input(&self, index: usize) -> Rc<Input> {
        self.shared.inputs.borrow()[index]
            .clone()
            .expect("empty input slot")
    }

    /// `input(index)`, or `None` for an empty slot.
    pub fn input_slot(&self, index: usize) -> Option<Rc<Input>> {
        self.shared.inputs.borrow().get(index).cloned().flatten()
    }

    /// `output(index)`.
    ///
    /// # Panics
    /// If the slot is out of range or empty.
    #[inline]
    pub fn output(&self, index: usize) -> Rc<Output> {
        self.shared.outputs.borrow()[index]
            .clone()
            .expect("empty output slot")
    }

    pub fn owned_input(&self, index: usize) -> Rc<Input> {
        self.owned_inputs.borrow()[index].clone()
    }

    pub fn owned_output(&self, index: usize) -> Rc<Output> {
        self.owned_outputs.borrow()[index].clone()
    }

    /// `input(index)->source->buffer`.
    #[inline]
    pub fn input_buffer(&self, index: usize) -> Buffer {
        self.input(index).buffer()
    }

    /// `output(index)->buffer`.
    #[inline]
    pub fn output_buffer(&self, index: usize) -> Buffer {
        self.output(index).buffer()
    }

    /// `getResetMask(input_index)`: lanes whose trigger value is `kVoiceOn`.
    #[inline]
    pub fn get_reset_mask(&self, input_index: usize) -> PolyMask {
        let trigger_value = self.input(input_index).trigger_value();
        PolyFloat::equal(
            trigger_value,
            PolyFloat::splat(VoiceEvent::VoiceOn.as_f32()),
        )
    }

    /// `clearOutputBufferForReset`.
    pub fn clear_output_buffer_for_reset(
        &self,
        reset_mask: PolyMask,
        input_index: usize,
        output_index: usize,
    ) {
        let audio_out = self.output_buffer(output_index);
        let trigger_offset = self.input(input_index).trigger_offset() & reset_mask;
        let num_samples_first = trigger_offset.get(0) as i32;
        let mask = PolyInt::new(0, 0, u32::MAX, u32::MAX);
        for sample in audio_out.iter().take(num_samples_first.max(0) as usize) {
            sample.set(sample.get() & mask);
        }

        let mask = PolyInt::new(u32::MAX, u32::MAX, 0, 0);
        let num_samples_second = trigger_offset.get(2) as i32;
        for sample in audio_out.iter().take(num_samples_second.max(0) as usize) {
            sample.set(sample.get() & mask);
        }
    }

    /// `inputMatchesBufferSize(input)`.
    pub fn input_matches_buffer_size(&self, input: usize) -> bool {
        if input >= self.num_inputs() {
            return false;
        }
        if self.num_outputs() == 0 {
            return true;
        }
        self.input(input).buffer_size() >= self.output(0).buffer_size()
    }

    /// `checkInputAndOutputSize(num_samples)`.
    pub fn check_input_and_output_size(&self, num_samples: usize) -> bool {
        if self.is_control_rate() {
            return true;
        }
        for i in 0..self.num_outputs() {
            let buffer_size = self.output(i).buffer_size();
            if buffer_size > 1 && buffer_size < num_samples {
                return false;
            }
        }
        for i in 0..self.num_inputs() {
            let buffer_size = self.input(i).buffer_size();
            if buffer_size > 1 && buffer_size < num_samples {
                return false;
            }
        }
        true
    }

    /// `isPolyphonic()`.
    pub fn is_polyphonic(&self) -> bool {
        match self.router() {
            Some(router) => router.router_is_polyphonic(self),
            None => false,
        }
    }

    /// `isInputSourcePolyphonic(index)`.
    pub fn is_input_source_polyphonic(&self, index: usize) -> bool {
        self.input(index)
            .source_owner()
            .is_some_and(|owner| owner.is_polyphonic())
    }

    /// `plug(source, input_index)`: connects an output (or a processor's
    /// first output) to an input.
    pub fn plug(&self, source: impl PlugSource, input_index: usize) {
        let source = source.plug_output();
        self.input(input_index).set_source(Some(source.clone()));

        if let Some(router) = self.router() {
            router.connect(&self.this_ref(), &source, input_index);
        }

        self.num_inputs_changed();
    }

    /// `plugNext(source)`: the first unconnected input at or after the
    /// plugging start, or a new input.
    pub fn plug_next(&self, source: impl PlugSource) {
        let source = source.plug_output();
        let num_inputs = self.num_inputs();
        for i in self.plugging_start.get()..num_inputs {
            if let Some(input) = self.input_slot(i) {
                if input.is_null_source() {
                    self.plug(source, i);
                    return;
                }
            }
        }

        let input = Input::new();
        self.owned_inputs.borrow_mut().push(input.clone());
        input.set_source(Some(source));
        self.register_input(input);
        self.num_inputs_changed();
    }

    /// `useInput(input, index)`: shares another processor's input object.
    pub fn use_input(&self, input: &Rc<Input>, index: usize) {
        self.shared.inputs.borrow_mut()[index] = Some(input.clone());
        self.num_inputs_changed();
    }

    /// `useOutput(output, index)`: shares another processor's output object.
    pub fn use_output(&self, output: &Rc<Output>, index: usize) {
        self.shared.outputs.borrow_mut()[index] = Some(output.clone());
    }

    /// `connectedInputs()`.
    pub fn connected_inputs(&self) -> usize {
        self.shared
            .inputs
            .borrow()
            .iter()
            .flatten()
            .filter(|input| !input.is_null_source())
            .count()
    }

    /// `unplugIndex(input_index)`.
    pub fn unplug_index(&self, input_index: usize) {
        if let Some(input) = self.input_slot(input_index) {
            input.set_source(None);
        }
        self.num_inputs_changed();
    }

    /// `unplug(const Output*)`.
    pub fn unplug_output(&self, source: &Rc<Output>) {
        if let Some(router) = self.router() {
            router.disconnect(self, source);
        }
        let inputs: Vec<_> = self
            .shared
            .inputs
            .borrow()
            .iter()
            .flatten()
            .cloned()
            .collect();
        for input in inputs {
            if input.source_is(source) {
                input.set_source(None);
            }
        }
        self.num_inputs_changed();
    }

    /// `unplug(const Processor*)`.
    pub fn unplug_processor(&self, source: &Processor) {
        if let Some(router) = self.router() {
            for i in 0..source.num_outputs() {
                router.disconnect(self, &source.output(i));
            }
        }
        let inputs: Vec<_> = self
            .shared
            .inputs
            .borrow()
            .iter()
            .flatten()
            .cloned()
            .collect();
        for input in inputs {
            if input.source_owner().is_some_and(|owner| owner.is(source)) {
                input.set_source(None);
            }
        }
        self.num_inputs_changed();
    }

    /// `router(router)`: sets the router that owns this processor.
    pub fn set_router(&self, router: Option<&ProcessorRef>) {
        debug_assert!(router.is_none_or(|r| !r.is(self)));
        *self.router.borrow_mut() = router.map(Rc::downgrade).unwrap_or_default();
    }

    /// `router()`.
    pub fn router(&self) -> Option<ProcessorRef> {
        self.router.borrow().upgrade()
    }

    /// `getTopLevelRouter()`.
    pub fn top_level_router(&self) -> Option<ProcessorRef> {
        let mut top_level = None;
        let mut current_level = self.router();
        while let Some(level) = current_level {
            current_level = level.router();
            top_level = Some(level);
        }
        top_level
    }

    /// `registerInput(input, index)`.
    pub fn register_input_at(&self, input: Rc<Input>, index: usize) {
        {
            let mut inputs = self.shared.inputs.borrow_mut();
            while inputs.len() <= index {
                inputs.push(None);
            }
            inputs[index] = Some(input.clone());
        }

        if let (Some(router), Some(source)) = (self.router(), input.source()) {
            router.connect(&self.this_ref(), &source, index);
        }
    }

    /// `registerInput(input)`: appends.
    pub fn register_input(&self, input: Rc<Input>) {
        let index = {
            let mut inputs = self.shared.inputs.borrow_mut();
            inputs.push(Some(input.clone()));
            inputs.len() - 1
        };

        if let (Some(router), Some(source)) = (self.router(), input.source()) {
            router.connect(&self.this_ref(), &source, index);
        }
    }

    /// `registerOutput(output, index)`.
    pub fn register_output_at(&self, output: Rc<Output>, index: usize) -> Rc<Output> {
        let mut outputs = self.shared.outputs.borrow_mut();
        while outputs.len() <= index {
            outputs.push(None);
        }
        outputs[index] = Some(output.clone());
        output
    }

    /// `registerOutput(output)`: appends.
    pub fn register_output(&self, output: Rc<Output>) -> Rc<Output> {
        self.shared.outputs.borrow_mut().push(Some(output.clone()));
        output
    }

    /// `addOutput(oversample)`: a new owned output (control-rate if this
    /// processor is).
    pub fn add_output(&self, oversample: usize) -> Rc<Output> {
        let output = if self.is_control_rate() {
            Output::new_control_rate()
        } else {
            Output::new(MAX_BUFFER_SIZE, oversample)
        };
        self.owned_outputs.borrow_mut().push(output.clone());
        output.set_owner(Some(&self.this_ref()));
        self.register_output(output)
    }

    /// `addInput()`: a new owned input reading the null source.
    pub fn add_input(&self) -> Rc<Input> {
        let input = Input::new();
        self.owned_inputs.borrow_mut().push(input.clone());
        self.register_input(input.clone());
        input
    }

    /// All input slots (shared with clones).
    pub fn input_slots(&self) -> Vec<Option<Rc<Input>>> {
        self.shared.inputs.borrow().clone()
    }

    /// True when this node has routing tables (`ProcessorRouter`).
    pub fn is_router(&self) -> bool {
        self.router_state.is_some()
    }
}

/// Things that can be plugged into an input: an output, or a processor
/// (meaning its first output), as with upstream's `plug` overloads.
pub trait PlugSource {
    fn plug_output(self) -> Rc<Output>;
}

impl PlugSource for Rc<Output> {
    fn plug_output(self) -> Rc<Output> {
        self
    }
}

impl PlugSource for &Rc<Output> {
    fn plug_output(self) -> Rc<Output> {
        self.clone()
    }
}

impl PlugSource for &Processor {
    fn plug_output(self) -> Rc<Output> {
        self.output(0)
    }
}

impl PlugSource for &ProcessorRef {
    fn plug_output(self) -> Rc<Output> {
        self.output(0)
    }
}

impl PlugSource for ProcessorRef {
    fn plug_output(self) -> Rc<Output> {
        self.output(0)
    }
}
