// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The on-screen keyboard's notes: ports JUCE's `MidiKeyboardState` as
//! upstream's `MidiKeyboard` and `MidiManager::replaceKeyboardMessages` use
//! it (`vital/src/common/midi_manager.cpp`,
//! `vital/third_party/JUCE/modules/juce_audio_basics/midi/juce_MidiKeyboardState.cpp`).
//!
//! The editor's [`KeyboardHandle`] marks a note held at once and queues its
//! message with a millisecond timestamp. At each block the audio side's
//! [`KeyboardConsumer`] updates the held notes from the host's MIDI, then
//! spreads the queued messages across the block by their timestamps, each
//! after any host message at the same sample (`processNextMidiBuffer`).
//! Nothing here locks, allocates or frees on the audio thread: the queue is
//! bounded and lock-free, and the held notes are atomics.

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crossbeam_queue::ArrayQueue;

/// Messages the editor can queue between two blocks, and that wait while
/// the plugin is bypassed. Further notes are dropped.
pub const KEYBOARD_QUEUE_CAPACITY: usize = 256;

/// `MidiKeyboard::midi_channel_` (MIDI channel 1).
const CHANNEL_BIT: u16 = 1;

/// `MidiKeyboardState::noteOn` and `noteOff` drop queued messages more than
/// this much older than the one they add.
const MAX_AGE_MS: i64 = 500;

const NUM_NOTES: usize = 128;

#[derive(Clone, Copy, Debug, Default)]
struct Queued {
    message: [u8; 3],
    millis: i64,
}

struct Shared {
    queue: ArrayQueue<Queued>,
    /// `MidiKeyboardState::noteStates`: bit `c` of note `n` is set while
    /// note `n` is held on MIDI channel `c + 1`.
    note_states: [AtomicU16; NUM_NOTES],
    epoch: Instant,
}

/// A new keyboard: the editor's handle and the audio side.
pub fn keyboard() -> (KeyboardHandle, KeyboardConsumer) {
    let shared = Arc::new(Shared {
        queue: ArrayQueue::new(KEYBOARD_QUEUE_CAPACITY),
        note_states: std::array::from_fn(|_| AtomicU16::new(0)),
        epoch: Instant::now(),
    });
    let consumer = KeyboardConsumer {
        shared: shared.clone(),
        waiting: [Queued::default(); KEYBOARD_QUEUE_CAPACITY],
        num_waiting: 0,
        scheduled: [(0, [0; 3]); KEYBOARD_QUEUE_CAPACITY],
        num_scheduled: 0,
        next: 0,
    };
    (KeyboardHandle { shared }, consumer)
}

/// The editor's side: cheap to clone, `Send` and `Sync`.
#[derive(Clone)]
pub struct KeyboardHandle {
    shared: Arc<Shared>,
}

impl KeyboardHandle {
    /// `MidiKeyboardState::noteOn(1, note, velocity)`: `velocity` from 0 to
    /// 1 (`MidiKeyboard::getVelocityForNote`). Returns false (and changes
    /// nothing) for a note above 127 or when the queue is full.
    pub fn note_on(&self, note: u8, velocity: f32) -> bool {
        self.note_on_at(note, velocity, self.now())
    }

    /// `MidiKeyboardState::noteOff(1, note, 0)`: queued only while the note
    /// is held on channel 1, from the editor or the host. Returns whether it
    /// was queued.
    pub fn note_off(&self, note: u8) -> bool {
        self.note_off_at(note, self.now())
    }

    /// [`note_on`](Self::note_on) at `millis` on the keyboard's clock, as
    /// JUCE's millisecond counter stamps it (for tests).
    pub fn note_on_at(&self, note: u8, velocity: f32, millis: i64) -> bool {
        if usize::from(note) >= NUM_NOTES {
            return false;
        }
        // MidiMessage::noteOn(int, int, float): floatValueToMidiByte, where
        // roundToInt rounds half to even.
        let velocity = ((velocity * 127.0) as f64)
            .round_ties_even()
            .clamp(0.0, 127.0) as u8;
        if !self.push([0x90, note, velocity], millis) {
            return false;
        }
        self.shared.note_states[usize::from(note)].fetch_or(CHANNEL_BIT, Ordering::AcqRel);
        true
    }

    /// [`note_off`](Self::note_off) at `millis` (for tests).
    pub fn note_off_at(&self, note: u8, millis: i64) -> bool {
        let Some(state) = self.shared.note_states.get(usize::from(note)) else {
            return false;
        };
        if state.load(Ordering::Acquire) & CHANNEL_BIT == 0 || !self.push([0x80, note, 0], millis) {
            return false;
        }
        state.fetch_and(!CHANNEL_BIT, Ordering::AcqRel);
        true
    }

