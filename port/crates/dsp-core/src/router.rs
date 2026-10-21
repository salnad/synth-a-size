// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::ProcessorRouter`: ordered sub-graphs, cycle breaking with
//! `Feedback`, and per-voice copies.
//!
//! Ports `vital/src/synthesis/framework/processor_router.h` and
//! `processor_router.cpp`.
//!
//! A router node keeps a **global order** of the processors added to it,
//! shared with every copy of the router (upstream's `shared_ptr`s), and a
//! **local order** of the instances this copy runs. The original router runs
//! the added processors themselves. A copy (`clone_processor`, one per voice
//! pair) runs clones of the processors that have state and shares the
//! stateless ones. Edits made through any copy bump the shared change
//! counter, and every copy rebuilds its local order lazily on its next
//! `process`/`set_sample_rate`.
//!
//! Upstream's virtual router hooks that `VoiceHandler` overrides
//! (`isPolyphonic`, `getMonoRouter`, `getPolyRouter`, `addProcessor`,
//! `removeProcessor`) are node data here ([`Processor::set_router_hooks`]),
//! so graph edits never need to borrow a running kernel.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use sas_simd::PolyMask;

use crate::feedback::Feedback;
use crate::output::Output;
use crate::processor::{Processor, ProcessorImpl, ProcessorRef};

type Key = usize;

/// The routing tables of a router node.
pub(crate) struct RouterState {
    global_order: Rc<RefCell<Vec<ProcessorRef>>>,
    local_order: RefCell<Vec<ProcessorRef>>,
    /// Global processor → (generation mark, local instance). `None` mirrors
    /// upstream's empty entries created by `std::map::operator[]` for
    /// stateless processors in copies.
    processors: RefCell<HashMap<Key, (i32, Option<ProcessorRef>)>>,
    idle_processors: RefCell<HashMap<Key, ProcessorRef>>,

    global_feedback_order: Rc<RefCell<Vec<ProcessorRef>>>,
    local_feedback_order: RefCell<Vec<ProcessorRef>>,
    feedback_processors: RefCell<HashMap<Key, (i32, Option<ProcessorRef>)>>,

    global_changes: Rc<Cell<i32>>,
    local_changes: Cell<i32>,

    hooks: RefCell<RouterHooks>,
}

/// Router behaviour that upstream subclasses (`VoiceHandler`) override.
#[derive(Clone, Default)]
pub struct RouterHooks {
    /// `isPolyphonic(processor)` is true exactly for this child, instead of
    /// asking the parent router.
    pub polyphonic_child: Option<Weak<Processor>>,
    /// `getMonoRouter()` returns this router.
    pub mono_router: Option<Weak<Processor>>,
    /// `getPolyRouter()` returns this router.
    pub poly_router: Option<Weak<Processor>>,
    /// `addProcessor`, `addIdleProcessor` and `removeProcessor` are
    /// forwarded to this router, as `VoiceHandler` forwards them to its voice
    /// router. Before forwarding an add or idle add, the processor's sample
    /// rate is set to this router's (`processor->setSampleRate(getSampleRate())`).
    pub forward_add_remove: Option<Weak<Processor>>,
}

impl RouterState {
    pub(crate) fn new() -> RouterState {
        RouterState {
            global_order: Rc::new(RefCell::new(Vec::new())),
            local_order: RefCell::new(Vec::new()),
            processors: RefCell::new(HashMap::new()),
            idle_processors: RefCell::new(HashMap::new()),
            global_feedback_order: Rc::new(RefCell::new(Vec::new())),
            local_feedback_order: RefCell::new(Vec::new()),
            feedback_processors: RefCell::new(HashMap::new()),
            global_changes: Rc::new(Cell::new(0)),
            local_changes: Cell::new(0),
            hooks: RefCell::new(RouterHooks::default()),
        }
    }

