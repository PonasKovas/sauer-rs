use std::collections::BTreeMap;

use bytes::Bytes;

use crate::protocol::{seq_lt, FREE_RELIABLE_WINDOWS, RELIABLE_WINDOW_SIZE, UNSEQUENCED_WINDOW_SIZE};

/// Maximum number of out-of-order reliable packets buffered per channel.
/// Caps per-channel memory use and prevents a malicious peer from exhausting
/// host memory by sending a flood of out-of-order packets.
const MAX_INCOMING_RELIABLE_QUEUE: usize = 4096;

// ── Fragment reassembly ───────────────────────────────────────────────────────

#[derive(Debug)]
pub struct FragmentBuffer {
    pub(crate) fragment_count: u32,
    pub(crate) received: u32,
    pub(crate) buf: Vec<u8>,
    /// Bitmask: bit i set = fragment i arrived.
    pub(crate) mask: Vec<u64>,
}

impl FragmentBuffer {
    pub fn new(total_length: usize, fragment_count: u32) -> Self {
        let mask_len = (fragment_count as usize).div_ceil(64);
        Self {
            fragment_count,
            received: 0,
            buf: vec![0u8; total_length],
            mask: vec![0u64; mask_len],
        }
    }

    /// Returns true if this was the last fragment needed.
    ///
    /// Returns false (without updating state) if the fragment is a duplicate,
    /// out-of-range, or its data extends beyond the declared `total_length`.
    pub fn receive(&mut self, fragment_number: u32, offset: usize, data: &[u8]) -> bool {
        let word = fragment_number as usize / 64;
        let bit = fragment_number as usize % 64;
        if word >= self.mask.len() {
            return false;
        }
        if self.mask[word] & (1 << bit) != 0 {
            // duplicate
            return false;
        }

        // Validate write bounds before accepting the fragment. If we counted the
        // fragment as received but skipped writing its bytes, we would later deliver
        // a packet with zeroed-out regions — silent data corruption.
        let end = match offset.checked_add(data.len()) {
            Some(e) => e,
            None => return false, // offset + len overflows usize
        };
        if end > self.buf.len() {
            // Malformed: data extends past the declared total_length. Reject without
            // updating accounting so the fragment can be received again correctly.
            return false;
        }

        self.mask[word] |= 1 << bit;
        self.received += 1;
        self.buf[offset..end].copy_from_slice(data);

        self.received == self.fragment_count
    }

    pub fn finish(self) -> Bytes {
        Bytes::from(self.buf)
    }
}

// ── Channel ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Channel {
    // Outgoing
    pub(crate) outgoing_reliable_seq: u16,
    pub(crate) outgoing_unreliable_seq: u16,
    pub(crate) outgoing_unsequenced_group: u16,

    // Incoming reliable
    pub(crate) incoming_reliable_seq: u16,
    /// Reliable commands held waiting for an earlier seq to arrive.
    pub(crate) incoming_reliable_queue: BTreeMap<u16, (Bytes, u32)>,

    // Incoming unreliable
    pub(crate) incoming_unreliable_seq: u16,

    // Reliable fragment reassembly: start_seq -> buffer
    pub(crate) fragment_buffers: BTreeMap<u16, FragmentBuffer>,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            outgoing_reliable_seq: 0,
            outgoing_unreliable_seq: 0,
            outgoing_unsequenced_group: 0,
            incoming_reliable_seq: 0,
            incoming_reliable_queue: BTreeMap::new(),
            incoming_unreliable_seq: 0,
            fragment_buffers: BTreeMap::new(),
        }
    }
}

