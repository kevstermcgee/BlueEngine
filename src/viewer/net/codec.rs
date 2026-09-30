//! A small binary codec for game protocols: little-endian fields, every read bounds-checked.
//!
//! The engine's own [`Packet`](super::Packet) is JSON, which cannot fit a whole game world in one
//! datagram. A game with its own protocol writes fixed layouts with [`Writer`] and reads them back with
//! [`Reader`]. Nothing here trusts the sender: a truncated, oversized, non-finite or otherwise nonsensical
//! datagram is a [`WireError`], never a panic, so a server can decode whatever the internet sends it.
//!
//! ```
//! use vesper3d::viewer::net::codec::{Reader, Writer};
//! let mut w = Writer::new();
//! w.u8(7);
//! w.f32(1.5);
//! w.text("hello", 16);
//! let bytes = w.finish();
//! let mut r = Reader::new(&bytes);
//! assert_eq!((r.u8().unwrap(), r.f32().unwrap(), r.text(16).unwrap().as_str()), (7, 1.5, "hello"));
//! r.done().unwrap();
//! assert!(Reader::new(&bytes[..3]).f32().is_err(), "truncated input is an error");
//! ```
use std::fmt;

/// Why a datagram was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireError(pub &'static str);

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad datagram: {}", self.0)
    }
}

impl std::error::Error for WireError {}

/// Shorthand for results of reading a datagram.
pub type WireResult<T> = Result<T, WireError>;

/// Builds one datagram.
#[derive(Default, Debug, Clone)]
pub struct Writer(Vec<u8>);

impl Writer {
    pub fn new() -> Self {
        Self(Vec::with_capacity(512))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn bool(&mut self, v: bool) {
        self.u8(v as u8);
    }
    /// A 128-bit token (session tokens, nonces).
    pub fn token(&mut self, t: [u64; 2]) {
        self.u64(t[0]);
        self.u64(t[1]);
    }
    /// Raw bytes with no length prefix (the reader must know the length).
    pub fn raw(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    /// A length-prefixed UTF-8 string of at most `max` bytes (cut on a character boundary; `max` <= 255).
    pub fn text(&mut self, s: &str, max: usize) {
        let mut n = s.len().min(max).min(255);
        while n > 0 && !s.is_char_boundary(n) {
            n -= 1;
        }
        self.u8(n as u8);
        self.0.extend_from_slice(&s.as_bytes()[..n]);
    }
    pub fn finish(self) -> Vec<u8> {
        self.0
    }
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// Reads one datagram.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    pub fn take(&mut self, n: usize) -> WireResult<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or(WireError("length overflow"))?;
        let slice = self.buf.get(self.pos..end).ok_or(WireError("truncated"))?;
        self.pos = end;
        Ok(slice)
    }
    pub fn u8(&mut self) -> WireResult<u8> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> WireResult<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> WireResult<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> WireResult<u64> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    /// A finite float; NaN and infinity are refused.
    pub fn f32(&mut self) -> WireResult<f32> {
        let v = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
        if v.is_finite() {
            Ok(v)
        } else {
            Err(WireError("non-finite number"))
        }
    }
    /// A float that must also lie in `-limit..=limit`.
    pub fn f32_within(&mut self, limit: f32) -> WireResult<f32> {
        let v = self.f32()?;
        if v.abs() <= limit {
            Ok(v)
        } else {
            Err(WireError("number out of range"))
        }
    }
    pub fn bool(&mut self) -> WireResult<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError("bad flag")),
        }
    }
    pub fn token(&mut self) -> WireResult<[u64; 2]> {
        Ok([self.u64()?, self.u64()?])
    }
    pub fn text(&mut self, max: usize) -> WireResult<String> {
        let n = self.u8()? as usize;
        if n > max {
            return Err(WireError("text too long"));
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| WireError("text is not utf-8"))
    }
    /// Everything not yet read.
    pub fn rest(&mut self) -> &'a [u8] {
        let rest = &self.buf[self.pos..];
        self.pos = self.buf.len();
        rest
    }
    /// Fail unless every byte has been read (trailing bytes mean a malformed or newer message).
    pub fn done(&self) -> WireResult<()> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(WireError("trailing bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_round_trips() {
        let mut w = Writer::new();
        w.u8(255);
        w.u16(65535);
        w.u32(0xdead_beef);
        w.u64(u64::MAX - 1);
        w.f32(-0.25);
        w.bool(true);
        w.token([1, 2]);
        w.text("héllo", 32);
        let bytes = w.finish();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8().unwrap(), 255);
        assert_eq!(r.u16().unwrap(), 65535);
        assert_eq!(r.u32().unwrap(), 0xdead_beef);
        assert_eq!(r.u64().unwrap(), u64::MAX - 1);
        assert_eq!(r.f32().unwrap(), -0.25);
        assert!(r.bool().unwrap());
        assert_eq!(r.token().unwrap(), [1, 2]);
        assert_eq!(r.text(32).unwrap(), "héllo");
        r.done().unwrap();
    }

    #[test]
    fn text_is_cut_on_a_character_boundary_and_capped() {
        let mut w = Writer::new();
        w.text("ééé", 3); // 3 bytes would split the second é
        let bytes = w.finish();
        assert_eq!(Reader::new(&bytes).text(3).unwrap(), "é");
        let mut w = Writer::new();
        w.text(&"x".repeat(400), 500);
        assert_eq!(w.len(), 1 + 255, "the length prefix is one byte");
    }

    #[test]
    fn hostile_input_is_an_error_never_a_panic() {
        assert!(Reader::new(&[]).u8().is_err());
        assert!(Reader::new(&[1, 2, 3]).u32().is_err());
        assert!(Reader::new(&f32::NAN.to_le_bytes()).f32().is_err());
        assert!(Reader::new(&f32::INFINITY.to_le_bytes()).f32().is_err());
        assert!(Reader::new(&1.0e9f32.to_le_bytes())
            .f32_within(100.)
            .is_err());
        assert!(Reader::new(&[2]).bool().is_err());
        assert!(
            Reader::new(&[200, 1]).text(255).is_err(),
            "declared length beyond the buffer"
        );
        assert!(
            Reader::new(&[3, b'a', b'b', b'c']).text(2).is_err(),
            "over the caller's cap"
        );
        assert!(Reader::new(&[2, 0xff, 0xfe]).text(8).is_err(), "not utf-8");
        assert!(Reader::new(&[0, 0]).done().is_err());
    }

    #[test]
    fn garbage_never_panics_any_reader_method() {
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..5_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let len = (x % 40) as usize;
            let bytes: Vec<u8> = (0..len).map(|i| (x >> (i % 56)) as u8).collect();
            let mut r = Reader::new(&bytes);
            let _ = (
                r.u8(),
                r.u16(),
                r.u32(),
                r.u64(),
                r.f32(),
                r.bool(),
                r.token(),
                r.text(16),
            );
            let _ = r.rest();
        }
    }
}