    /// `ProcessorRouter(const ProcessorRouter&)`: shares the global tables
    /// and clones every processor and feedback node in them.
    pub(crate) fn clone_for_copy(&self) -> RouterState {
        let global_order = self.global_order.clone();
        let global_feedback_order = self.global_feedback_order.clone();

        let mut local_order = Vec::new();
        let mut processors = HashMap::new();
        for next in global_order.borrow().iter() {
            let clone = next.clone_processor();
            local_order.push(clone.clone());
            processors.insert(next.key(), (0, Some(clone)));
        }

        let mut local_feedback_order = Vec::new();
        let mut feedback_processors = HashMap::new();
        for next in global_feedback_order.borrow().iter() {
            let clone = next.clone_processor();
            local_feedback_order.push(clone.clone());
            feedback_processors.insert(next.key(), (0, Some(clone)));
        }

        RouterState {
            global_order,
            local_order: RefCell::new(local_order),
            processors: RefCell::new(processors),
            idle_processors: RefCell::new(HashMap::new()),
            global_feedback_order,
            local_feedback_order: RefCell::new(local_feedback_order),
            feedback_processors: RefCell::new(feedback_processors),
            global_changes: self.global_changes.clone(),
            local_changes: Cell::new(self.local_changes.get()),
            hooks: RefCell::new(self.hooks.borrow().clone()),
        }
    }
}

/// Kernel of a plain `vital::ProcessorRouter` (every method is the router
/// default).
#[derive(Clone, Default)]
pub struct ProcessorRouter;

impl ProcessorImpl for ProcessorRouter {
    crate::processor_boilerplate!();
}

impl ProcessorRouter {
    /// `new ProcessorRouter(num_inputs, num_outputs, control_rate)`.
    pub fn create(num_inputs: usize, num_outputs: usize, control_rate: bool) -> ProcessorRef {
        Processor::new_router(ProcessorRouter, num_inputs, num_outputs, control_rate)
    }
}

fn contains(list: &[ProcessorRef], processor: &Processor) -> bool {
    list.iter().any(|p| p.is(processor))
}

impl Processor {
    fn rs(&self) -> &RouterState {
        self.router_state
            .as_ref()
            .expect("router method called on a processor that is not a router")
    }

    /// Overrides the router hooks (for `VoiceHandler`).
    pub fn set_router_hooks(&self, hooks: RouterHooks) {
        *self.rs().hooks.borrow_mut() = hooks;
    }

    pub fn router_hooks(&self) -> RouterHooks {
        self.rs().hooks.borrow().clone()
    }

    #[inline]
    fn should_update(&self) -> bool {
        let rs = self.rs();
        rs.local_changes.get() != rs.global_changes.get()
    }

    fn bump_changes(&self) {
        let rs = self.rs();
        rs.global_changes.set(rs.global_changes.get() + 1);
        rs.local_changes.set(rs.local_changes.get() + 1);
    }

    /// `ProcessorRouter::process`.
    pub fn router_process(&self, num_samples: usize) {
        if self.should_update() {
            self.update_all_processors();
        }
        let rs = self.rs();

        let num_feedbacks = rs.local_feedback_order.borrow().len();
        for i in 0..num_feedbacks {
            let feedback = rs.local_feedback_order.borrow()[i].clone();
            feedback.refresh_output(num_samples);
        }

        let normal_samples = (num_samples / self.oversample_amount() as usize).max(1);
        let num_processors = rs.local_order.borrow().len();
        for i in 0..num_processors {
            let processor = rs.local_order.borrow()[i].clone();
            if processor.enabled() {
                let processor_samples = normal_samples * processor.oversample_amount() as usize;
                debug_assert!(processor.check_input_and_output_size(processor_samples));
                processor.process(processor_samples);
            }
        }

        for i in 0..num_feedbacks {
            let enabled = rs.global_feedback_order.borrow()[i].enabled();
            if enabled {
                let feedback = rs.local_feedback_order.borrow()[i].clone();
                feedback.process(num_samples);
            }
        }
    }

    /// `ProcessorRouter::init`.
    pub fn router_init(&self) {
        self.processor_init();
        let local: Vec<_> = self.rs().local_order.borrow().clone();
        for processor in local {
            processor.init();
        }
    }

