// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::CircularQueue`: an ordered ring buffer with removal by value.
//!
//! Ports `vital/src/synthesis/framework/circular_queue.h`, keeping its
//! order semantics (`remove` shifts later entries down; `push_front` and
//! `pop_front` are O(1)) and its capacity growth rules. Comparisons use
//! `PartialEq`; wrap `Rc`s in a pointer-equality newtype if identity is
//! meant.

/// `vital::CircularQueue<T>`.
#[derive(Clone, Debug)]
pub struct CircularQueue<T: Clone> {
    data: Vec<Option<T>>,
    /// Storage size: requested capacity + 1.
    capacity: usize,
    start: usize,
    end: usize,
}

impl<T: Clone> Default for CircularQueue<T> {
    /// `CircularQueue()`: no storage; call [`reserve`](Self::reserve) first.
    fn default() -> Self {
        CircularQueue {
            data: Vec::new(),
            capacity: 0,
            start: 0,
            end: 0,
        }
    }
}

impl<T: Clone> CircularQueue<T> {
    /// `CircularQueue(capacity)`.
    pub fn new(capacity: usize) -> Self {
        CircularQueue {
            data: vec![None; capacity + 1],
            capacity: capacity + 1,
            start: 0,
            end: 0,
        }
    }

    /// `reserve(capacity)`: grows (never shrinks) and re-packs from index 0.
    pub fn reserve(&mut self, capacity: usize) {
        let new_capacity = capacity + 1;
        if new_capacity < self.capacity {
            return;
        }

        let mut tmp = vec![None; new_capacity];
        if self.capacity > 0 {
            let size = self.len();
            for (i, slot) in tmp.iter_mut().enumerate().take(size) {
                *slot = self.data[(self.start + i) % self.capacity].take();
            }
            self.end = size;
        }
        self.data = tmp;
        self.capacity = new_capacity;
        self.start = 0;
    }

    /// `assign(num, value)`.
    pub fn assign(&mut self, num: usize, value: T) {
        if num > self.capacity {
            self.reserve(num);
        }
        for slot in self.data.iter_mut().take(num) {
            *slot = Some(value.clone());
        }
        self.start = 0;
        self.end = num;
    }

    /// `at(index)`.
    pub fn at(&self, index: usize) -> &T {
        debug_assert!(index < self.len());
        self.data[(self.start + index) % self.capacity]
            .as_ref()
            .expect("CircularQueue slot is empty")
    }

    /// `at(index)` (mutable).
    pub fn at_mut(&mut self, index: usize) -> &mut T {
        debug_assert!(index < self.len());
        let capacity = self.capacity;
        self.data[(self.start + index) % capacity]
            .as_mut()
            .expect("CircularQueue slot is empty")
    }

    pub fn push_back(&mut self, entry: T) {
        self.data[self.end] = Some(entry);
        self.end = (self.end + 1) % self.capacity;
        debug_assert!(self.end != self.start);
    }

    pub fn pop_back(&mut self) -> T {
        debug_assert!(self.end != self.start);
        self.end = (self.end + self.capacity - 1) % self.capacity;
        self.data[self.end]
            .clone()
            .expect("CircularQueue slot is empty")
    }

    pub fn push_front(&mut self, entry: T) {
        self.start = (self.start + self.capacity - 1) % self.capacity;
        self.data[self.start] = Some(entry);
        debug_assert!(self.end != self.start);
    }

    pub fn pop_front(&mut self) -> T {
        debug_assert!(self.end != self.start);
        let last = self.start;
        self.start = (self.start + 1) % self.capacity;
        self.data[last]
            .clone()
            .expect("CircularQueue slot is empty")
    }

    /// `removeAt(index)`: shifts the later entries down by one.
    pub fn remove_at(&mut self, index: usize) {
        debug_assert!(self.end != self.start);
        let mut i = (index + self.start) % self.capacity;
        self.end = (self.end + self.capacity - 1) % self.capacity;
        while i != self.end {
            let next = (i + 1) % self.capacity;
            self.data[i] = self.data[next].clone();
            i = next;
        }
    }

