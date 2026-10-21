// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::Memory` and `vital::StereoMemory`: delay-line storage read with
//! Catmull-Rom interpolation.
//!
//! Ports `vital/src/synthesis/lookups/memory.h`. Each channel stores its
//! samples twice (`[0, size)` and `[size, 2 * size)`) so a 4-sample read
//! never wraps. The read index uses `utils::toInt` (site A1a of
//! `migration/decisions/reference-arm.md`): the interpolation parameter is
//! in `[0.5, 1.5]` on x86_64 and `(0, 1]` on aarch64, as upstream.

use sas_simd::poly_utils as utils;
use sas_simd::utils as mono;
use sas_simd::{Matrix, PolyFloat, PolyInt, PolyMask};

/// `MemoryTemplate<kChannels>`.
#[derive(Debug)]
pub struct MemoryTemplate<const CHANNELS: usize> {
    buffers: [Vec<f32>; CHANNELS],
    size: u32,
    bitmask: u32,
    offset: u32,
}

impl<const CHANNELS: usize> Clone for MemoryTemplate<CHANNELS> {
    /// Upstream's copy constructor: same size and offset, **zeroed** storage.
    fn clone(&self) -> Self {
        MemoryTemplate {
            buffers: std::array::from_fn(|_| vec![0.0; 2 * self.size as usize]),
            size: self.size,
            bitmask: self.bitmask,
            offset: self.offset,
        }
    }
}

impl<const CHANNELS: usize> MemoryTemplate<CHANNELS> {
    pub const MIN_PERIOD: f32 = 2.0;
    pub const EXTRA_INTERPOLATION_VALUES: u32 = 3;

    /// `MemoryTemplate(size)`: rounds `size` up to a power of two.
    pub fn new(size: i32) -> Self {
        let size = mono::next_power_of_two(size as f32) as u32;
        MemoryTemplate {
            buffers: std::array::from_fn(|_| vec![0.0; 2 * size as usize]),
            size,
            bitmask: size - 1,
            offset: 0,
        }
    }

    /// `push(sample)`: one sample per channel (the first `CHANNELS` lanes).
    pub fn push(&mut self, sample: PolyFloat) {
        self.offset = (self.offset + 1) & self.bitmask;
        let lanes = sample.to_array();
        let (offset, size) = (self.offset as usize, self.size as usize);
        for (buffer, &value) in self.buffers.iter_mut().zip(lanes.iter()) {
            buffer[offset] = value;
            buffer[offset + size] = value;
        }
    }

    /// `clearMemory(num, clear_mask)`: zeroes the last `num` samples (plus
    /// interpolation margin) of the channels in `clear_mask`.
    pub fn clear_memory(&mut self, num: i32, clear_mask: PolyMask) {
        let extra = Self::EXTRA_INTERPOLATION_VALUES;
        let start = self.offset.wrapping_sub((num as u32).wrapping_add(extra)) & self.bitmask;
        let end = (self.offset + extra) & self.bitmask;
        let mask = clear_mask.to_array();
        for (p, buffer) in self.buffers.iter_mut().enumerate() {
            if mask[p] != 0 {
                let mut i = start;
                while i != end {
                    buffer[i as usize] = 0.0;
                    i = (i + 1) & self.bitmask;
                }
                buffer[end as usize] = 0.0;
                for i in 0..extra {
                    buffer[(self.size + i) as usize] = 0.0;
                }
            }
        }
    }

    /// `clearAll()`.
    pub fn clear_all(&mut self) {
        for buffer in &mut self.buffers {
            buffer.fill(0.0);
        }
    }

    /// `readSamples(output, num_samples, offset, channel)`.
    pub fn read_samples(
        &self,
        output: &mut [f32],
        num_samples: usize,
        offset: i32,
        channel: usize,
    ) {
        let buffer = &self.buffers[channel];
        let start_index = self
            .offset
            .wrapping_sub(num_samples as u32)
            .wrapping_sub(offset as u32)
            & self.bitmask;
        for (i, out) in output.iter_mut().enumerate().take(num_samples) {
            *out = buffer[((i as u32 + start_index) & self.bitmask) as usize];
        }
    }

    pub fn offset(&self) -> u32 {
        self.offset
    }

    pub fn set_offset(&mut self, offset: i32) {
        self.offset = offset as u32;
    }

    pub fn size(&self) -> i32 {
        self.size as i32
    }

    /// `getMaxPeriod()`.
    pub fn max_period(&self) -> i32 {
        (self.size - Self::EXTRA_INTERPOLATION_VALUES) as i32
    }

    /// Channel storage (`2 * size` samples).
    pub fn buffer(&self, channel: usize) -> &[f32] {
        &self.buffers[channel]
    }

    fn read_parameters(&self, past: PolyFloat) -> (Matrix, PolyInt) {
        let past_index = utils::to_int(past);
        let t = utils::to_float(past_index) - past + 1.0;
        let interpolation_matrix = utils::get_catmull_interpolation_matrix(t);
        let indices = (PolyInt::splat(self.offset) - past_index - 2) & PolyInt::splat(self.bitmask);
        (interpolation_matrix, indices)
    }
}

/// `vital::Memory`: four channels, one per lane.
pub type Memory = MemoryTemplate<4>;

impl MemoryTemplate<4> {
    /// `get(past)`: the value `past` samples ago in each lane.
    pub fn get(&self, past: PolyFloat) -> PolyFloat {
        let (interpolation_matrix, indices) = self.read_parameters(past);
        let mut value_matrix = utils::get_value_matrix_separate(
            [
                &self.buffers[0],
                &self.buffers[1],
                &self.buffers[2],
                &self.buffers[3],
            ],
            indices,
        );
        value_matrix.transpose();
        interpolation_matrix.multiply_and_sum_rows(&value_matrix)
    }
}

/// `vital::StereoMemory`: two channels (lanes 0 and 1).
pub type StereoMemory = MemoryTemplate<2>;

impl MemoryTemplate<2> {
    /// `get(past)`: lanes 0 and 1 read their channel; lanes 2 and 3 are 0.
    pub fn get(&self, past: PolyFloat) -> PolyFloat {
        let (interpolation_matrix, indices) = self.read_parameters(past);
        let i = indices.to_array();
        let mut value_matrix = Matrix::new(
            utils::to_poly_float_from_unaligned(&self.buffers[0][i[0] as usize..]),
            utils::to_poly_float_from_unaligned(&self.buffers[1][i[1] as usize..]),
            0.0,
            0.0,
        );
        value_matrix.transpose();
        interpolation_matrix.multiply_and_sum_rows(&value_matrix)
    }
}