    /// `ProcessorRouter::setSampleRate`.
    pub fn router_set_sample_rate(&self, sample_rate: i32) {
        self.processor_set_sample_rate(sample_rate);
        if self.should_update() {
            self.update_all_processors();
        }
        let local: Vec<_> = self.rs().local_order.borrow().clone();
        for processor in local {
            processor.set_sample_rate(sample_rate);
        }
        let feedbacks: Vec<_> = self.rs().local_feedback_order.borrow().clone();
        for feedback in feedbacks {
            feedback.set_sample_rate(sample_rate);
        }
    }

    /// `ProcessorRouter::setOversampleAmount`.
    pub fn router_set_oversample_amount(&self, oversample: i32) {
        self.processor_set_oversample_amount(oversample);
        if self.should_update() {
            self.update_all_processors();
        }
        let idle: Vec<_> = self
            .rs()
            .idle_processors
            .borrow()
            .values()
            .cloned()
            .collect();
        for processor in idle {
            processor.set_oversample_amount(oversample);
        }
        let local: Vec<_> = self.rs().local_order.borrow().clone();
        for processor in local {
            processor.set_oversample_amount(oversample);
        }
        let feedbacks: Vec<_> = self.rs().local_feedback_order.borrow().clone();
        for feedback in feedbacks {
            feedback.set_oversample_amount(oversample);
        }
    }

    /// `addProcessor(processor)`: takes `processor` into this router's
    /// order (forwarded if a hook says so).
    pub fn add_processor(&self, processor: &ProcessorRef) {
        if let Some(target) = self.forward_target() {
            processor.set_sample_rate(self.sample_rate());
            target.add_processor(processor);
            return;
        }
        self.add_processor_real_time(processor);
    }

    fn forward_target(&self) -> Option<ProcessorRef> {
        let target = self.rs().hooks.borrow().forward_add_remove.clone();
        target.and_then(|target| target.upgrade())
    }

    /// `addProcessorRealTime(processor)`.
    pub fn add_processor_real_time(&self, processor: &ProcessorRef) {
        debug_assert!(processor.router().is_none());
        self.bump_changes();

        let this = self.this_ref();
        processor.set_router(Some(&this));
        if self.oversample_amount() > 1 {
            processor.set_oversample_amount(self.oversample_amount());
        }

        let rs = self.rs();
        rs.global_order.borrow_mut().push(processor.clone());
        rs.processors
            .borrow_mut()
            .insert(processor.key(), (0, Some(processor.clone())));
        rs.local_order.borrow_mut().push(processor.clone());

        for i in 0..processor.num_inputs() {
            if let Some(source) = processor.input_slot(i).and_then(|input| input.source()) {
                self.connect(processor, &source, i);
            } else {
                self.connect_null(processor);
            }
        }
    }

    /// `addIdleProcessor(processor)`: owned (and oversampled) by this router
    /// but not run by it (forwarded if a hook says so).
    pub fn add_idle_processor(&self, processor: &ProcessorRef) {
        if let Some(target) = self.forward_target() {
            processor.set_sample_rate(self.sample_rate());
            target.add_idle_processor(processor);
            return;
        }
        processor.set_router(Some(&self.this_ref()));
        self.rs()
            .idle_processors
            .borrow_mut()
            .insert(processor.key(), processor.clone());
    }

    /// `removeProcessor(processor)`.
    pub fn remove_processor(&self, processor: &ProcessorRef) {
        if let Some(target) = self.forward_target() {
            target.remove_processor(processor);
            return;
        }

        for i in 0..processor.num_inputs() {
            if let Some(source) = processor.input_slot(i).and_then(|input| input.source()) {
                self.disconnect(processor, &source);
            }
        }

        debug_assert!(processor.router().is_some_and(|r| r.is(self)));
        self.bump_changes();
        let rs = self.rs();
        remove_first(&mut rs.global_order.borrow_mut(), processor);
        remove_first(&mut rs.local_order.borrow_mut(), processor);
        processor.set_router(None);
        rs.processors.borrow_mut().remove(&processor.key());
    }

