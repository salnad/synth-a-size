// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::CircularQueue` at the level of its raw storage slots.
//!
//! Ports `vital/src/synthesis/framework/circular_queue.h` for the voice
//! handler. `sas_dsp_core::CircularQueue` keeps the same order semantics, but
//! the voice handler also depends on two storage-level behaviours that only a
//! slot-exact replica reproduces:
//!
//! - range-for loops over `active_voices_` whose body removes and pushes
//!   voices (`VoiceHandler::noteOff` with stealing). A C++ range-for captures
//!   `begin()` and `end()` once and walks raw slots, wrapping at the last
//!   slot, so later shifts and pushes change what the loop visits;
//! - `grabNextUnplayedPressedNote` dereferences `end()` when every pressed
//!   note is playing, which reads the stale slot past the last entry.
//!
//! Storage is value-initialised (`std::make_unique<T[]>`), and removals copy
//! entries down without clearing the vacated slot, as upstream.

/// `vital::CircularQueue<T>` with slot access.
#[derive(Clone, Debug)]
pub struct SlotQueue<T: Copy + Default + PartialEq> {
    data: Vec<T>,
    /// Storage size: requested capacity + 1.
    capacity: usize,
    start: usize,
    end: usize,
}

impl<T: Copy + Default + PartialEq> Default for SlotQueue<T> {
    fn default() -> Self {
        SlotQueue {
            data: Vec::new(),
            capacity: 0,
            start: 0,
            end: 0,
        }
    }
}

impl<T: Copy + Default + PartialEq> SlotQueue<T> {
    /// `reserve(capacity)`.
    pub fn reserve(&mut self, capacity: usize) {
        let new_capacity = capacity + 1;
        if new_capacity < self.capacity {
            return;
        }
        let mut tmp = vec![T::default(); new_capacity];
        if self.capacity > 0 {
            self.end = self.len();
            for (i, slot) in tmp.iter_mut().enumerate().take(self.end) {
                *slot = self.at(i);
            }
        }
        self.data = tmp;
        self.capacity = new_capacity;
        self.start = 0;
    }

    #[inline]
    pub fn at(&self, index: usize) -> T {
        debug_assert!(index < self.len());
        self.data[(self.start + index) % self.capacity]
    }

    #[inline]
    pub fn push_back(&mut self, entry: T) {
        self.data[self.end] = entry;
        self.end = (self.end + 1) % self.capacity;
        debug_assert!(self.end != self.start);
    }

    #[inline]
    pub fn push_front(&mut self, entry: T) {
        self.start = (self.start + self.capacity - 1) % self.capacity;
        self.data[self.start] = entry;
        debug_assert!(self.end != self.start);
    }

    #[inline]
    pub fn pop_front(&mut self) -> T {
        debug_assert!(self.end != self.start);
        let last = self.start;
        self.start = (self.start + 1) % self.capacity;
        self.data[last]
    }

    /// `removeAt(index)`.
    pub fn remove_at(&mut self, index: usize) {
        debug_assert!(self.end != self.start);
        let mut i = (index + self.start) % self.capacity;
        self.end = (self.end + self.capacity - 1) % self.capacity;
        while i != self.end {
            let next = (i + 1) % self.capacity;
            self.data[i] = self.data[next];
            i = next;
        }
    }

    /// `remove(entry)`: the first match.
    pub fn remove(&mut self, entry: T) {
        let mut i = self.start;
        while i != self.end {
            if self.data[i] == entry {
                self.remove_at((i + self.capacity - self.start) % self.capacity);
                return;
            }
            i = (i + 1) % self.capacity;
        }
    }

    /// `removeAll(entry)`.
    pub fn remove_all(&mut self, entry: T) {
        let mut i = self.start as isize;
        while i as usize != self.end {
            if self.data[i as usize] == entry {
                self.remove_at((i as usize + self.capacity - self.start) % self.capacity);
                i -= 1;
            }
            // `i--` then `i = (i + 1) % capacity_`: from slot 0, `i` reaches
            // -1 and the modulo brings it back to 0.
            i = (i + 1).rem_euclid(self.capacity as isize);
        }
    }

    /// `sort<compare>()`: insertion sort, moving entries while
    /// `compare(at(j), value) < 0`.
    pub fn sort(&mut self, compare: impl Fn(T, T) -> i32) {
        let total = self.len();
        for i in 1..total {
            let value = self.at(i);
            let mut j = i as isize - 1;
            while j >= 0 && compare(self.at(j as usize), value) < 0 {
                let moved = self.at(j as usize);
                self.set(j as usize + 1, moved);
                j -= 1;
            }
            self.set((j + 1) as usize, value);
        }
    }

    #[inline]
    fn set(&mut self, index: usize, value: T) {
        let slot = (self.start + index) % self.capacity;
        self.data[slot] = value;
    }

    pub fn count(&self, entry: T) -> usize {
        self.iter().filter(|&e| e == entry).count()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
    }

    #[inline]
    pub fn back(&self) -> T {
        self.data[(self.end + self.capacity - 1) % self.capacity]
    }

    #[inline]
    pub fn len(&self) -> usize {
        debug_assert!(self.capacity > 0);
        (self.end + self.capacity - self.start) % self.capacity
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entries in order (a read-only walk; the queue must not change).
    pub fn iter(&self) -> impl Iterator<Item = T> + '_ {
        (0..self.len()).map(move |i| self.at(i))
    }

    // ----- raw slots (C++ iterators) -----

    /// `begin()` as a slot index.
    #[inline]
    pub fn begin_slot(&self) -> usize {
        self.start
    }

    /// `end()` as a slot index.
    #[inline]
    pub fn end_slot(&self) -> usize {
        self.end
    }

    /// `iterator::increment`: the next slot, wrapping after the last one.
    #[inline]
    pub fn next_slot(&self, slot: usize) -> usize {
        if slot == self.capacity - 1 {
            0
        } else {
            slot + 1
        }
    }

    /// `iterator::decrement`.
    #[inline]
    pub fn prev_slot(&self, slot: usize) -> usize {
        if slot == 0 {
            self.capacity - 1
        } else {
            slot - 1
        }
    }

    /// `*iterator`: whatever the slot holds, live or stale.
    #[inline]
    pub fn slot(&self, slot: usize) -> T {
        self.data[slot]
    }

    /// `erase(iterator)`.
    pub fn erase_slot(&mut self, slot: usize) {
        self.remove_at((slot + self.capacity - self.start) % self.capacity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue(capacity: usize, entries: &[i32]) -> SlotQueue<i32> {
        let mut q = SlotQueue::default();
        q.reserve(capacity);
        for &e in entries {
            q.push_back(e);
        }
        q
    }

    #[test]
    fn remove_keeps_stale_slot() {
        let mut q = queue(4, &[1, 2, 3]);
        q.remove(2);
        assert_eq!(q.iter().collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(q.slot(q.end_slot()), 3);
    }

    #[test]
    fn remove_all_and_wrap() {
        let mut q = queue(4, &[]);
        q.push_back(7);
        q.push_back(7);
        q.pop_front();
        q.pop_front();
        for e in [5, 7, 5, 7] {
            q.push_back(e);
        }
        q.remove_all(7);
        assert_eq!(q.iter().collect::<Vec<_>>(), vec![5, 5]);
    }

    #[test]
    fn sort_matches_insertion_order() {
        let mut q = queue(8, &[3, 1, 4, 1, 5]);
        q.sort(|a, b| a - b);
        assert_eq!(q.iter().collect::<Vec<_>>(), vec![5, 4, 3, 1, 1]);
    }
}