    /// Bit `n` is set while note `n` is held on any channel, from the editor
    /// (at once) or the host (as of the last block): what `MidiKeyboard`
    /// paints (`isNoteOnForChannels(0xffff, n)`).
    pub fn held_notes(&self) -> u128 {
        held_notes(&self.shared)
    }

    fn now(&self) -> i64 {
        self.shared.epoch.elapsed().as_millis() as i64
    }

    fn push(&self, message: [u8; 3], millis: i64) -> bool {
        self.shared.queue.push(Queued { message, millis }).is_ok()
    }
}

fn held_notes(shared: &Shared) -> u128 {
    shared
        .note_states
        .iter()
        .enumerate()
        .filter(|(_, state)| state.load(Ordering::Acquire) != 0)
        .fold(0, |mask, (note, _)| mask | 1 << note)
}

/// The audio side, owned by the block processor.
pub struct KeyboardConsumer {
    shared: Arc<Shared>,
    /// `MidiKeyboardState::eventsToAdd`, in time order.
    waiting: [Queued; KEYBOARD_QUEUE_CAPACITY],
    num_waiting: usize,
    /// This block's messages and their sample positions, in order.
    scheduled: [(usize, [u8; 3]); KEYBOARD_QUEUE_CAPACITY],
    num_scheduled: usize,
    next: usize,
}

impl KeyboardConsumer {
    /// Takes the editor's queued messages, as `MidiKeyboardState::noteOn`
    /// and `noteOff` add them to `eventsToAdd`: after any with the same
    /// time, dropping those more than 500 ms older. Call at the start of
    /// every block, bypassed or not.
    pub fn collect(&mut self) {
        while let Some(queued) = self.shared.queue.pop() {
            let at = self.waiting[..self.num_waiting]
                .iter()
                .rposition(|waiting| waiting.millis <= queued.millis)
                .map_or(0, |index| index + 1);
            if self.num_waiting < KEYBOARD_QUEUE_CAPACITY {
                self.waiting.copy_within(at..self.num_waiting, at + 1);
                self.waiting[at] = queued;
                self.num_waiting += 1;
            }
            // MidiBuffer::clear(0, timeNow - 500).
            let cutoff = queued.millis - MAX_AGE_MS;
            let stale = self.waiting[..self.num_waiting]
                .iter()
                .take_while(|waiting| waiting.millis < cutoff)
                .count();
            self.waiting.copy_within(stale..self.num_waiting, 0);
            self.num_waiting -= stale;
        }
    }

    /// `processNextMidiBuffer(buffer, 0, num_samples, true)`'s placement of
    /// the waiting messages: spread over the block in proportion to their
    /// times, the first at sample 0. Read them back with
    /// [`next_position`](Self::next_position) and [`pop`](Self::pop). Not
    /// called for an empty block or while bypassed, so they wait.
    pub fn schedule(&mut self, num_samples: usize) {
        self.num_scheduled = 0;
        self.next = 0;
        if num_samples == 0 || self.num_waiting == 0 {
            return;
        }
        let waiting = &self.waiting[..self.num_waiting];
        let first = waiting[0].millis;
        let last = waiting[waiting.len() - 1].millis;
        let scale = num_samples as f64 / (last + 1 - first) as f64;
        for (slot, queued) in self.scheduled.iter_mut().zip(waiting) {
            let position = ((queued.millis - first) as f64 * scale).round_ties_even();
            *slot = (
                position.clamp(0.0, (num_samples - 1) as f64) as usize,
                queued.message,
            );
        }
        self.num_scheduled = self.num_waiting;
        self.num_waiting = 0;
    }

    /// The sample position of this block's next keyboard message.
    pub fn next_position(&self) -> Option<usize> {
        (self.next < self.num_scheduled).then(|| self.scheduled[self.next].0)
    }

    /// This block's next keyboard message and its sample position. A host
    /// message at the same sample goes first.
    pub fn pop(&mut self) -> Option<(usize, [u8; 3])> {
        let scheduled = self.scheduled[..self.num_scheduled].get(self.next).copied();
        self.next += usize::from(scheduled.is_some());
        scheduled
    }