    /// `connect(destination, source, index)`: called whenever an input of
    /// `destination` (a descendant of this router) is plugged into `source`.
    /// Inserts a control-rate `Feedback` if the connection closes a cycle,
    /// otherwise reorders `destination` after its dependencies.
    pub fn connect(&self, destination: &ProcessorRef, source: &Rc<Output>, index: usize) {
        let owner = source.owner();
        if self.is_downstream(destination, owner.as_deref()) {
            let feedback = Feedback::create_control_rate();
            feedback.plug(source, 0);
            destination.plug(&feedback, index);
            self.add_feedback(&feedback);
        } else {
            self.reorder(destination);
        }
    }

    /// `connect` for a null source (whose owner is null upstream).
    fn connect_null(&self, destination: &ProcessorRef) {
        self.reorder(destination);
    }

    /// `disconnect(destination, source)`.
    pub fn disconnect(&self, destination: &Processor, source: &Rc<Output>) {
        let owner = source.owner();
        if !self.is_downstream(destination, owner.as_deref()) {
            return;
        }

        for i in 0..destination.num_inputs() {
            let input = destination.input(i);
            let Some(owner) = input.source_owner() else {
                continue;
            };
            let feedback = self
                .rs()
                .feedback_processors
                .borrow()
                .get(&owner.key())
                .and_then(|(_, f)| f.clone());
            if self
                .rs()
                .feedback_processors
                .borrow()
                .contains_key(&owner.key())
            {
                if let Some(feedback) = feedback {
                    if feedback.input(0).source_is(source) {
                        self.remove_feedback(&feedback);
                    }
                }
                input.set_source(None);
            }
        }
    }

    /// `isDownstream(first, second)`: true when `first` is among the
    /// dependencies of `second` (so connecting `second` into `first` would
    /// close a cycle).
    pub fn is_downstream(&self, first: &Processor, second: Option<&Processor>) -> bool {
        let dependencies = self.get_dependencies(second);
        contains(&dependencies, first)
    }

    /// `areOrdered(first, second)`.
    pub fn are_ordered(&self, first: &Processor, second: &Processor) -> bool {
        let first_context = self.get_context(Some(first));
        let second_context = self.get_context(Some(second));

        if let (Some(first_context), Some(second_context)) = (first_context, second_context) {
            for processor in self.rs().global_order.borrow().iter() {
                if processor.is(&first_context) {
                    return true;
                } else if processor.is(&second_context) {
                    return false;
                }
            }
        } else if let Some(router) = self.router() {
            return router.are_ordered(first, second);
        }
        true
    }

    /// `ProcessorRouter::isPolyphonic(processor)`.
    pub fn router_is_polyphonic(&self, processor: &Processor) -> bool {
        if let Some(child) = &self.rs().hooks.borrow().polyphonic_child {
            return std::ptr::eq(child.as_ptr(), processor);
        }
        match self.router() {
            Some(router) => router.router_is_polyphonic(self),
            None => false,
        }
    }

    /// `getMonoRouter()`.
    pub fn get_mono_router(&self) -> ProcessorRef {
        if let Some(mono) = self
            .rs()
            .hooks
            .borrow()
            .mono_router
            .as_ref()
            .and_then(Weak::upgrade)
        {
            return mono;
        }
        if self.router_is_polyphonic(self) {
            if let Some(router) = self.router() {
                return router.get_mono_router();
            }
        }
        self.this_ref()
    }

    /// `getPolyRouter()`.
    pub fn get_poly_router(&self) -> ProcessorRef {
        if let Some(poly) = self
            .rs()
            .hooks
            .borrow()
            .poly_router
            .as_ref()
            .and_then(Weak::upgrade)
        {
            return poly;
        }
        self.this_ref()
    }

    /// `ProcessorRouter::resetFeedbacks`.
    pub fn router_reset_feedbacks(&self, reset_mask: PolyMask) {
        let feedbacks: Vec<_> = self.rs().local_feedback_order.borrow().clone();
        for feedback in feedbacks {
            feedback.reset(reset_mask);
        }
    }

