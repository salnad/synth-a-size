// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `Output`, `cr::Output` and `Input`.
//!
//! Ports the `Output`/`Input` structs of
//! `vital/src/synthesis/framework/processor.h`.
//!
//! Buffers are [`Buffer`]s (`Rc<[Cell<PolyFloat>]>`): upstream freely points
//! one output's buffer at another's storage (`ValueSwitch`, `FiltersModule`)
//! and lets processors read and write the same memory, which `Cell` makes
//! sound without `unsafe`.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use sas_simd::common::{MAX_BUFFER_SIZE, MAX_OVERSAMPLE};
use sas_simd::poly_utils as utils;
use sas_simd::{PolyFloat, PolyInt, PolyMask};

use crate::processor::{Processor, ProcessorRef};

/// A shared, aliasable sample buffer (`poly_float*`).
pub type Buffer = Rc<[Cell<PolyFloat>]>;

/// A zeroed buffer of `size` samples.
pub fn new_buffer(size: usize) -> Buffer {
    (0..size).map(|_| Cell::new(PolyFloat::zero())).collect()
}

thread_local! {
    /// Storage read through unconnected inputs (`Processor::null_source_`):
    /// `kMaxBufferSize * kMaxOversample` zeros. Never written by upstream.
    static NULL_BUFFER: Buffer = new_buffer(NULL_SOURCE_SIZE);
}

/// Buffer size of the null source.
pub const NULL_SOURCE_SIZE: usize = MAX_BUFFER_SIZE * MAX_OVERSAMPLE;

/// `vital::Output` (and `cr::Output` via [`Output::new_control_rate`]).
pub struct Output {
    buffer: RefCell<Buffer>,
    owned_buffer: RefCell<Buffer>,
    buffer_size: Cell<usize>,
    trigger_mask: Cell<PolyMask>,
    /// One element. A control-rate output's `buffer` is this storage, as in
    /// upstream (`buffer = &trigger_value`).
    trigger_value: Buffer,
    trigger_offset: Cell<PolyInt>,
    owner: RefCell<Weak<Processor>>,
}

impl Output {
    /// `Output(size, max_oversample)`: an audio-rate output of
    /// `size * max_oversample` samples.
    pub fn new(size: usize, max_oversample: usize) -> Rc<Output> {
        assert!(size > 0);
        let buffer_size = size * max_oversample;
        let owned = new_buffer(buffer_size);
        let output = Output {
            buffer: RefCell::new(owned.clone()),
            owned_buffer: RefCell::new(owned),
            buffer_size: Cell::new(buffer_size),
            trigger_mask: Cell::new(PolyMask::zero()),
            trigger_value: new_buffer(1),
            trigger_offset: Cell::new(PolyInt::zero()),
            owner: RefCell::new(Weak::new()),
        };
        output.clear_buffer();
        output.clear_trigger();
        Rc::new(output)
    }

    /// `Output()` with the default size (`kMaxBufferSize`, no oversampling).
    pub fn new_default() -> Rc<Output> {
        Self::new(MAX_BUFFER_SIZE, 1)
    }

    /// `cr::Output`: one sample, and the buffer *is* the trigger value.
    pub fn new_control_rate() -> Rc<Output> {
        let trigger_value = new_buffer(1);
        let output = Output {
            buffer: RefCell::new(trigger_value.clone()),
            owned_buffer: RefCell::new(new_buffer(1)),
            buffer_size: Cell::new(1),
            trigger_mask: Cell::new(PolyMask::zero()),
            trigger_value,
            trigger_offset: Cell::new(PolyInt::zero()),
            owner: RefCell::new(Weak::new()),
        };
        output.clear_buffer();
        output.clear_trigger();
        Rc::new(output)
    }

    /// The current buffer (`buffer`). May be another output's storage.
    #[inline]
    pub fn buffer(&self) -> Buffer {
        self.buffer.borrow().clone()
    }

    /// Points `buffer` at other storage (`output->buffer = ...`).
    #[inline]
    pub fn set_buffer(&self, buffer: Buffer) {
        *self.buffer.borrow_mut() = buffer;
    }

    /// This output's own storage (`owned_buffer`).
    #[inline]
    pub fn owned_buffer(&self) -> Buffer {
        self.owned_buffer.borrow().clone()
    }

    /// True when `buffer` points at the owned storage.
    pub fn buffer_is_owned(&self) -> bool {
        Rc::ptr_eq(&self.buffer.borrow(), &self.owned_buffer.borrow())
    }

    /// `buffer[index]`.
    #[inline]
    pub fn at(&self, index: usize) -> PolyFloat {
        self.buffer.borrow()[index].get()
    }

    /// `buffer[index] = value`.
    #[inline]
    pub fn set_at(&self, index: usize, value: PolyFloat) {
        self.buffer.borrow()[index].set(value);
    }

    #[inline]
    pub fn buffer_size(&self) -> usize {
        self.buffer_size.get()
    }

    /// Sets `buffer_size` without reallocating (`output->buffer_size = n`).
    #[inline]
    pub fn set_buffer_size(&self, size: usize) {
        self.buffer_size.set(size);
    }

    #[inline]
    pub fn trigger_mask(&self) -> PolyMask {
        self.trigger_mask.get()
    }

    #[inline]
    pub fn set_trigger_mask(&self, mask: PolyMask) {
        self.trigger_mask.set(mask);
    }

    #[inline]
    pub fn trigger_value(&self) -> PolyFloat {
        self.trigger_value[0].get()
    }