    /// `ensureSpace(space)`.
    pub fn ensure_space(&mut self, space: usize) {
        if self.len() + space >= self.capacity() {
            self.reserve(self.capacity + self.capacity.max(space));
        }
    }

    /// `ensureCapacity(min_capacity)`.
    pub fn ensure_capacity(&mut self, min_capacity: usize) {
        if min_capacity >= self.capacity() {
            self.reserve(self.capacity + self.capacity.max(min_capacity));
        }
    }

    pub fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
    }

    pub fn front(&self) -> &T {
        self.data[self.start]
            .as_ref()
            .expect("CircularQueue is empty")
    }

    pub fn back(&self) -> &T {
        self.data[(self.end + self.capacity - 1) % self.capacity]
            .as_ref()
            .expect("CircularQueue is empty")
    }

    /// `size()`.
    pub fn len(&self) -> usize {
        debug_assert!(self.capacity > 0);
        (self.end + self.capacity - self.start) % self.capacity
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// `capacity()`: usable slots.
    pub fn capacity(&self) -> usize {
        self.capacity.saturating_sub(1)
    }

    /// Entries in queue order.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len()).map(move |i| self.at(i))
    }

    /// `sort<compare>()`: insertion sort, moving an entry before every entry
    /// for which `compare(existing, entry) < 0`.
    pub fn sort(&mut self, compare: impl Fn(&T, &T) -> i32) {
        let total = self.len();
        for i in 1..total {
            let value = self.at(i).clone();
            let mut j = i as isize - 1;
            while j >= 0 && compare(self.at(j as usize), &value) < 0 {
                let moved = self.at(j as usize).clone();
                *self.at_mut(j as usize + 1) = moved;
                j -= 1;
            }
            *self.at_mut((j + 1) as usize) = value;
        }
    }
}

impl<T: Clone + PartialEq> CircularQueue<T> {
    /// `remove(entry)`: the first matching entry.
    pub fn remove(&mut self, entry: &T) {
        let mut i = self.start;
        while i != self.end {
            if self.data[i].as_ref() == Some(entry) {
                self.remove_at((i + self.capacity - self.start) % self.capacity);
                return;
            }
            i = (i + 1) % self.capacity;
        }
    }

    /// `removeAll(entry)`.
    pub fn remove_all(&mut self, entry: &T) {
        let mut i = self.start;
        while i != self.end {
            if self.data[i].as_ref() == Some(entry) {
                self.remove_at((i + self.capacity - self.start) % self.capacity);
            } else {
                i = (i + 1) % self.capacity;
            }
        }
    }

    /// `count(entry)`.
    pub fn count(&self, entry: &T) -> usize {
        self.iter().filter(|e| *e == entry).count()
    }

    /// `contains(entry)`.
    pub fn contains(&self, entry: &T) -> bool {
        self.iter().any(|e| e == entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents(q: &CircularQueue<i32>) -> Vec<i32> {
        q.iter().copied().collect()
    }

    #[test]
    fn order_and_removal() {
        let mut q = CircularQueue::new(4);
        q.push_back(1);
        q.push_back(2);
        q.push_back(3);
        q.push_front(0);
        assert_eq!(contents(&q), [0, 1, 2, 3]);
        q.remove(&2);
        assert_eq!(contents(&q), [0, 1, 3]);
        q.push_back(1);
        q.remove_all(&1);
        assert_eq!(contents(&q), [0, 3]);
        assert_eq!(q.pop_front(), 0);
        assert_eq!(q.pop_back(), 3);
        assert!(q.is_empty());
    }

    #[test]
    fn wraps_and_grows() {
        let mut q = CircularQueue::new(3);
        for i in 0..3 {
            q.push_back(i);
        }
        q.pop_front();
        q.push_back(3);
        assert_eq!(contents(&q), [1, 2, 3]);
        q.ensure_space(2);
        assert!(q.capacity() >= 5);
        q.push_back(4);
        q.push_back(5);
        assert_eq!(contents(&q), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn sort_descending_compare() {
        let mut q = CircularQueue::new(8);
        for v in [3, 1, 4, 1, 5] {
            q.push_back(v);
        }
        q.sort(|a, b| a - b);
        assert_eq!(contents(&q), [5, 4, 3, 1, 1]);
    }
}