    /// `addFeedback(feedback)`.
    pub fn add_feedback(&self, feedback: &ProcessorRef) {
        feedback.set_router(Some(&self.this_ref()));
        let rs = self.rs();
        rs.global_feedback_order.borrow_mut().push(feedback.clone());
        rs.local_feedback_order.borrow_mut().push(feedback.clone());
        rs.feedback_processors
            .borrow_mut()
            .insert(feedback.key(), (0, Some(feedback.clone())));
    }

    /// `removeFeedback(feedback)`.
    pub fn remove_feedback(&self, feedback: &ProcessorRef) {
        self.bump_changes();
        let rs = self.rs();
        remove_first(&mut rs.global_feedback_order.borrow_mut(), feedback);
        remove_first(&mut rs.local_feedback_order.borrow_mut(), feedback);
        rs.feedback_processors.borrow_mut().remove(&feedback.key());
    }

    /// `reorder(processor)`: moves `processor`'s dependencies (in their
    /// current order) before it, and everything else after it, then asks the
    /// parent router to do the same.
    pub fn reorder(&self, processor: &Processor) {
        self.bump_changes();

        let dependencies = self.get_dependencies(Some(processor));
        if dependencies.is_empty() {
            if let Some(router) = self.router() {
                router.reorder(processor);
            }
            return;
        }

        let rs = self.rs();
        // Upstream iterates `processors_.size()` entries of the global order.
        let num_processors = rs
            .processors
            .borrow()
            .len()
            .min(rs.global_order.borrow().len());
        let mut reorder: Vec<ProcessorRef> = Vec::with_capacity(num_processors);
        {
            let global_order = rs.global_order.borrow();
            for current in global_order.iter().take(num_processors) {
                if !current.is(processor) && contains(&dependencies, current) {
                    reorder.push(current.clone());
                }
            }

            if rs.processors.borrow().contains_key(&processor.key()) {
                reorder.push(processor.this_ref());
            }

            for current in global_order.iter().take(num_processors) {
                if !current.is(processor) && !contains(&dependencies, current) {
                    reorder.push(current.clone());
                }
            }
        }

        {
            let mut global_order = rs.global_order.borrow_mut();
            for (i, processor) in reorder.into_iter().enumerate().take(num_processors) {
                global_order[i] = processor;
            }
        }

        if let Some(router) = self.router() {
            router.reorder(processor);
        }
    }

    /// `updateAllProcessors()`.
    pub fn update_all_processors(&self) {
        if !self.should_update() {
            return;
        }
        self.create_added_processors();
        self.delete_removed_processors();
        let rs = self.rs();
        rs.local_changes.set(rs.global_changes.get());
    }

    fn create_added_processors(&self) {
        let rs = self.rs();
        let global_order: Vec<_> = rs.global_order.borrow().clone();
        let mut local_order = Vec::with_capacity(global_order.len());
        for next in &global_order {
            if next.has_state() {
                let existing = rs
                    .processors
                    .borrow()
                    .get(&next.key())
                    .map(|(_, local)| local.clone());
                let local = match existing {
                    Some(local) => local,
                    None => {
                        let clone = next.clone_processor();
                        rs.processors
                            .borrow_mut()
                            .insert(next.key(), (0, Some(clone.clone())));
                        Some(clone)
                    }
                };
                local_order
                    .push(local.expect("router has an empty entry for a stateful processor"));
            } else {
                local_order.push(next.clone());
            }
        }
        *rs.local_order.borrow_mut() = local_order;

        let global_feedbacks: Vec<_> = rs.global_feedback_order.borrow().clone();
        let mut local_feedbacks = Vec::with_capacity(global_feedbacks.len());
        for next in &global_feedbacks {
            let existing = rs
                .feedback_processors
                .borrow()
                .get(&next.key())
                .map(|(_, local)| local.clone());
            let local = match existing {
                Some(Some(local)) => local,
                _ => {
                    let clone = next.clone_processor();
                    rs.feedback_processors
                        .borrow_mut()
                        .insert(next.key(), (0, Some(clone.clone())));
                    clone
                }
            };
            local_feedbacks.push(local);
        }
        *rs.local_feedback_order.borrow_mut() = local_feedbacks;
    }

