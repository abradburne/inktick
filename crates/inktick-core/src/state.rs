//! Save states: each part has one `visit` function that both saves and loads, so the two can
//! never disagree.

use alloc::vec::Vec;

/// Something that walks a part's fields, saving or loading them.
pub trait Visitor {
    /// A byte.
    fn u8(&mut self, v: &mut u8);
    /// A little-endian u16.
    fn u16(&mut self, v: &mut u16);
    /// A little-endian u32.
    fn u32(&mut self, v: &mut u32);
    /// A little-endian u64.
    fn u64(&mut self, v: &mut u64);
    /// A bool, as a byte.
    fn bool(&mut self, v: &mut bool);
    /// A fixed-length run of bytes.
    fn bytes(&mut self, v: &mut [u8]);
    /// An f32, by its bits.
    fn f32(&mut self, v: &mut f32) {
        let mut bits = v.to_bits();
        self.u32(&mut bits);
        *v = f32::from_bits(bits);
    }
}

/// A part that can be saved and loaded.
pub trait Visit {
    /// Visits every field that makes up the part's state.
    fn visit(&mut self, v: &mut impl Visitor);
}

/// Saves into a byte vector.
#[derive(Default)]
pub struct Saver(pub Vec<u8>);

impl Visitor for Saver {
    fn u8(&mut self, v: &mut u8) {
        self.0.push(*v);
    }
    fn u16(&mut self, v: &mut u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: &mut u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: &mut u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bool(&mut self, v: &mut bool) {
        self.0.push(*v as u8);
    }
    fn bytes(&mut self, v: &mut [u8]) {
        self.0.extend_from_slice(v);
    }
}

/// Loads from bytes; `ok` turns false if they run out.
pub struct Loader<'a> {
    data: &'a [u8],
    /// Whether every field was there.
    pub ok: bool,
}

impl<'a> Loader<'a> {
    /// A loader over `data`.
    pub fn new(data: &'a [u8]) -> Loader<'a> {
        Loader { data, ok: true }
    }

    /// Whether every byte was used.
    pub fn finished(&self) -> bool {
        self.ok && self.data.is_empty()
    }

    fn take<const N: usize>(&mut self) -> [u8; N] {
        if self.data.len() < N {
            self.ok = false;
            self.data = &[];
            return [0; N];
        }
        let (head, rest) = self.data.split_at(N);
        self.data = rest;
        head.try_into().unwrap_or([0; N])
    }
}

impl Visitor for Loader<'_> {
    fn u8(&mut self, v: &mut u8) {
        *v = self.take::<1>()[0];
    }
    fn u16(&mut self, v: &mut u16) {
        *v = u16::from_le_bytes(self.take());
    }
    fn u32(&mut self, v: &mut u32) {
        *v = u32::from_le_bytes(self.take());
    }
    fn u64(&mut self, v: &mut u64) {
        *v = u64::from_le_bytes(self.take());
    }
    fn bool(&mut self, v: &mut bool) {
        *v = self.take::<1>()[0] != 0;
    }
    fn bytes(&mut self, v: &mut [u8]) {
        if self.data.len() < v.len() {
            self.ok = false;
            self.data = &[];
            return;
        }
        let (head, rest) = self.data.split_at(v.len());
        v.copy_from_slice(head);
        self.data = rest;
    }
}