impl Channel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance and return the next outgoing reliable sequence number.
    pub fn next_reliable_seq(&mut self) -> u16 {
        self.outgoing_reliable_seq = self.outgoing_reliable_seq.wrapping_add(1);
        self.outgoing_unreliable_seq = 0;
        self.outgoing_reliable_seq
    }

    pub fn next_unreliable_seq(&mut self) -> u16 {
        self.outgoing_unreliable_seq = self.outgoing_unreliable_seq.wrapping_add(1);
        self.outgoing_unreliable_seq
    }

    pub fn next_unsequenced_group(&mut self) -> u16 {
        self.outgoing_unsequenced_group = self.outgoing_unsequenced_group.wrapping_add(1);
        self.outgoing_unsequenced_group
    }

    /// Receive a reliable in-order delivery. Returns in-order messages that are now deliverable.
    pub fn receive_reliable(&mut self, seq: u16, data: Bytes, frag_count: u32) -> Vec<Bytes> {
        let expected = self.incoming_reliable_seq.wrapping_add(1);

        if seq == expected {
            // Advance by frag_count and reset unreliable sequence
            self.incoming_reliable_seq = seq.wrapping_add((frag_count.max(1) - 1) as u16);
            self.incoming_unreliable_seq = 0;
            let mut out = vec![data];
            // Drain any buffered follow-on sequences
            loop {
                let next = self.incoming_reliable_seq.wrapping_add(1);
                if let Some(d) = self.incoming_reliable_queue.remove(&next) {
                    // Advance by buffered frag_count (d.1) and reset unreliable sequence
                    self.incoming_reliable_seq = next.wrapping_add((d.1.max(1) - 1) as u16);
                    self.incoming_unreliable_seq = 0;
                    out.push(d.0);
                } else {
                    break;
                }
            }
            out
        } else if seq_lt(expected, seq) {
            // Buffer only if within the receive window and below the queue size cap.
            let window: u16 = FREE_RELIABLE_WINDOWS * RELIABLE_WINDOW_SIZE;
            if seq.wrapping_sub(expected) <= window
                && self.incoming_reliable_queue.len() < MAX_INCOMING_RELIABLE_QUEUE
            {
                self.incoming_reliable_queue.insert(seq, (data, frag_count));
            }
            vec![]
        } else {
            // Old duplicate — discard
            vec![]
        }
    }

    /// Receive an unreliable sequenced message. Returns Some(data) if accepted.
    pub fn receive_unreliable(&mut self, seq: u16, data: Bytes) -> Option<Bytes> {
        // Accept if seq comes strictly after current incoming seq
        if seq_lt(self.incoming_unreliable_seq, seq) {
            self.incoming_unreliable_seq = seq;
            Some(data)
        } else {
            None // late/duplicate, discard
        }
    }
}

// ── Unsequenced duplicate detection ──────────────────────────────────────────

pub struct UnsequencedWindow {
    pub(crate) window: [u32; 32],
    pub(crate) group: u16,
}

impl Default for UnsequencedWindow {
    fn default() -> Self {
        Self {
            window: [0u32; 32],
            group: 0,
        }
    }
}

impl UnsequencedWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the packet with this group is new (not a duplicate).
    ///
    /// `self.group` tracks the highest received group (high-water mark).
    /// The window covers the 1024 most recent group values.
    pub fn check_and_set(&mut self, group: u16) -> bool {
        let diff = group.wrapping_sub(self.group) as i16;

        if diff > 0 {
            // This group is ahead of the current high-water mark — advance.
            let advance = diff as u16;
            if advance >= UNSEQUENCED_WINDOW_SIZE as u16 {
                // More than the full window ahead: reset everything.
                self.window = [0u32; 32];
            } else {
                // Slide forward: clear the bits for positions being recycled.
                for i in 1..=advance {
                    let pos = self.group.wrapping_add(i);
                    let idx = pos as usize % UNSEQUENCED_WINDOW_SIZE as usize;
                    self.window[idx / 32] &= !(1u32 << (idx % 32));
                }
            }
            self.group = group;
        } else if diff < 0 {
            // This group is behind the high-water mark.
            let behind = (-(diff as i32)) as u16;
            if behind >= UNSEQUENCED_WINDOW_SIZE as u16 {
                return false; // Too old — outside the dedup window.
            }
        }
        // diff == 0: same as current high-water mark; bit should already be set.

        let idx = group as usize % UNSEQUENCED_WINDOW_SIZE as usize;
        let word = idx / 32;
        let bit = 1u32 << (idx % 32);
        if self.window[word] & bit != 0 {
            return false; // Duplicate.
        }
        self.window[word] |= bit;
        true
    }
}