    fn delete_removed_processors(&self) {
        let rs = self.rs();
        let generation = rs.global_changes.get();
        {
            let mut processors = rs.processors.borrow_mut();
            for global in rs.global_order.borrow().iter() {
                processors.entry(global.key()).or_insert((0, None)).0 = generation;
            }
            processors.retain(|_, (mark, _)| *mark == generation);
        }
        {
            let mut feedbacks = rs.feedback_processors.borrow_mut();
            for global in rs.global_feedback_order.borrow().iter() {
                feedbacks.entry(global.key()).or_insert((0, None)).0 = generation;
            }
            feedbacks.retain(|_, (mark, _)| *mark == generation);
        }

        let global_feedbacks: Vec<_> = rs.global_feedback_order.borrow().clone();
        let local_feedbacks = global_feedbacks
            .iter()
            .map(|next| {
                rs.feedback_processors.borrow()[&next.key()]
                    .1
                    .clone()
                    .expect("missing local feedback")
            })
            .collect();
        *rs.local_feedback_order.borrow_mut() = local_feedbacks;
    }

    /// `getContext(processor)`: the ancestor of `processor` that is a direct
    /// child (or idle child) of this router.
    pub fn get_context(&self, processor: Option<&Processor>) -> Option<ProcessorRef> {
        let rs = self.rs();
        let mut context = processor.map(Processor::this_ref);
        while let Some(current) = context.clone() {
            if rs.processors.borrow().contains_key(&current.key())
                || rs.idle_processors.borrow().contains_key(&current.key())
            {
                break;
            }
            context = current.router();
        }
        context
    }

    /// `getLocalProcessor(global_processor)`: this copy's instance of a
    /// processor added to the router.
    pub fn get_local_processor(&self, global_processor: &Processor) -> Option<ProcessorRef> {
        self.rs()
            .processors
            .borrow_mut()
            .entry(global_processor.key())
            .or_insert((0, None))
            .1
            .clone()
    }

    /// `getDependencies(processor)`: the children of this router that
    /// `processor` transitively reads from (through owned inputs), excluding
    /// `processor`'s own context.
    pub fn get_dependencies(&self, processor: Option<&Processor>) -> Vec<ProcessorRef> {
        let mut dependencies: Vec<ProcessorRef> = Vec::new();
        let mut visited: Vec<ProcessorRef> = Vec::new();
        let context = self.get_context(processor);

        let mut dependency_inputs: Vec<Option<ProcessorRef>> =
            vec![processor.map(Processor::this_ref)];
        let mut i = 0;
        while i < dependency_inputs.len() {
            let current = dependency_inputs[i].clone();
            if let Some(dependency) = self.get_context(current.as_deref()) {
                if !contains(&dependencies, &dependency) {
                    dependencies.push(dependency);
                }

                let current = current.expect("a context implies a processor");
                // Upstream reads `ownedInput(j)` for `j < numInputs()`.
                let count = current.num_inputs().min(current.num_owned_inputs());
                for j in 0..count {
                    let input = current.owned_input(j);
                    if let Some(owner) = input.source_owner() {
                        if !contains(&visited, &owner) {
                            dependency_inputs.push(Some(owner.clone()));
                            visited.push(owner);
                        }
                    }
                }
            }
            i += 1;
        }

        if let Some(context) = context {
            dependencies.retain(|p| !p.is(&context));
        }
        dependencies
    }

    /// The processors this copy runs, in order.
    pub fn local_order(&self) -> Vec<ProcessorRef> {
        self.rs().local_order.borrow().clone()
    }

    /// The shared global order.
    pub fn global_order(&self) -> Vec<ProcessorRef> {
        self.rs().global_order.borrow().clone()
    }

    /// This copy's feedback nodes, in order.
    pub fn local_feedback_order(&self) -> Vec<ProcessorRef> {
        self.rs().local_feedback_order.borrow().clone()
    }
}

fn remove_first(list: &mut Vec<ProcessorRef>, processor: &Processor) {
    if let Some(position) = list.iter().position(|p| p.is(processor)) {
        list.remove(position);
    }
}