    /// `MidiKeyboardState::processNextMidiEvent` for a host message: note
    /// on, note off (or note on at velocity 0) and all notes off (CC 123)
    /// update the held notes.
    pub fn host_message(&mut self, message: [u8; 3]) {
        let [status, data1, data2] = message;
        let bit = 1u16 << (status & 0x0f);
        let note = usize::from(data1 & 0x7f);
        let states = &self.shared.note_states;
        match status & 0xf0 {
            0x90 if data2 != 0 => {
                states[note].fetch_or(bit, Ordering::AcqRel);
            }
            0x80 | 0x90 => {
                states[note].fetch_and(!bit, Ordering::AcqRel);
            }
            0xb0 if data1 == 123 => {
                for state in states {
                    state.fetch_and(!bit, Ordering::AcqRel);
                }
            }
            _ => {}
        }
    }

    /// See [`KeyboardHandle::held_notes`].
    pub fn held_notes(&self) -> u128 {
        held_notes(&self.shared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(consumer: &mut KeyboardConsumer) -> Vec<(usize, [u8; 3])> {
        std::iter::from_fn(|| consumer.pop()).collect()
    }

    #[test]
    fn messages_spread_over_the_block_by_time() {
        let (handle, mut consumer) = keyboard();
        assert!(handle.note_on_at(60, 1.0, 1000));
        assert!(handle.note_on_at(64, 0.5, 1005));
        assert!(handle.note_off_at(60, 1009));
        consumer.collect();
        consumer.schedule(100);
        // scale = 100 / (1009 + 1 - 1000) = 10.
        assert_eq!(
            drain(&mut consumer),
            vec![
                (0, [0x90, 60, 127]),
                (50, [0x90, 64, 64]),
                (90, [0x80, 60, 0])
            ]
        );
        consumer.schedule(100);
        assert_eq!(consumer.next_position(), None);
    }

    #[test]
    fn velocity_rounds_half_to_even() {
        let (handle, mut consumer) = keyboard();
        // 0.5 / 127 * 127 rounds to 0, 1.5 / 127 * 127 to 2.
        handle.note_on_at(1, 0.5 / 127.0, 0);
        handle.note_on_at(2, 1.5 / 127.0, 0);
        consumer.collect();
        consumer.schedule(4);
        let velocities: Vec<u8> = drain(&mut consumer).iter().map(|(_, m)| m[2]).collect();
        assert_eq!(velocities, vec![0, 2]);
    }

    #[test]
    fn note_off_needs_the_note_held_on_channel_one() {
        let (handle, mut consumer) = keyboard();
        assert!(!handle.note_off_at(60, 0));
        consumer.host_message([0x91, 60, 100]);
        assert!(!handle.note_off_at(60, 0), "held on channel 2 only");
        consumer.host_message([0x90, 60, 100]);
        assert!(
            handle.note_off_at(60, 0),
            "the host's channel 1 note counts"
        );
        assert_eq!(handle.held_notes(), 1 << 60, "still held on channel 2");
        assert!(!handle.note_on_at(128, 1.0, 0));
    }

    #[test]
    fn host_messages_update_the_held_notes() {
        let (handle, mut consumer) = keyboard();
        handle.note_on_at(10, 1.0, 0);
        consumer.host_message([0x93, 20, 1]);
        consumer.host_message([0x90, 30, 1]);
        assert_eq!(handle.held_notes(), 1 << 10 | 1 << 20 | 1 << 30);
        consumer.host_message([0x90, 30, 0]);
        assert_eq!(consumer.held_notes(), 1 << 10 | 1 << 20);
        consumer.host_message([0xb3, 123, 0]);
        assert_eq!(
            consumer.held_notes(),
            1 << 10,
            "all notes off clears channel 4 only"
        );
        consumer.host_message([0xb0, 123, 0]);
        assert_eq!(consumer.held_notes(), 0, "including the editor's note");
    }

    #[test]
    fn old_messages_are_dropped_and_the_rest_wait() {
        let (handle, mut consumer) = keyboard();
        handle.note_on_at(1, 1.0, 0);
        handle.note_on_at(2, 1.0, 400);
        consumer.collect();
        handle.note_on_at(3, 1.0, 900);
        consumer.collect();
        // Not scheduled (a bypassed block): they wait, and 0 is now too old.
        consumer.collect();
        consumer.schedule(10);
        let notes: Vec<u8> = drain(&mut consumer).iter().map(|(_, m)| m[1]).collect();
        assert_eq!(notes, vec![2, 3]);
    }

    #[test]
    fn a_full_queue_refuses() {
        let (handle, _consumer) = keyboard();
        for i in 0..KEYBOARD_QUEUE_CAPACITY {
            assert!(handle.note_on_at((i % 128) as u8, 1.0, 0));
        }
        assert!(!handle.note_on_at(5, 1.0, 0));
    }
}