    #[inline]
    pub fn set_trigger_value(&self, value: PolyFloat) {
        self.trigger_value[0].set(value);
    }

    #[inline]
    pub fn trigger_offset(&self) -> PolyInt {
        self.trigger_offset.get()
    }

    #[inline]
    pub fn set_trigger_offset(&self, offset: PolyInt) {
        self.trigger_offset.set(offset);
    }

    /// The processor that owns this output, if any.
    pub fn owner(&self) -> Option<ProcessorRef> {
        self.owner.borrow().upgrade()
    }

    pub fn set_owner(&self, owner: Option<&ProcessorRef>) {
        *self.owner.borrow_mut() = owner.map(Rc::downgrade).unwrap_or_default();
    }

    /// True when `owner` is exactly `processor`.
    pub fn is_owned_by(&self, processor: &Processor) -> bool {
        std::ptr::eq(self.owner.borrow().as_ptr(), processor)
    }

    /// Merges a trigger into the lanes in `mask`.
    #[inline]
    pub fn trigger(&self, mask: PolyMask, value: PolyFloat, offset: PolyInt) {
        self.trigger_mask.set(self.trigger_mask.get() | mask);
        self.set_trigger_value(utils::mask_load(self.trigger_value(), value, mask));
        self.trigger_offset.set(utils::mask_load_int(
            self.trigger_offset.get(),
            offset,
            mask,
        ));
    }

    #[inline]
    pub fn clear_trigger(&self) {
        self.trigger_mask.set(PolyMask::zero());
        self.set_trigger_value(PolyFloat::zero());
        self.trigger_offset.set(PolyInt::zero());
    }

    /// Zeroes the owned storage (not a redirected `buffer`).
    pub fn clear_buffer(&self) {
        for sample in self.owned_buffer.borrow().iter() {
            sample.set(PolyFloat::zero());
        }
    }

    #[inline]
    pub fn is_control_rate(&self) -> bool {
        self.buffer_size.get() == 1
    }

    /// Grows the owned storage to `new_max_buffer_size` (no-op for
    /// control-rate outputs and outputs that are already large enough).
    pub fn ensure_buffer_size(&self, new_max_buffer_size: usize) {
        if self.buffer_size.get() >= new_max_buffer_size || self.buffer_size.get() == 1 {
            return;
        }

        self.buffer_size.set(new_max_buffer_size);
        let buffer_is_original = self.buffer_is_owned();
        let owned = new_buffer(new_max_buffer_size);
        *self.owned_buffer.borrow_mut() = owned.clone();
        if buffer_is_original {
            *self.buffer.borrow_mut() = owned;
        }
        self.clear_buffer();
    }
}

/// `vital::Input`: a reference to the output it reads from.
///
/// An unconnected input reads the null source: zeros, no trigger, buffer size
/// [`NULL_SOURCE_SIZE`], no owner.
#[derive(Default)]
pub struct Input {
    source: RefCell<Option<Rc<Output>>>,
}

impl Input {
    pub fn new() -> Rc<Input> {
        Rc::new(Input::default())
    }

    /// The connected output, or `None` for the null source.
    pub fn source(&self) -> Option<Rc<Output>> {
        self.source.borrow().clone()
    }

    /// `input->source = source` (`None` is `&Processor::null_source_`).
    pub fn set_source(&self, source: Option<Rc<Output>>) {
        *self.source.borrow_mut() = source;
    }

    /// `source == &Processor::null_source_`.
    pub fn is_null_source(&self) -> bool {
        self.source.borrow().is_none()
    }

    /// True when the source is exactly `output`.
    pub fn source_is(&self, output: &Rc<Output>) -> bool {
        self.source
            .borrow()
            .as_ref()
            .is_some_and(|s| Rc::ptr_eq(s, output))
    }

    /// `source->buffer`.
    #[inline]
    pub fn buffer(&self) -> Buffer {
        match &*self.source.borrow() {
            Some(output) => output.buffer(),
            None => NULL_BUFFER.with(Rc::clone),
        }
    }

    /// `at(i)` / `source->buffer[i]`.
    #[inline]
    pub fn at(&self, index: usize) -> PolyFloat {
        match &*self.source.borrow() {
            Some(output) => output.at(index),
            None => PolyFloat::zero(),
        }
    }

    /// `source->buffer_size`.
    pub fn buffer_size(&self) -> usize {
        self.source
            .borrow()
            .as_ref()
            .map_or(NULL_SOURCE_SIZE, |s| s.buffer_size())
    }

    /// `source->isControlRate()`.
    pub fn source_is_control_rate(&self) -> bool {
        self.buffer_size() == 1
    }

    /// `source->trigger_mask`.
    pub fn trigger_mask(&self) -> PolyMask {
        self.source
            .borrow()
            .as_ref()
            .map_or(PolyMask::zero(), |s| s.trigger_mask())
    }

    /// `source->trigger_value`.
    pub fn trigger_value(&self) -> PolyFloat {
        self.source
            .borrow()
            .as_ref()
            .map_or(PolyFloat::zero(), |s| s.trigger_value())
    }

    /// `source->trigger_offset`.
    pub fn trigger_offset(&self) -> PolyInt {
        self.source
            .borrow()
            .as_ref()
            .map_or(PolyInt::zero(), |s| s.trigger_offset())
    }

    /// `source->owner`.
    pub fn source_owner(&self) -> Option<ProcessorRef> {
        self.source.borrow().as_ref().and_then(|s| s.owner())
    }
}
