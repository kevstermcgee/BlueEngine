//! Deterministic, title-seeded application icons for BlueEngine games.
//!
//! Every game made with the engine ships an icon on four surfaces: the executable's resource, the
//! window and taskbar icon, `dist/<game>.ico` and the desktop shortcut. This module makes one from
//! nothing but the game title, so even a game whose author draws no art gets a tile that is unique
//! and decent looking.
//!
//! * [`IconSpec`] names the artwork: a title and a `variant` number. The same spec always renders
//!   the same pixels; another variant is a substantially different design (other hue family, tile
//!   composition, pattern and accent), meant for resolving a clash between two games.
//! * [`render`] draws one size directly (super-sampled, with a level of detail that suits the size),
//!   [`ico`] packs every size in [`ICO_SIZES`] into a multi-size `.ico`, [`window_icon_blobs`] gives
//!   the raw blobs a miniquad window wants and [`write_icon_set`] writes the files a game project
//!   keeps.
//! * [`png()`] is a small PNG encoder with a real DEFLATE (LZ77 with hash chains, fixed and dynamic
//!   Huffman codes); [`decode_png`] and [`parse_ico`] read the results back so tests and tooling can
//!   prove the files are valid.
//! * [`signature`] and [`hash_distance`] give a 64-bit perceptual hash, to spot two icons that look
//!   alike.
//!
//! The module uses only `std`, no `unsafe`, no clock and no randomness other than its own seeded
//! generator. Its floating point sticks to operations that are exact under IEEE 754 (`+ - * /`,
//! `sqrt`, `floor`, `round`) plus a few polynomial functions defined here, so the pixels do not
//! depend on the platform's libm.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, LN_2, LOG2_E, PI, SQRT_2, TAU};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Icon sizes written to the .ico, ascending.
pub const ICO_SIZES: [u32; 10] = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];

/// Largest edge length [`render`] produces; bigger requests are clamped to this.
pub const MAX_RENDER_SIZE: u32 = 2048;

/// Identity of the artwork. Same spec, same pixels (on the same platform).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconSpec {
    /// Game title the artwork is derived from (trimmed and lower-cased before hashing).
    pub title: String,
    /// Design number: 0 is the default design, any other number picks a substantially different one.
    pub variant: u32,
}

impl IconSpec {
    /// Spec for `title`, variant 0.
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            variant: 0,
        }
    }

    /// The same title with another design number.
    pub fn with_variant(mut self, variant: u32) -> Self {
        self.variant = variant;
        self
    }
}

// ---------------------------------------------------------------------------------------------
// Deterministic maths. Only exact IEEE operations and polynomials, never the platform libm.
// ---------------------------------------------------------------------------------------------

type Rgb = [f32; 3];

/// Clamp to `0..=1`; NaN becomes 0.
fn sat(x: f32) -> f32 {
    if x > 0.0 {
        if x < 1.0 {
            x
        } else {
            1.0
        }
    } else {
        0.0
    }
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix3(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = sat((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

/// Sine and cosine of `x` radians (polynomial; about 1e-6 accurate for moderate `x`).
fn sin_cos(x: f32) -> (f32, f32) {
    if !x.is_finite() {
        return (0.0, 1.0);
    }
    let x = if x.abs() > 1000.0 { x % TAU } else { x };
    let k = (x * (2.0 / PI)).round();
    let r = x - k * FRAC_PI_2;
    let r2 = r * r;
    let s = r
        * (1.0
            + r2 * (-1.0 / 6.0
                + r2 * (1.0 / 120.0 + r2 * (-1.0 / 5040.0 + r2 * (1.0 / 362_880.0)))));
    let c = 1.0 + r2 * (-0.5 + r2 * (1.0 / 24.0 + r2 * (-1.0 / 720.0 + r2 * (1.0 / 40_320.0))));
    match (k as i32).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    }
}

/// `atan2(y, x)` in radians (polynomial, about 1e-5 accurate).
fn atan2(y: f32, x: f32) -> f32 {
    let ax = x.abs();
    let ay = y.abs();
    let hi = ax.max(ay);
    if hi <= 0.0 || hi.is_nan() {
        return 0.0;
    }
    let a = ax.min(ay) / hi;
    let s = a * a;
    let mut r =
        a * (0.999_866 + s * (-0.330_299_5 + s * (0.180_141 + s * (-0.085_133 + s * 0.020_835_1))));
    if ay > ax {
        r = FRAC_PI_2 - r;
    }
    if x < 0.0 {
        r = PI - r;
    }
    if y < 0.0 {
        r = -r;
    }
    r
}

fn ln_f(x: f32) -> f32 {
    if x.is_nan() || x <= 1e-30 {
        return -69.0;
    }
    let bits = x.to_bits();
    let mut e = ((bits >> 23) & 0xff) as i32 - 127;
    let mut m = f32::from_bits((bits & 0x007f_ffff) | 0x3f80_0000);
    if m > SQRT_2 {
        m *= 0.5;
        e += 1;
    }
    let z = (m - 1.0) / (m + 1.0);
    let z2 = z * z;
    let series = 1.0 + z2 * (1.0 / 3.0 + z2 * (1.0 / 5.0 + z2 * (1.0 / 7.0 + z2 * (1.0 / 9.0))));
    e as f32 * LN_2 + 2.0 * z * series
}

fn exp_f(x: f32) -> f32 {
    if x.is_nan() {
        return 1.0;
    }
    let x = x.clamp(-80.0, 80.0);
    let k = (x * LOG2_E).round();
    let r = x - k * LN_2;
    let p = 1.0
        + r * (1.0
            + r * (0.5
                + r * (1.0 / 6.0
                    + r * (1.0 / 24.0
                        + r * (1.0 / 120.0 + r * (1.0 / 720.0 + r * (1.0 / 5040.0)))))));
    p * f32::from_bits(((k as i32 + 127) as u32) << 23)
}

fn powf(x: f32, y: f32) -> f32 {
    if x <= 0.0 {
        0.0
    } else {
        exp_f(y * ln_f(x))
    }
}

// ---------------------------------------------------------------------------------------------
// Hashing and the seeded generator.
// ---------------------------------------------------------------------------------------------

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// SplitMix64 finaliser.
fn mix64(x: u64) -> u64 {
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Tiny SplitMix64 generator.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix64(self.0)
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16_777_216.0
    }

    /// Uniform in `[lo, hi)`.
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform integer in `0..n` (`n` must be below 2^32; 0 gives 0).
    fn below(&mut self, n: usize) -> usize {
        (((self.next_u64() >> 32) * n as u64) >> 32) as usize
    }

    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    fn sign(&mut self) -> f32 {
        if self.next_u64() & 1 == 0 {
            1.0
        } else {
            -1.0
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Checksums and bit-level helpers shared by the PNG encoder and decoder.
// ---------------------------------------------------------------------------------------------

const fn make_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xedb8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = make_crc_table();

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for part in parts {
        for &b in *part {
            crc = CRC_TABLE[((crc ^ u32::from(b)) & 0xff) as usize] ^ (crc >> 8);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65_521;
        b %= 65_521;
    }
    (b << 16) | a
}

/// LSB-first bit writer, as DEFLATE wants.
struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            acc: 0,
            n: 0,
        }
    }

    /// Appends the low `bits` (at most 32) bits of `value`, least significant first.
    fn put(&mut self, value: u32, bits: u32) {
        let mask = (1u64 << bits) - 1;
        self.acc |= (u64::from(value) & mask) << self.n;
        self.n += bits;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

// ---------------------------------------------------------------------------------------------
// DEFLATE encoder: LZ77 with hash chains and lazy matching, fixed or dynamic Huffman per block.
// ---------------------------------------------------------------------------------------------

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12_289, 16_385, 24_577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// Order in which the code-length code lengths are stored in a dynamic block header.
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

const WINDOW: usize = 32_768;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const HASH_BITS: u32 = 15;
const MAX_CHAIN: usize = 1024;
const NICE_MATCH: usize = 258;
const LAZY_MATCH: usize = 258;
const BLOCK_TOKENS: usize = 12_288;
const NIL: u32 = u32::MAX;

/// One LZ77 token: a literal byte (`len == 0`, byte in `val`) or a match (`len`, distance in `val`).
#[derive(Clone, Copy)]
struct Token {
    len: u16,
    val: u16,
}

struct Matcher<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
}

impl<'a> Matcher<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            head: vec![NIL; 1 << HASH_BITS],
            prev: vec![NIL; data.len()],
        }
    }

    fn hash(&self, i: usize) -> usize {
        let d = self.data;
        let v = (u32::from(d[i]) << 16) | (u32::from(d[i + 1]) << 8) | u32::from(d[i + 2]);
        (v.wrapping_mul(0x9e37_79b1) >> (32 - HASH_BITS)) as usize
    }

    fn insert(&mut self, i: usize) {
        if i + MIN_MATCH <= self.data.len() {
            let h = self.hash(i);
            self.prev[i] = self.head[h];
            self.head[h] = i as u32;
        }
    }

    /// Longest earlier match for the bytes at `i` as `(length, distance)`; `(0, 0)` when none.
    fn longest(&self, i: usize) -> (usize, usize) {
        let n = self.data.len();
        if i + MIN_MATCH > n {
            return (0, 0);
        }
        let max_len = (n - i).min(MAX_MATCH);
        let mut best_len = MIN_MATCH - 1;
        let mut best_dist = 0;
        let mut cand = self.head[self.hash(i)];
        let mut chain = 0;
        while cand != NIL && chain < MAX_CHAIN {
            let c = cand as usize;
            let dist = i - c;
            if dist > WINDOW {
                break;
            }
            if best_len >= max_len || self.data[c + best_len] == self.data[i + best_len] {
                let mut l = 0;
                while l < max_len && self.data[c + l] == self.data[i + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_dist = dist;
                    if l >= max_len || l >= NICE_MATCH {
                        break;
                    }
                }
            }
            cand = self.prev[c];
            chain += 1;
        }
        if best_dist == 0 {
            (0, 0)
        } else {
            (best_len, best_dist)
        }
    }
}

fn lz77(data: &[u8]) -> Vec<Token> {
    let n = data.len();
    let mut m = Matcher::new(data);
    let mut tokens = Vec::with_capacity(n / 4 + 16);
    let mut carry: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < n {
        let (mut len, dist) = match carry.take() {
            Some(found) => found,
            None => m.longest(i),
        };
        if len == MIN_MATCH && dist > 4096 {
            len = 0;
        }
        m.insert(i);
        if len >= MIN_MATCH {
            if len < LAZY_MATCH && i + 1 < n {
                let next = m.longest(i + 1);
                if next.0 > len + 1 {
                    tokens.push(Token {
                        len: 0,
                        val: u16::from(data[i]),
                    });
                    i += 1;
                    carry = Some(next);
                    continue;
                }
            }
            tokens.push(Token {
                len: len as u16,
                val: dist as u16,
            });
            for j in i + 1..i + len {
                m.insert(j);
            }
            i += len;
        } else {
            tokens.push(Token {
                len: 0,
                val: u16::from(data[i]),
            });
            i += 1;
        }
    }
    tokens
}

/// Length symbol (257..=285), extra-bit value and extra-bit count for a match length of 3..=258.
fn length_symbol(len: usize) -> (usize, u32, u32) {
    let mut idx = 28;
    while idx > 0 && usize::from(LEN_BASE[idx]) > len {
        idx -= 1;
    }
    (
        257 + idx,
        (len - usize::from(LEN_BASE[idx])) as u32,
        u32::from(LEN_EXTRA[idx]),
    )
}

/// Distance symbol (0..=29), extra-bit value and extra-bit count for a distance of 1..=32768.
fn dist_symbol(dist: usize) -> (usize, u32, u32) {
    let mut idx = 29;
    while idx > 0 && usize::from(DIST_BASE[idx]) > dist {
        idx -= 1;
    }
    (
        idx,
        (dist - usize::from(DIST_BASE[idx])) as u32,
        u32::from(DIST_EXTRA[idx]),
    )
}

/// Huffman code lengths (at most `max_bits`) for the given symbol frequencies. The result is always
/// a complete prefix code: with fewer than two used symbols, unused ones are added as dummies.
fn code_lengths(freq: &[u32], max_bits: u8) -> Vec<u8> {
    let mut f: Vec<u64> = freq.iter().map(|&x| u64::from(x)).collect();
    let used = f.iter().filter(|&&x| x > 0).count();
    let mut need = 2usize.saturating_sub(used);
    for x in f.iter_mut() {
        if need == 0 {
            break;
        }
        if *x == 0 {
            *x = 1;
            need -= 1;
        }
    }
    let mut lens = vec![0u8; f.len()];
    loop {
        let leaves: Vec<usize> = (0..f.len()).filter(|&i| f[i] > 0).collect();
        if leaves.len() < 2 {
            return lens;
        }
        let mut weight: Vec<u64> = leaves.iter().map(|&i| f[i]).collect();
        let mut parent: Vec<usize> = vec![usize::MAX; weight.len()];
        let mut heap: BinaryHeap<Reverse<(u64, usize)>> = weight
            .iter()
            .enumerate()
            .map(|(i, &w)| Reverse((w, i)))
            .collect();
        while heap.len() > 1 {
            let (Some(Reverse(x)), Some(Reverse(y))) = (heap.pop(), heap.pop()) else {
                break;
            };
            let id = weight.len();
            weight.push(x.0 + y.0);
            parent.push(usize::MAX);
            parent[x.1] = id;
            parent[y.1] = id;
            heap.push(Reverse((x.0 + y.0, id)));
        }
        let mut depth = vec![0u32; weight.len()];
        for idx in (0..weight.len() - 1).rev() {
            depth[idx] = depth[parent[idx]] + 1;
        }
        let deepest = depth[..leaves.len()].iter().copied().max().unwrap_or(0);
        if deepest <= u32::from(max_bits) {
            for (k, &sym) in leaves.iter().enumerate() {
                lens[sym] = depth[k] as u8;
            }
            return lens;
        }
        for x in f.iter_mut() {
            if *x > 1 {
                *x = (*x).div_ceil(2);
            }
        }
    }
}

/// Canonical Huffman codes for `lens`, already bit-reversed for an LSB-first writer.
fn canonical_codes(lens: &[u8]) -> Vec<u16> {
    let mut count = [0u32; 16];
    for &l in lens {
        count[usize::from(l)] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 16];
    let mut code = 0u32;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                0
            } else {
                let c = next[usize::from(l)];
                next[usize::from(l)] += 1;
                (c.reverse_bits() >> (32 - u32::from(l))) as u16
            }
        })
        .collect()
}

struct Codes {
    code: Vec<u16>,
    len: Vec<u8>,
}

impl Codes {
    fn new(len: Vec<u8>) -> Self {
        Self {
            code: canonical_codes(&len),
            len,
        }
    }

    fn put(&self, bw: &mut BitWriter, sym: usize) {
        bw.put(u32::from(self.code[sym]), u32::from(self.len[sym]));
    }
}

fn fixed_lit_len(sym: usize) -> u8 {
    match sym {
        0..=143 => 8,
        144..=255 => 9,
        256..=279 => 7,
        _ => 8,
    }
}

/// Run-length codes for a sequence of code lengths: `(symbol 0..=18, extra bits value)`.
fn rle_lengths(lens: &[u8]) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lens.len() {
        let v = lens[i];
        let mut run = 1;
        while i + run < lens.len() && lens[i + run] == v {
            run += 1;
        }
        i += run;
        if v == 0 {
            let mut r = run;
            while r >= 11 {
                let n = r.min(138);
                out.push((18, (n - 11) as u8));
                r -= n;
            }
            if r >= 3 {
                out.push((17, (r - 3) as u8));
                r = 0;
            }
            for _ in 0..r {
                out.push((0, 0));
            }
        } else {
            out.push((v, 0));
            let mut r = run - 1;
            while r >= 3 {
                let n = r.min(6);
                out.push((16, (n - 3) as u8));
                r -= n;
            }
            for _ in 0..r {
                out.push((v, 0));
            }
        }
    }
    out
}

fn emit_tokens(bw: &mut BitWriter, tokens: &[Token], lit: &Codes, dist: &Codes) {
    for t in tokens {
        if t.len == 0 {
            lit.put(bw, usize::from(t.val));
        } else {
            let (ls, lx, lb) = length_symbol(usize::from(t.len));
            lit.put(bw, ls);
            bw.put(lx, lb);
            let (ds, dx, db) = dist_symbol(usize::from(t.val));
            dist.put(bw, ds);
            bw.put(dx, db);
        }
    }
    lit.put(bw, 256);
}

/// Writes one DEFLATE block, choosing whichever of fixed and dynamic Huffman codes is smaller.
fn write_block(bw: &mut BitWriter, tokens: &[Token], last: bool) {
    let mut lit_freq = [0u32; 286];
    let mut dist_freq = [0u32; 30];
    for t in tokens {
        if t.len == 0 {
            lit_freq[usize::from(t.val)] += 1;
        } else {
            lit_freq[length_symbol(usize::from(t.len)).0] += 1;
            dist_freq[dist_symbol(usize::from(t.val)).0] += 1;
        }
    }
    lit_freq[256] += 1;

    let fixed_bits: u64 = 3
        + lit_freq
            .iter()
            .enumerate()
            .map(|(s, &f)| u64::from(f) * u64::from(fixed_lit_len(s)))
            .sum::<u64>()
        + dist_freq.iter().map(|&f| u64::from(f) * 5).sum::<u64>();

    let lit_lens = code_lengths(&lit_freq, 15);
    let dist_lens = code_lengths(&dist_freq, 15);
    let hlit = lit_lens
        .iter()
        .rposition(|&l| l > 0)
        .map_or(257, |p| (p + 1).max(257));
    let hdist = dist_lens.iter().rposition(|&l| l > 0).map_or(1, |p| p + 1);
    let mut seq = lit_lens[..hlit].to_vec();
    seq.extend_from_slice(&dist_lens[..hdist]);
    let rle = rle_lengths(&seq);
    let mut cl_freq = [0u32; 19];
    for &(sym, _) in &rle {
        cl_freq[usize::from(sym)] += 1;
    }
    let cl_lens = code_lengths(&cl_freq, 7);
    let hclen = CL_ORDER
        .iter()
        .rposition(|&s| cl_lens[s] > 0)
        .map_or(4, |p| (p + 1).max(4));
    let header_bits: u64 = 3
        + 14
        + 3 * hclen as u64
        + rle
            .iter()
            .map(|&(sym, _)| {
                u64::from(cl_lens[usize::from(sym)])
                    + match sym {
                        16 => 2,
                        17 => 3,
                        18 => 7,
                        _ => 0,
                    }
            })
            .sum::<u64>();
    let body_bits: u64 = lit_freq
        .iter()
        .zip(&lit_lens)
        .map(|(&f, &l)| u64::from(f) * u64::from(l))
        .sum::<u64>()
        + dist_freq
            .iter()
            .zip(&dist_lens)
            .map(|(&f, &l)| u64::from(f) * u64::from(l))
            .sum::<u64>();

    bw.put(u32::from(last), 1);
    if header_bits + body_bits < fixed_bits {
        bw.put(2, 2);
        bw.put((hlit - 257) as u32, 5);
        bw.put((hdist - 1) as u32, 5);
        bw.put((hclen - 4) as u32, 4);
        for &s in &CL_ORDER[..hclen] {
            bw.put(u32::from(cl_lens[s]), 3);
        }
        let cl = Codes::new(cl_lens);
        for &(sym, extra) in &rle {
            cl.put(bw, usize::from(sym));
            match sym {
                16 => bw.put(u32::from(extra), 2),
                17 => bw.put(u32::from(extra), 3),
                18 => bw.put(u32::from(extra), 7),
                _ => {}
            }
        }
        emit_tokens(bw, tokens, &Codes::new(lit_lens), &Codes::new(dist_lens));
    } else {
        bw.put(1, 2);
        let lit = Codes::new((0..288).map(fixed_lit_len).collect());
        let dist = Codes::new(vec![5; 30]);
        emit_tokens(bw, tokens, &lit, &dist);
    }
}

/// Raw DEFLATE stream (RFC 1951) for `data`.
fn deflate(data: &[u8]) -> Vec<u8> {
    let tokens = lz77(data);
    let mut bw = BitWriter::new();
    if tokens.is_empty() {
        write_block(&mut bw, &[], true);
    }
    let blocks = tokens.len().div_ceil(BLOCK_TOKENS);
    for (i, chunk) in tokens.chunks(BLOCK_TOKENS).enumerate() {
        write_block(&mut bw, chunk, i + 1 == blocks);
    }
    bw.finish()
}

/// zlib stream (RFC 1950): header, DEFLATE data, Adler-32.
fn zlib_compress(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9c];
    out.extend_from_slice(&deflate(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

// ---------------------------------------------------------------------------------------------
// PNG encoder.
// ---------------------------------------------------------------------------------------------

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const MAX_PNG_BYTES: usize = 256 << 20;

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i16::from(a), i16::from(b), i16::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Filters one RGBA row with filter type `kind` (0..=4) into `out`.
fn filter_row(kind: u8, cur: &[u8], prev: &[u8], out: &mut [u8]) {
    for (i, o) in out.iter_mut().enumerate() {
        let x = cur[i];
        let a = if i >= 4 { cur[i - 4] } else { 0 };
        let b = prev[i];
        let c = if i >= 4 { prev[i - 4] } else { 0 };
        *o = match kind {
            0 => x,
            1 => x.wrapping_sub(a),
            2 => x.wrapping_sub(b),
            3 => x.wrapping_sub(((u16::from(a) + u16::from(b)) / 2) as u8),
            _ => x.wrapping_sub(paeth(a, b, c)),
        };
    }
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// Minimal PNG encoder: RGBA8, non-interlaced, per-row adaptive filter (filters 0-4, keeping the
/// smallest sum of absolute values), real DEFLATE compression (LZ77 with a hash chain and lazy
/// matching, fixed or dynamic Huffman codes per block), zlib header and Adler-32, CRC-32 per chunk.
/// Any standard decoder reads the result.
///
/// `rgba` should hold `width * height * 4` bytes; missing bytes count as transparent black and extra
/// bytes are ignored. A zero dimension, or an image over 256 MiB, gives an empty `Vec`.
pub fn png(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = (width as usize, height as usize);
    let total = match w.checked_mul(4).and_then(|row| row.checked_mul(h)) {
        Some(t)
            if w > 0 && h > 0 && t <= MAX_PNG_BYTES && width < (1 << 31) && height < (1 << 31) =>
        {
            t
        }
        _ => return Vec::new(),
    };
    let row = w * 4;
    let padded;
    let data: &[u8] = if rgba.len() >= total {
        &rgba[..total]
    } else {
        let mut v = rgba.to_vec();
        v.resize(total, 0);
        padded = v;
        &padded
    };

    let zero = vec![0u8; row];
    let mut cands: Vec<Vec<u8>> = vec![vec![0u8; row]; 5];
    let mut raw = Vec::with_capacity((row + 1) * h);
    for y in 0..h {
        let cur = &data[y * row..(y + 1) * row];
        let prev = if y == 0 {
            &zero[..]
        } else {
            &data[(y - 1) * row..y * row]
        };
        let mut best = 0u8;
        let mut best_cost = u64::MAX;
        for (kind, cand) in cands.iter_mut().enumerate() {
            filter_row(kind as u8, cur, prev, cand);
            let cost: u64 = cand
                .iter()
                .map(|&b| u64::from((b as i8).unsigned_abs()))
                .sum();
            if cost < best_cost {
                best_cost = cost;
                best = kind as u8;
            }
        }
        raw.push(best);
        raw.extend_from_slice(&cands[usize::from(best)]);
    }

    let mut out = Vec::new();
    out.extend_from_slice(&PNG_SIGNATURE);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &zlib_compress(&raw));
    png_chunk(&mut out, b"IEND", &[]);
    out
}

// ---------------------------------------------------------------------------------------------
// Inflate and PNG decoder (used to prove the encoder round-trips, and by tooling).
// ---------------------------------------------------------------------------------------------

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit: 0,
        }
    }

    fn bit(&mut self) -> Result<u32, String> {
        let Some(&byte) = self.data.get(self.pos) else {
            return Err("deflate stream ends early".to_string());
        };
        let b = (byte >> self.bit) & 1;
        self.bit += 1;
        if self.bit == 8 {
            self.bit = 0;
            self.pos += 1;
        }
        Ok(u32::from(b))
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }

    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }
}

struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    /// Builds a decoder from code lengths (each at most 15). Over-subscribed sets are rejected,
    /// incomplete ones allowed (as zlib does for a lone distance code).
    fn new(lengths: &[u8]) -> Result<Self, String> {
        let mut count = [0u16; 16];
        for &l in lengths {
            if l > 15 {
                return Err("code length above 15".to_string());
            }
            count[usize::from(l)] += 1;
        }
        let mut left: i32 = 1;
        for &c in &count[1..] {
            left = (left << 1) - i32::from(c);
            if left < 0 {
                return Err("over-subscribed Huffman code".to_string());
            }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbol[usize::from(offs[usize::from(l)])] = sym as u16;
                offs[usize::from(l)] += 1;
            }
        }
        count[0] = 0;
        Ok(Self { count, symbol })
    }

    fn decode(&self, br: &mut BitReader) -> Result<usize, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= br.bit()? as i32;
            let count = i32::from(self.count[len]);
            if code - count < first {
                return self
                    .symbol
                    .get((index + (code - first)) as usize)
                    .map(|&s| usize::from(s))
                    .ok_or_else(|| "bad Huffman symbol".to_string());
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err("bad Huffman code".to_string())
    }
}

/// Inflates a raw DEFLATE stream. Returns the data and the byte position just after the stream.
/// Output beyond `max_out` bytes is an error, which also bounds memory for hostile input.
fn inflate(data: &[u8], max_out: usize) -> Result<(Vec<u8>, usize), String> {
    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = br.bit()?;
        match br.bits(2)? {
            0 => {
                br.align();
                let p = br.pos;
                let hdr = data
                    .get(p..p + 4)
                    .ok_or_else(|| "stored block header ends early".to_string())?;
                let len = usize::from(u16::from_le_bytes([hdr[0], hdr[1]]));
                let nlen = usize::from(u16::from_le_bytes([hdr[2], hdr[3]]));
                if len != (!nlen & 0xffff) {
                    return Err("stored block length check failed".to_string());
                }
                let body = data
                    .get(p + 4..p + 4 + len)
                    .ok_or_else(|| "stored block ends early".to_string())?;
                if out.len() + len > max_out {
                    return Err("inflated data larger than expected".to_string());
                }
                out.extend_from_slice(body);
                br.pos = p + 4 + len;
            }
            kind @ (1 | 2) => {
                let (lit, dist) = if kind == 1 {
                    let lens: Vec<u8> = (0..288).map(fixed_lit_len).collect();
                    (Huffman::new(&lens)?, Huffman::new(&[5u8; 30])?)
                } else {
                    read_dynamic_tables(&mut br)?
                };
                inflate_block(&mut br, &lit, &dist, &mut out, max_out)?;
            }
            _ => return Err("invalid deflate block type".to_string()),
        }
        if last == 1 {
            br.align();
            return Ok((out, br.pos));
        }
    }
}

fn read_dynamic_tables(br: &mut BitReader) -> Result<(Huffman, Huffman), String> {
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err("too many length or distance codes".to_string());
    }
    let mut cl_lens = [0u8; 19];
    for &s in &CL_ORDER[..hclen] {
        cl_lens[s] = br.bits(3)? as u8;
    }
    let cl = Huffman::new(&cl_lens)?;
    let mut lens = vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < lens.len() {
        let sym = cl.decode(br)?;
        let (value, repeat) = match sym {
            0..=15 => (sym as u8, 1),
            16 => {
                if i == 0 {
                    return Err("repeat code with nothing to repeat".to_string());
                }
                (lens[i - 1], 3 + br.bits(2)? as usize)
            }
            17 => (0, 3 + br.bits(3)? as usize),
            _ => (0, 11 + br.bits(7)? as usize),
        };
        if i + repeat > lens.len() {
            return Err("code lengths overflow".to_string());
        }
        lens[i..i + repeat].fill(value);
        i += repeat;
    }
    if lens[256] == 0 {
        return Err("no end-of-block code".to_string());
    }
    Ok((Huffman::new(&lens[..hlit])?, Huffman::new(&lens[hlit..])?))
}

fn inflate_block(
    br: &mut BitReader,
    lit: &Huffman,
    dist: &Huffman,
    out: &mut Vec<u8>,
    max_out: usize,
) -> Result<(), String> {
    loop {
        let sym = lit.decode(br)?;
        match sym {
            0..=255 => {
                if out.len() >= max_out {
                    return Err("inflated data larger than expected".to_string());
                }
                out.push(sym as u8);
            }
            256 => return Ok(()),
            _ => {
                let idx = sym - 257;
                if idx >= 29 {
                    return Err("invalid length symbol".to_string());
                }
                let len = usize::from(LEN_BASE[idx]) + br.bits(u32::from(LEN_EXTRA[idx]))? as usize;
                let ds = dist.decode(br)?;
                if ds >= 30 {
                    return Err("invalid distance symbol".to_string());
                }
                let d = usize::from(DIST_BASE[ds]) + br.bits(u32::from(DIST_EXTRA[ds]))? as usize;
                if d > out.len() {
                    return Err("distance reaches before the start of the data".to_string());
                }
                if out.len() + len > max_out {
                    return Err("inflated data larger than expected".to_string());
                }
                let start = out.len() - d;
                for k in 0..len {
                    out.push(out[start + k]);
                }
            }
        }
    }
}

/// Decodes a PNG produced by [`png()`] (or any 8-bit non-interlaced grey, grey+alpha, RGB or RGBA
/// PNG) back to RGBA. Checks every chunk CRC and the Adler-32. Used by tests and by tooling to
/// prove the encoder round-trips; rejects exotic PNGs (palette, 16-bit, interlaced) with `Err`.
pub fn decode_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if bytes.len() < 8 || bytes[..8] != PNG_SIGNATURE {
        return Err("not a PNG (bad signature)".to_string());
    }
    let mut pos = 8usize;
    let mut header: Option<(u32, u32, u8, u8, u8)> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut ended = false;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let kind = &bytes[pos + 4..pos + 8];
        let end = pos
            .checked_add(8)
            .and_then(|p| p.checked_add(len))
            .filter(|&e| e + 4 <= bytes.len())
            .ok_or_else(|| "PNG chunk runs past the end of the data".to_string())?;
        let data = &bytes[pos + 8..end];
        let crc = u32::from_be_bytes([bytes[end], bytes[end + 1], bytes[end + 2], bytes[end + 3]]);
        if crc != crc32(&[kind, data]) {
            return Err(format!(
                "CRC mismatch in {} chunk",
                String::from_utf8_lossy(kind)
            ));
        }
        match kind {
            b"IHDR" => {
                if data.len() != 13 || header.is_some() {
                    return Err("malformed IHDR".to_string());
                }
                let w = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                let h = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                if data[10] != 0 || data[11] != 0 {
                    return Err("unknown PNG compression or filter method".to_string());
                }
                if data[12] != 0 {
                    return Err("interlaced PNG is not supported".to_string());
                }
                header = Some((w, h, data[8], data[9], data[12]));
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => {
                ended = true;
                break;
            }
            _ => {}
        }
        pos = end + 4;
    }
    let (w, h, depth, color, _) = header.ok_or_else(|| "PNG has no IHDR".to_string())?;
    if !ended {
        return Err("PNG has no IEND".to_string());
    }
    if depth != 8 {
        return Err(format!("unsupported PNG bit depth {depth}"));
    }
    let channels = match color {
        0 => 1usize,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(format!("unsupported PNG colour type {color}")),
    };
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > (1 << 26) {
        return Err("PNG dimensions out of range".to_string());
    }
    let (wu, hu) = (w as usize, h as usize);
    let stride = wu * channels;
    let raw_len = hu * (stride + 1);
    if idat.len() < 6 {
        return Err("PNG has no image data".to_string());
    }
    let (cmf, flg) = (idat[0], idat[1]);
    if cmf & 0x0f != 8 || (u16::from(cmf) * 256 + u16::from(flg)) % 31 != 0 || flg & 0x20 != 0 {
        return Err("bad zlib header".to_string());
    }
    let (raw, used) = inflate(&idat[2..], raw_len)?;
    if raw.len() != raw_len {
        return Err("PNG image data has the wrong size".to_string());
    }
    let tail = idat
        .get(2 + used..2 + used + 4)
        .ok_or_else(|| "zlib stream has no Adler-32".to_string())?;
    if u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]) != adler32(&raw) {
        return Err("zlib Adler-32 mismatch".to_string());
    }

    let mut pixels = vec![0u8; hu * stride];
    for y in 0..hu {
        let kind = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for i in 0..stride {
            let a = if i >= channels {
                pixels[y * stride + i - channels]
            } else {
                0
            };
            let b = if y > 0 {
                pixels[(y - 1) * stride + i]
            } else {
                0
            };
            let c = if y > 0 && i >= channels {
                pixels[(y - 1) * stride + i - channels]
            } else {
                0
            };
            let x = line[i];
            pixels[y * stride + i] = match kind {
                0 => x,
                1 => x.wrapping_add(a),
                2 => x.wrapping_add(b),
                3 => x.wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                4 => x.wrapping_add(paeth(a, b, c)),
                _ => return Err(format!("bad PNG filter type {kind}")),
            };
        }
    }
    let rgba = match channels {
        4 => pixels,
        3 => pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        2 => pixels
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        _ => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
    };
    Ok((w, h, rgba))
}

// ---------------------------------------------------------------------------------------------
// ICO writer and parser.
// ---------------------------------------------------------------------------------------------

/// 32-bit BMP frame as stored in an .ico: BITMAPINFOHEADER (height doubled), bottom-up BGRA rows,
/// then the 1-bit AND mask, its rows padded to four bytes.
fn bmp_frame(rgba: &[u8], size: u32) -> Vec<u8> {
    let s = size as usize;
    let mask_row = s.div_ceil(32) * 4;
    let xor_len = s * s * 4;
    let and_len = mask_row * s;
    let mut out = Vec::with_capacity(40 + xor_len + and_len);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(size as i32 * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((xor_len + and_len) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for y in (0..s).rev() {
        for px in rgba[y * s * 4..(y + 1) * s * 4].chunks_exact(4) {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    for y in (0..s).rev() {
        let mut row = vec![0u8; mask_row];
        for (x, px) in rgba[y * s * 4..(y + 1) * s * 4].chunks_exact(4).enumerate() {
            if px[3] == 0 {
                row[x / 8] |= 0x80 >> (x % 8);
            }
        }
        out.extend_from_slice(&row);
    }
    out
}

/// Assembles an .ico from `(size, payload)` frames (32 bpp, one plane).
fn pack_ico(frames: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(frames.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * frames.len();
    for (size, payload) in frames {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += payload.len();
    }
    for (_, payload) in frames {
        out.extend_from_slice(payload);
    }
    out
}

/// One directory entry of an .ico, for validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcoFrame {
    /// Width in pixels (the directory's 0 already turned into 256).
    pub width: u32,
    /// Height in pixels (the directory's 0 already turned into 256).
    pub height: u32,
    /// Bits per pixel from the directory entry.
    pub bpp: u16,
    /// True when the frame payload is a PNG file, false for a BMP DIB.
    pub png: bool,
    /// Byte offset of the frame payload in the file.
    pub offset: u32,
    /// Byte length of the frame payload.
    pub len: u32,
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Parses an .ico (bounds-checked; `Err` on any malformed input, never panics). Besides the
/// directory it checks that every payload lies inside the file and is either a PNG whose header
/// agrees with the directory or a BMP DIB with a 40-byte header, doubled height and enough data.
pub fn parse_ico(bytes: &[u8]) -> Result<Vec<IcoFrame>, String> {
    if bytes.len() < 6 {
        return Err("ICO is shorter than its header".to_string());
    }
    if le16(bytes, 0) != 0 {
        return Err("ICO reserved field is not 0".to_string());
    }
    if le16(bytes, 2) != 1 {
        return Err("not an icon file (type is not 1)".to_string());
    }
    let count = usize::from(le16(bytes, 4));
    if count == 0 {
        return Err("ICO has no images".to_string());
    }
    let dir_end = 6 + 16 * count;
    if bytes.len() < dir_end {
        return Err("ICO directory is truncated".to_string());
    }
    let mut frames = Vec::with_capacity(count);
    for i in 0..count {
        let e = 6 + 16 * i;
        let width = if bytes[e] == 0 {
            256
        } else {
            u32::from(bytes[e])
        };
        let height = if bytes[e + 1] == 0 {
            256
        } else {
            u32::from(bytes[e + 1])
        };
        let planes = le16(bytes, e + 4);
        let bpp = le16(bytes, e + 6);
        let len = le32(bytes, e + 8);
        let offset = le32(bytes, e + 12);
        if planes > 1 {
            return Err(format!("image {i}: bad plane count {planes}"));
        }
        let start = offset as usize;
        let end = start
            .checked_add(len as usize)
            .filter(|&end| end <= bytes.len() && start >= dir_end && len > 0)
            .ok_or_else(|| format!("image {i}: payload lies outside the file"))?;
        let payload = &bytes[start..end];
        let is_png = payload.len() >= 8 && payload[..8] == PNG_SIGNATURE;
        if is_png {
            if payload.len() < 33 || &payload[12..16] != b"IHDR" {
                return Err(format!("image {i}: PNG payload has no IHDR"));
            }
            let (pw, ph) = (
                u32::from_be_bytes([payload[16], payload[17], payload[18], payload[19]]),
                u32::from_be_bytes([payload[20], payload[21], payload[22], payload[23]]),
            );
            if pw != width || ph != height {
                return Err(format!(
                    "image {i}: directory says {width}x{height} but the PNG is {pw}x{ph}"
                ));
            }
        } else {
            if payload.len() < 40 || le32(payload, 0) != 40 {
                return Err(format!("image {i}: payload is neither PNG nor a DIB"));
            }
            let (bw, bh) = (le32(payload, 4), le32(payload, 8));
            let bits = le16(payload, 14);
            if bw != width || bh != height * 2 || le16(payload, 12) != 1 || le32(payload, 16) != 0 {
                return Err(format!(
                    "image {i}: DIB header disagrees with the directory"
                ));
            }
            if bpp != 0 && bits != bpp {
                return Err(format!(
                    "image {i}: DIB bit count disagrees with the directory"
                ));
            }
            let row = (width as usize * usize::from(bits)).div_ceil(32) * 4;
            let mask = (width as usize).div_ceil(32) * 4;
            let need = 40 + row * height as usize + mask * height as usize;
            if bits != 32 || payload.len() < need {
                return Err(format!(
                    "image {i}: DIB data is shorter than its header says"
                ));
            }
        }
        frames.push(IcoFrame {
            width,
            height,
            bpp,
            png: is_png,
            offset,
            len,
        });
    }
    Ok(frames)
}

// ---------------------------------------------------------------------------------------------
// Signature.
// ---------------------------------------------------------------------------------------------

/// Composites the render over mid-grey and returns integer luminance (scale 10 000 x 255).
fn grey_luma_32(spec: &IconSpec) -> Vec<u32> {
    render(spec, 32)
        .chunks_exact(4)
        .map(|p| {
            let a = u32::from(p[3]);
            let over = |c: u8| (u32::from(c) * a + 128 * (255 - a) + 127) / 255;
            2126 * over(p[0]) + 7152 * over(p[1]) + 722 * over(p[2])
        })
        .collect()
}

/// 64-bit average hash of the 32x32 render composited over mid-grey: the 8x8 block means of the
/// luminance, a bit set when a block is above the overall mean. Bit 63 is the top-left block, bit 0
/// the bottom-right one.
pub fn signature(spec: &IconSpec) -> u64 {
    let luma = grey_luma_32(spec);
    let mut blocks = [0u64; 64];
    for (i, &l) in luma.iter().enumerate() {
        let (x, y) = (i % 32, i / 32);
        blocks[(y / 4) * 8 + x / 4] += u64::from(l);
    }
    let total: u64 = blocks.iter().sum();
    blocks.iter().enumerate().fold(0u64, |acc, (i, &b)| {
        // block mean b/16 above global mean total/1024  <=>  b * 64 > total
        acc | (u64::from(b * 64 > total) << (63 - i))
    })
}

/// Hamming distance between two [`signature`] values (0 = identical, 64 = complementary).
pub fn hash_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

// ---------------------------------------------------------------------------------------------
// Public rendering entry points and file output.
// ---------------------------------------------------------------------------------------------

/// Straight-alpha RGBA8, `size * size * 4` bytes, row-major, top row first.
///
/// Every size is drawn directly (super-sampled, with a level of detail that suits the size)
/// rather than scaled from one big image. A size of 0 gives an empty `Vec`; sizes above
/// [`MAX_RENDER_SIZE`] are clamped to it.
pub fn render(spec: &IconSpec, size: u32) -> Vec<u8> {
    let size = clamp_size(size);
    if size == 0 {
        return Vec::new();
    }
    render_design(&Design::new(spec), size)
}

/// The edge length [`render`] really uses for a request of `size` pixels.
fn clamp_size(size: u32) -> u32 {
    size.min(MAX_RENDER_SIZE)
}

/// Payload of one .ico frame: a PNG from 64 px up, a 32-bit BMP DIB below.
fn frame_payload(rgba: &[u8], size: u32) -> Vec<u8> {
    if size >= 64 {
        png(rgba, size, size)
    } else {
        bmp_frame(rgba, size)
    }
}

/// One rendered size: `(size, raw RGBA, .ico frame payload)`.
type RenderedFrame = (u32, Vec<u8>, Vec<u8>);

/// Renders every size in [`ICO_SIZES`].
fn render_all(spec: &IconSpec) -> Vec<RenderedFrame> {
    let design = Design::new(spec);
    ICO_SIZES
        .iter()
        .map(|&size| {
            let rgba = render_design(&design, size);
            let payload = frame_payload(&rgba, size);
            (size, rgba, payload)
        })
        .collect()
}

/// Multi-size ICO containing every size in [`ICO_SIZES`].
///
/// Sizes below 64 are 32-bit BMP DIB frames (BITMAPINFOHEADER with height = 2 * size, bottom-up
/// BGRA rows, then the 1-bit AND mask with rows padded to 4 bytes); sizes from 64 up are PNG frames.
/// Directory entries use width/height byte 0 for 256, one plane and 32 bits per pixel. The result
/// is accepted by the Windows shell, by `rc.exe` (`1 ICON "file.ico"`) and by [`parse_ico`].
pub fn ico(spec: &IconSpec) -> Vec<u8> {
    let frames: Vec<(u32, Vec<u8>)> = render_all(spec)
        .into_iter()
        .map(|(size, _, payload)| (size, payload))
        .collect();
    pack_ico(&frames)
}

/// The raw RGBA blobs miniquad's window icon wants: 16x16 (1024 B), 32x32 (4096 B) and 64x64
/// (16384 B).
pub fn window_icon_blobs(spec: &IconSpec) -> ([u8; 1024], [u8; 4096], [u8; 16384]) {
    let design = Design::new(spec);
    let mut small = [0u8; 1024];
    let mut medium = [0u8; 4096];
    let mut big = [0u8; 16384];
    let targets: [(&mut [u8], u32); 3] = [(&mut small, 16), (&mut medium, 32), (&mut big, 64)];
    for (dst, size) in targets {
        for (d, s) in dst.iter_mut().zip(render_design(&design, size)) {
            *d = s;
        }
    }
    (small, medium, big)
}

/// File names written by [`write_icon_set`], in the order of the returned paths.
const SET_FILES: [&str; 5] = [
    "icon.ico",
    "icon_16.rgba",
    "icon_32.rgba",
    "icon_64.rgba",
    "icon.png",
];

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Refuses (naming the file) when a target is a directory, or exists while `replace` is false.
fn check_targets(dir: &Path, names: &[&str], replace: bool) -> Result<(), String> {
    for name in names {
        let path = dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                if meta.is_dir() {
                    return Err(format!("{} is a directory", path.display()));
                }
                if !replace {
                    return Err(format!(
                        "{} already exists (pass replace to overwrite it)",
                        path.display()
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot inspect {}: {e}", path.display())),
        }
    }
    Ok(())
}

/// Writes `contents` to `dir/names` all-or-nothing. Files are first written under temporary names,
/// existing targets are moved aside, then everything is renamed into place; any failure undoes the
/// steps already taken. `hook` is called with a running index before every file system step so a
/// test can make step *k* fail.
fn commit_files(
    dir: &Path,
    names: &[&str],
    contents: &[Vec<u8>],
    replace: bool,
    hook: &mut dyn FnMut(usize) -> std::io::Result<()>,
) -> Result<Vec<PathBuf>, String> {
    check_targets(dir, names, replace)?;
    let dir_existed = dir.is_dir();
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

    let tag = format!(
        "{}.{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let finals: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
    let temps: Vec<PathBuf> = names
        .iter()
        .map(|n| dir.join(format!(".{n}.{tag}.tmp")))
        .collect();
    let mut backups: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut renamed = 0usize;
    let mut ops = 0usize;
    let mut tick = || -> std::io::Result<()> {
        let n = ops;
        ops += 1;
        hook(n)
    };

    let outcome: Result<(), String> = (|| {
        for (i, (temp, data)) in temps.iter().zip(contents).enumerate() {
            tick()
                .and_then(|()| {
                    let mut f = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(temp)?;
                    f.write_all(data)?;
                    f.flush()
                })
                .map_err(|e| format!("cannot write {}: {e}", finals[i].display()))?;
        }
        for (i, path) in finals.iter().enumerate() {
            if fs::symlink_metadata(path).is_ok() {
                let backup = dir.join(format!(".{}.{tag}.bak", names[i]));
                tick()
                    .and_then(|()| fs::rename(path, &backup))
                    .map_err(|e| format!("cannot replace {}: {e}", path.display()))?;
                backups.push((path.clone(), backup));
            }
        }
        for (i, (temp, path)) in temps.iter().zip(&finals).enumerate() {
            tick()
                .and_then(|()| fs::rename(temp, path))
                .map_err(|e| format!("cannot write {}: {e}", finals[i].display()))?;
            renamed += 1;
        }
        Ok(())
    })();

    match outcome {
        Ok(()) => {
            for (_, backup) in &backups {
                let _ = fs::remove_file(backup);
            }
            Ok(finals)
        }
        Err(message) => {
            for path in &finals[..renamed] {
                let _ = fs::remove_file(path);
            }
            for temp in &temps {
                let _ = fs::remove_file(temp);
            }
            for (path, backup) in &backups {
                let _ = fs::rename(backup, path);
            }
            if !dir_existed {
                let _ = fs::remove_dir(dir);
            }
            Err(message)
        }
    }
}

/// Writes `icon.ico`, `icon_16.rgba`, `icon_32.rgba`, `icon_64.rgba` and `icon.png` (the 256 px
/// preview) into `dir`, creating it if missing, and returns the five paths in that order.
///
/// If any of the five exists and `replace` is false the call fails naming it and writes nothing.
/// The set is all-or-nothing: on an I/O error whatever this call already wrote is removed and files
/// it replaced are restored.
pub fn write_icon_set(spec: &IconSpec, dir: &Path, replace: bool) -> Result<Vec<PathBuf>, String> {
    check_targets(dir, &SET_FILES, replace)?;
    let frames = render_all(spec);
    let raw = |size: u32| {
        frames
            .iter()
            .find(|f| f.0 == size)
            .map(|f| f.1.clone())
            .unwrap_or_default()
    };
    let preview = frames
        .iter()
        .find(|f| f.0 == 256)
        .map(|f| f.2.clone())
        .unwrap_or_default();
    let packed: Vec<(u32, Vec<u8>)> = frames.iter().map(|f| (f.0, f.2.clone())).collect();
    let contents = vec![pack_ico(&packed), raw(16), raw(32), raw(64), preview];
    commit_files(dir, &SET_FILES, &contents, replace, &mut |_| Ok(()))
}

// ---------------------------------------------------------------------------------------------
// Colour: OKLab / OKLCH to sRGB with gamut mapping.
// ---------------------------------------------------------------------------------------------

/// sRGB transfer function (linear light to encoded), input clamped to `0..=1`.
fn srgb_encode(x: f32) -> f32 {
    let x = sat(x);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * powf(x, 1.0 / 2.4) - 0.055
    }
}

/// Linear sRGB of an OKLab colour; components may fall outside `0..=1`.
fn oklab_linear(l: f32, a: f32, b: f32) -> Rgb {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_93 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_4 * s3,
        -0.004_196_086 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ]
}

/// Encoded sRGB of an OKLab colour; when it is out of gamut the chroma is scaled back until it fits.
fn oklab_srgb(l: f32, a: f32, b: f32) -> Rgb {
    let fits = |k: f32| {
        oklab_linear(l, a * k, b * k)
            .iter()
            .all(|v| (-0.001..=1.001).contains(v))
    };
    let mut k = 1.0;
    if !fits(1.0) {
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..14 {
            let mid = 0.5 * (lo + hi);
            if fits(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        k = lo;
    }
    let lin = oklab_linear(l, a * k, b * k);
    [
        srgb_encode(lin[0]),
        srgb_encode(lin[1]),
        srgb_encode(lin[2]),
    ]
}

/// Encoded sRGB of an OKLCH colour (lightness 0..1, chroma about 0..0.3, hue in degrees).
fn oklch(l: f32, c: f32, hue_deg: f32) -> Rgb {
    let (s, co) = sin_cos(hue_deg * (PI / 180.0));
    oklab_srgb(l, c * co, c * s)
}

// ---------------------------------------------------------------------------------------------
// Hero shapes: a hand-made stroke font (A-Z, 0-9) and a set of emblems for everything else.
// All shapes live in "glyph units": a box `width` wide and 100 high, y pointing down.
// ---------------------------------------------------------------------------------------------

/// One stroke of a glyph centre line.
#[derive(Clone, Copy)]
enum Prim {
    /// Straight segment from `(x0, y0)` to `(x1, y1)`; a zero-length one is a dot.
    Line(f32, f32, f32, f32),
    /// Circular arc `(cx, cy, radius, start, end)` with angles in degrees that grow clockwise on
    /// screen (y points down) and `end > start`; 0 degrees points right, 270 points up.
    Arc(f32, f32, f32, f32, f32),
}

use Prim::{Arc, Line};

/// A letter: the box width and the centre lines of its strokes.
struct GlyphDef {
    width: f32,
    prims: &'static [Prim],
}

const GLYPH_A: GlyphDef = GlyphDef {
    width: 80.0,
    prims: &[
        Line(0.0, 100.0, 40.0, 0.0),
        Line(40.0, 0.0, 80.0, 100.0),
        Line(12.8, 68.0, 67.2, 68.0),
    ],
};
const GLYPH_B: GlyphDef = GlyphDef {
    width: 66.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(0.0, 0.0, 40.0, 0.0),
        Arc(40.0, 24.0, 24.0, 270.0, 450.0),
        Line(40.0, 48.0, 0.0, 48.0),
        Arc(40.0, 74.0, 26.0, 270.0, 450.0),
        Line(40.0, 100.0, 0.0, 100.0),
    ],
};
const GLYPH_C: GlyphDef = GlyphDef {
    width: 78.0,
    prims: &[
        Arc(40.0, 38.5, 40.0, 180.0, 322.0),
        Line(0.0, 38.5, 0.0, 61.5),
        Arc(40.0, 61.5, 40.0, 38.0, 180.0),
    ],
};
const GLYPH_D: GlyphDef = GlyphDef {
    width: 74.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(0.0, 0.0, 38.0, 0.0),
        Arc(38.0, 36.0, 36.0, 270.0, 360.0),
        Line(74.0, 36.0, 74.0, 64.0),
        Arc(38.0, 64.0, 36.0, 0.0, 90.0),
        Line(38.0, 100.0, 0.0, 100.0),
    ],
};
const GLYPH_E: GlyphDef = GlyphDef {
    width: 60.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(0.0, 0.0, 60.0, 0.0),
        Line(0.0, 50.0, 50.0, 50.0),
        Line(0.0, 100.0, 60.0, 100.0),
    ],
};
const GLYPH_F: GlyphDef = GlyphDef {
    width: 58.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(0.0, 0.0, 58.0, 0.0),
        Line(0.0, 48.0, 48.0, 48.0),
    ],
};
const GLYPH_G: GlyphDef = GlyphDef {
    width: 80.0,
    prims: &[
        Arc(40.0, 38.5, 40.0, 180.0, 322.0),
        Line(0.0, 38.5, 0.0, 61.5),
        Arc(40.0, 61.5, 40.0, 0.0, 180.0),
        Line(80.0, 61.5, 80.0, 54.0),
        Line(80.0, 54.0, 44.0, 54.0),
    ],
};
const GLYPH_H: GlyphDef = GlyphDef {
    width: 70.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(70.0, 0.0, 70.0, 100.0),
        Line(0.0, 50.0, 70.0, 50.0),
    ],
};
const GLYPH_I: GlyphDef = GlyphDef {
    width: 44.0,
    prims: &[
        Line(22.0, 0.0, 22.0, 100.0),
        Line(0.0, 0.0, 44.0, 0.0),
        Line(0.0, 100.0, 44.0, 100.0),
    ],
};
const GLYPH_J: GlyphDef = GlyphDef {
    width: 60.0,
    prims: &[
        Line(24.0, 0.0, 60.0, 0.0),
        Line(60.0, 0.0, 60.0, 71.5),
        Arc(30.0, 71.5, 30.0, 0.0, 180.0),
    ],
};
const GLYPH_K: GlyphDef = GlyphDef {
    width: 68.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(68.0, 0.0, 2.0, 56.0),
        Line(26.0, 36.0, 68.0, 100.0),
    ],
};
const GLYPH_L: GlyphDef = GlyphDef {
    width: 58.0,
    prims: &[Line(0.0, 0.0, 0.0, 100.0), Line(0.0, 100.0, 58.0, 100.0)],
};
const GLYPH_M: GlyphDef = GlyphDef {
    width: 92.0,
    prims: &[
        Line(0.0, 100.0, 0.0, 0.0),
        Line(0.0, 0.0, 46.0, 64.0),
        Line(46.0, 64.0, 92.0, 0.0),
        Line(92.0, 0.0, 92.0, 100.0),
    ],
};
const GLYPH_N: GlyphDef = GlyphDef {
    width: 72.0,
    prims: &[
        Line(0.0, 100.0, 0.0, 0.0),
        Line(0.0, 0.0, 72.0, 100.0),
        Line(72.0, 100.0, 72.0, 0.0),
    ],
};
const GLYPH_O: GlyphDef = GlyphDef {
    width: 80.0,
    prims: &[
        Arc(40.0, 38.5, 40.0, 180.0, 360.0),
        Arc(40.0, 61.5, 40.0, 0.0, 180.0),
        Line(0.0, 38.5, 0.0, 61.5),
        Line(80.0, 38.5, 80.0, 61.5),
    ],
};
const GLYPH_P: GlyphDef = GlyphDef {
    width: 65.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(0.0, 0.0, 38.0, 0.0),
        Arc(38.0, 27.0, 27.0, 270.0, 450.0),
        Line(38.0, 54.0, 0.0, 54.0),
    ],
};
const GLYPH_Q: GlyphDef = GlyphDef {
    width: 80.0,
    prims: &[
        Arc(40.0, 38.5, 40.0, 180.0, 360.0),
        Arc(40.0, 61.5, 40.0, 0.0, 180.0),
        Line(0.0, 38.5, 0.0, 61.5),
        Line(80.0, 38.5, 80.0, 61.5),
        Line(46.0, 68.0, 80.0, 106.0),
    ],
};
const GLYPH_R: GlyphDef = GlyphDef {
    width: 68.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 100.0),
        Line(0.0, 0.0, 38.0, 0.0),
        Arc(38.0, 27.0, 27.0, 270.0, 450.0),
        Line(38.0, 54.0, 0.0, 54.0),
        Line(34.0, 54.0, 68.0, 100.0),
    ],
};
const GLYPH_S: GlyphDef = GlyphDef {
    width: 52.0,
    prims: &[
        Arc(26.0, 24.25, 25.75, 90.0, 330.0),
        Arc(26.0, 75.75, 25.75, 270.0, 510.0),
    ],
};
const GLYPH_T: GlyphDef = GlyphDef {
    width: 72.0,
    prims: &[Line(0.0, 0.0, 72.0, 0.0), Line(36.0, 0.0, 36.0, 100.0)],
};
const GLYPH_U: GlyphDef = GlyphDef {
    width: 74.0,
    prims: &[
        Line(0.0, 0.0, 0.0, 64.5),
        Arc(37.0, 64.5, 37.0, 0.0, 180.0),
        Line(74.0, 64.5, 74.0, 0.0),
    ],
};
const GLYPH_V: GlyphDef = GlyphDef {
    width: 80.0,
    prims: &[Line(0.0, 0.0, 40.0, 100.0), Line(40.0, 100.0, 80.0, 0.0)],
};
const GLYPH_W: GlyphDef = GlyphDef {
    width: 106.0,
    prims: &[
        Line(0.0, 0.0, 27.0, 100.0),
        Line(27.0, 100.0, 53.0, 30.0),
        Line(53.0, 30.0, 79.0, 100.0),
        Line(79.0, 100.0, 106.0, 0.0),
    ],
};
const GLYPH_X: GlyphDef = GlyphDef {
    width: 76.0,
    prims: &[Line(0.0, 0.0, 76.0, 100.0), Line(76.0, 0.0, 0.0, 100.0)],
};
const GLYPH_Y: GlyphDef = GlyphDef {
    width: 76.0,
    prims: &[
        Line(0.0, 0.0, 38.0, 48.0),
        Line(76.0, 0.0, 38.0, 48.0),
        Line(38.0, 48.0, 38.0, 100.0),
    ],
};
const GLYPH_Z: GlyphDef = GlyphDef {
    width: 68.0,
    prims: &[
        Line(0.0, 0.0, 68.0, 0.0),
        Line(68.0, 0.0, 0.0, 100.0),
        Line(0.0, 100.0, 68.0, 100.0),
    ],
};
const GLYPH_0: GlyphDef = GlyphDef {
    width: 62.0,
    prims: &[
        Arc(31.0, 29.5, 31.0, 180.0, 360.0),
        Arc(31.0, 70.5, 31.0, 0.0, 180.0),
        Line(0.0, 29.5, 0.0, 70.5),
        Line(62.0, 29.5, 62.0, 70.5),
    ],
};
const GLYPH_1: GlyphDef = GlyphDef {
    width: 46.0,
    prims: &[
        Line(28.0, 0.0, 28.0, 100.0),
        Line(28.0, 0.0, 4.0, 20.0),
        Line(4.0, 100.0, 46.0, 100.0),
    ],
};
const GLYPH_2: GlyphDef = GlyphDef {
    width: 62.0,
    prims: &[
        Arc(31.0, 30.0, 31.5, 190.0, 400.0),
        Line(55.1, 50.2, 0.0, 100.0),
        Line(0.0, 100.0, 62.0, 100.0),
    ],
};
const GLYPH_3: GlyphDef = GlyphDef {
    width: 56.0,
    prims: &[
        Arc(28.0, 24.25, 25.75, 200.0, 450.0),
        Arc(28.0, 75.75, 25.75, 270.0, 520.0),
    ],
};
const GLYPH_4: GlyphDef = GlyphDef {
    width: 68.0,
    prims: &[
        Line(48.0, 0.0, 48.0, 100.0),
        Line(48.0, 0.0, 0.0, 68.0),
        Line(0.0, 68.0, 68.0, 68.0),
    ],
};
const GLYPH_5: GlyphDef = GlyphDef {
    width: 56.0,
    prims: &[
        Line(54.0, 0.0, 6.0, 0.0),
        Line(6.0, 0.0, 2.0, 49.0),
        Arc(27.0, 69.5, 32.0, 217.0, 510.0),
    ],
};
const GLYPH_6: GlyphDef = GlyphDef {
    width: 58.0,
    prims: &[
        Arc(29.0, 70.0, 31.5, 0.0, 360.0),
        Arc(67.0, 70.0, 69.0, 180.0, 258.0),
    ],
};
const GLYPH_7: GlyphDef = GlyphDef {
    width: 58.0,
    prims: &[Line(0.0, 0.0, 58.0, 0.0), Line(58.0, 0.0, 18.0, 100.0)],
};
const GLYPH_8: GlyphDef = GlyphDef {
    width: 56.0,
    prims: &[
        Arc(28.0, 24.25, 25.75, 0.0, 360.0),
        Arc(28.0, 75.75, 25.75, 0.0, 360.0),
    ],
};
const GLYPH_9: GlyphDef = GlyphDef {
    width: 58.0,
    prims: &[
        Arc(29.0, 30.0, 31.5, 0.0, 360.0),
        Arc(-9.0, 30.0, 69.0, 0.0, 78.0),
    ],
};

/// Upper-case ASCII letter or digit for `c`, folding the Latin-1 accented letters onto their base
/// letter; `None` for anything without a glyph (punctuation, emoji, CJK, ...).
fn glyph_char(c: char) -> Option<char> {
    let base = match c {
        'À'..='Å' | 'à'..='å' => 'A',
        'Ç' | 'ç' => 'C',
        'È'..='Ë' | 'è'..='ë' => 'E',
        'Ì'..='Ï' | 'ì'..='ï' => 'I',
        'Ñ' | 'ñ' => 'N',
        'Ò'..='Ö' | 'Ø' | 'ò'..='ö' | 'ø' => 'O',
        'Ù'..='Ü' | 'ù'..='ü' => 'U',
        'Ý' | 'ý' | 'ÿ' => 'Y',
        'ß' => 'S',
        other => other,
    };
    let up = base.to_ascii_uppercase();
    up.is_ascii_alphanumeric().then_some(up)
}

fn glyph_table(c: char) -> Option<&'static GlyphDef> {
    Some(match c {
        'A' => &GLYPH_A,
        'B' => &GLYPH_B,
        'C' => &GLYPH_C,
        'D' => &GLYPH_D,
        'E' => &GLYPH_E,
        'F' => &GLYPH_F,
        'G' => &GLYPH_G,
        'H' => &GLYPH_H,
        'I' => &GLYPH_I,
        'J' => &GLYPH_J,
        'K' => &GLYPH_K,
        'L' => &GLYPH_L,
        'M' => &GLYPH_M,
        'N' => &GLYPH_N,
        'O' => &GLYPH_O,
        'P' => &GLYPH_P,
        'Q' => &GLYPH_Q,
        'R' => &GLYPH_R,
        'S' => &GLYPH_S,
        'T' => &GLYPH_T,
        'U' => &GLYPH_U,
        'V' => &GLYPH_V,
        'W' => &GLYPH_W,
        'X' => &GLYPH_X,
        'Y' => &GLYPH_Y,
        'Z' => &GLYPH_Z,
        '0' => &GLYPH_0,
        '1' => &GLYPH_1,
        '2' => &GLYPH_2,
        '3' => &GLYPH_3,
        '4' => &GLYPH_4,
        '5' => &GLYPH_5,
        '6' => &GLYPH_6,
        '7' => &GLYPH_7,
        '8' => &GLYPH_8,
        '9' => &GLYPH_9,
        _ => return None,
    })
}

/// A hero shape ready to lay out: stroked centre lines plus filled, rounded polygons.
struct HeroDef {
    /// Width of the box in glyph units (the height is always 100).
    width: f32,
    /// Stroke half width in glyph units at weight 1.
    hw: f32,
    strokes: Vec<Prim>,
    /// Filled polygons with the radius that rounds their corners.
    polys: Vec<(Vec<[f32; 2]>, f32)>,
    /// True for letters and digits (which get pixel-grid fitting at small sizes).
    letter: bool,
}

/// Number of different emblems.
const EMBLEM_COUNT: usize = 12;

fn letter_def(c: char) -> Option<HeroDef> {
    glyph_table(glyph_char(c)?).map(|g| HeroDef {
        width: g.width,
        hw: 8.5,
        strokes: g.prims.to_vec(),
        polys: Vec::new(),
        letter: true,
    })
}

/// Regular polygon vertices around `(cx, cy)`; `first` is the angle of the first one in degrees.
fn regular_poly(cx: f32, cy: f32, r: f32, sides: usize, first: f32) -> Vec<[f32; 2]> {
    (0..sides)
        .map(|k| {
            let a = (first + 360.0 * k as f32 / sides as f32) * (PI / 180.0);
            let (s, c) = sin_cos(a);
            [cx + r * c, cy + r * s]
        })
        .collect()
}

/// Closed outline of consecutive `Line` strokes through `pts`.
fn outline(pts: &[[f32; 2]]) -> Vec<Prim> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            Line(a[0], a[1], b[0], b[1])
        })
        .collect()
}

/// Crescent moon: circle 1 with circle 2 cut out of it, opening towards the upper right.
fn crescent_points() -> Vec<[f32; 2]> {
    let (c1, r1) = ([46.0f32, 53.0], 43.0f32);
    let (c2, r2) = ([65.0f32, 39.0], 35.0f32);
    let (dx, dy) = (c2[0] - c1[0], c2[1] - c1[1]);
    let d = (dx * dx + dy * dy).sqrt();
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let phi = atan2(dy, dx);
    let gamma = atan2(h, a);
    let delta = atan2(h, d - a);
    let steps = 18;
    // the part of circle 1 outside circle 2, then the part of circle 2 inside circle 1
    let mut pts = Vec::new();
    for k in 0..=steps {
        let t = phi + gamma + (TAU - 2.0 * gamma) * k as f32 / steps as f32;
        let (s, c) = sin_cos(t);
        pts.push([c1[0] + r1 * c, c1[1] + r1 * s]);
    }
    let end = pts[pts.len() - 1];
    let inner = |t: f32| {
        let (s, c) = sin_cos(phi + PI + t);
        [c2[0] + r2 * c, c2[1] + r2 * s]
    };
    let gap = |p: [f32; 2]| (p[0] - end[0]) * (p[0] - end[0]) + (p[1] - end[1]) * (p[1] - end[1]);
    let forward = gap(inner(-delta)) <= gap(inner(delta));
    for k in 1..steps {
        let f = k as f32 / steps as f32;
        pts.push(inner(if forward {
            -delta + 2.0 * delta * f
        } else {
            delta - 2.0 * delta * f
        }));
    }
    pts
}

/// Closed Catmull-Rom spline through `ctl`, `per` samples per segment.
fn spline(ctl: &[[f32; 2]], per: usize) -> Vec<[f32; 2]> {
    let n = ctl.len();
    let mut out = Vec::with_capacity(n * per);
    for i in 0..n {
        let p0 = ctl[(i + n - 1) % n];
        let p1 = ctl[i];
        let p2 = ctl[(i + 1) % n];
        let p3 = ctl[(i + 2) % n];
        for k in 0..per {
            let t = k as f32 / per as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * (2.0 * b
                    + (c - a) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (3.0 * b - a - 3.0 * c + d) * t3)
            };
            out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    out
}

fn heart_points() -> Vec<[f32; 2]> {
    (0..36)
        .map(|k| {
            let t = TAU * k as f32 / 36.0;
            let (s1, c1) = sin_cos(t);
            let (_, c2) = sin_cos(2.0 * t);
            let (_, c3) = sin_cos(3.0 * t);
            let (_, c4) = sin_cos(4.0 * t);
            let x = 16.0 * s1 * s1 * s1;
            let y = -(13.0 * c1 - 5.0 * c2 - 2.0 * c3 - c4);
            [50.0 + x * 2.7, 47.0 + y * 2.7]
        })
        .collect()
}

/// Emblem number `kind` (taken modulo [`EMBLEM_COUNT`]).
fn emblem_def(kind: usize) -> HeroDef {
    let mut def = HeroDef {
        width: 100.0,
        hw: 9.0,
        strokes: Vec::new(),
        polys: Vec::new(),
        letter: false,
    };
    match kind % EMBLEM_COUNT {
        0 => {
            // lightning bolt
            def.polys.push((
                vec![
                    [63.0, 6.0],
                    [22.0, 56.0],
                    [46.0, 56.0],
                    [36.0, 94.0],
                    [80.0, 40.0],
                    [55.0, 40.0],
                ],
                5.0,
            ));
        }
        1 => {
            // five-point star
            let mut pts = Vec::new();
            for k in 0..10 {
                let r = if k % 2 == 0 { 42.0 } else { 20.0 };
                let a = (-90.0 + 36.0 * k as f32) * (PI / 180.0);
                let (s, c) = sin_cos(a);
                pts.push([50.0 + r * c, 54.0 + r * s]);
            }
            def.polys.push((pts, 5.0));
        }
        2 => {
            // target: a ring around a dot
            def.hw = 11.0;
            def.strokes.push(Arc(50.0, 50.0, 34.0, 0.0, 360.0));
            def.strokes.push(Line(50.0, 50.0, 50.0, 50.0));
        }
        3 => {
            // two stacked chevrons
            def.hw = 10.0;
            def.strokes.push(Line(10.0, 50.0, 50.0, 14.0));
            def.strokes.push(Line(50.0, 14.0, 90.0, 50.0));
            def.strokes.push(Line(10.0, 84.0, 50.0, 48.0));
            def.strokes.push(Line(50.0, 48.0, 90.0, 84.0));
        }
        4 => {
            // gem: a diamond outline round a small solid diamond
            def.hw = 8.5;
            let ring = [[50.0, 8.0], [92.0, 50.0], [50.0, 92.0], [8.0, 50.0]];
            def.strokes = outline(&ring);
            def.polys.push((
                vec![[50.0, 32.0], [68.0, 50.0], [50.0, 68.0], [32.0, 50.0]],
                3.0,
            ));
        }
        5 => {
            // hexagon outline round a small hexagon
            def.hw = 9.0;
            def.strokes = outline(&regular_poly(50.0, 50.0, 40.0, 6, -90.0));
            def.polys
                .push((regular_poly(50.0, 50.0, 14.0, 6, -90.0), 3.0));
        }
        6 => {
            // plus
            def.hw = 15.0;
            def.strokes.push(Line(50.0, 18.0, 50.0, 82.0));
            def.strokes.push(Line(18.0, 50.0, 82.0, 50.0));
        }
        7 => {
            def.polys.push((crescent_points(), 3.0));
        }
        8 => {
            // flame
            let ctl = [
                [58.0, 3.0],
                [70.0, 24.0],
                [86.0, 48.0],
                [84.0, 72.0],
                [67.0, 92.0],
                [46.0, 97.0],
                [26.0, 90.0],
                [14.0, 68.0],
                [19.0, 46.0],
                [31.0, 30.0],
                [37.0, 46.0],
                [45.0, 22.0],
            ];
            def.polys.push((spline(&ctl, 4), 2.0));
        }
        9 => {
            // four-point sparkle
            def.polys.push((
                vec![
                    [50.0, 4.0],
                    [59.0, 41.0],
                    [96.0, 50.0],
                    [59.0, 59.0],
                    [50.0, 96.0],
                    [41.0, 59.0],
                    [4.0, 50.0],
                    [41.0, 41.0],
                ],
                2.0,
            ));
        }
        10 => {
            // play triangle
            def.polys
                .push((vec![[27.0, 12.0], [90.0, 50.0], [27.0, 88.0]], 8.0));
        }
        _ => {
            def.polys.push((heart_points(), 2.0));
        }
    }
    def
}

/// Exact distance from `(px, py)` to the segment `(ax, ay)`-`(bx, by)`.
fn segment_dist(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    let (ex, ey) = (bx - ax, by - ay);
    let (wx, wy) = (px - ax, py - ay);
    let len2 = ex * ex + ey * ey;
    let t = if len2 > 1e-9 {
        sat((wx * ex + wy * ey) / len2)
    } else {
        0.0
    };
    let (dx, dy) = (wx - ex * t, wy - ey * t);
    (dx * dx + dy * dy).sqrt()
}

/// Signed distance to a simple polygon: negative inside, positive outside.
fn poly_sd(pts: &[[f32; 2]], px: f32, py: f32) -> f32 {
    let Some(&last) = pts.last() else {
        return f32::MAX;
    };
    let mut prev = last;
    let mut best = f32::MAX;
    let mut inside = false;
    for &cur in pts {
        best = best.min(segment_dist(prev[0], prev[1], cur[0], cur[1], px, py));
        if (cur[1] > py) != (prev[1] > py)
            && px < (prev[0] - cur[0]) * (py - cur[1]) / (prev[1] - cur[1]) + cur[0]
        {
            inside = !inside;
        }
        prev = cur;
    }
    if inside {
        -best
    } else {
        best
    }
}

// ---------------------------------------------------------------------------------------------
// Design: everything the title (and variant) decides, before any pixel is drawn.
// ---------------------------------------------------------------------------------------------

const LUT_N: usize = 33;
const BLOB_N: usize = 6;
const COARSE_N: usize = 5;
const FINE_N: usize = 9;

/// Fine texture drawn over the tile from 40 px up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PatternKind {
    Stripes,
    Dots,
    Rays,
    Grid,
    Checker,
    Rings,
    Chevrons,
    Diamonds,
    Waves,
}

const PATTERNS: [PatternKind; 9] = [
    PatternKind::Stripes,
    PatternKind::Dots,
    PatternKind::Rays,
    PatternKind::Grid,
    PatternKind::Checker,
    PatternKind::Rings,
    PatternKind::Chevrons,
    PatternKind::Diamonds,
    PatternKind::Waves,
];

/// Small feature on the tile besides the hero, drawn from 40 px up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AccentKind {
    Fold,
    Sparkle,
    Orbit,
    Gloss,
    Dots,
    Keyline,
}

const ACCENTS: [AccentKind; 6] = [
    AccentKind::Fold,
    AccentKind::Sparkle,
    AccentKind::Orbit,
    AccentKind::Gloss,
    AccentKind::Dots,
    AccentKind::Keyline,
];

/// Big soft or crisp shape that shapes the tile's tone; part of the tile, so present at every size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OverlayKind {
    Half,
    Disc,
    Band,
    Spot,
    Wave,
    Ring,
    Plate,
    Blob,
}

/// One tone overlay in tile space (the tile spans -1..1 on both axes).
struct Overlay {
    kind: OverlayKind,
    /// Direction for `Half`, `Band` and `Wave`.
    dir: [f32; 2],
    /// Kind specific numbers (offset, centre, radius, softness, ...).
    p: [f32; 6],
    /// Random lattice that shapes a `Blob`.
    lat: [[f32; BLOB_N]; BLOB_N],
    tint: Rgb,
    alpha: f32,
    /// Whether the hero switches to its second colour set where this overlay covers it.
    flips: bool,
}

struct PatternParams {
    cos: f32,
    sin: f32,
    period: f32,
    duty: f32,
    phase: f32,
    cx: f32,
    cy: f32,
    /// Ray count for `Rays`, wave frequency for `Waves`, chevron slope for `Chevrons`.
    extra: f32,
    /// Wave amplitude.
    amp: f32,
    alpha: f32,
    color: Rgb,
    /// Radius around the tile centre inside which the pattern fades out (keeps the hero calm).
    clear: f32,
    /// Direction the pattern fades in towards, and where along it the fade starts: the pattern
    /// covers only part of the tile.
    fade_dir: [f32; 2],
    fade_from: f32,
}

struct AccentParams {
    /// Which corner (each component +1 or -1).
    corner: [f32; 2],
    size: f32,
    angle: f32,
    gap: f32,
    color: Rgb,
}

struct Design {
    lut: [Rgb; LUT_N],
    dir: [f32; 2],
    dir_norm: f32,
    coarse: [[f32; COARSE_N]; COARSE_N],
    fine: [[f32; FINE_N]; FINE_N],
    coarse_amp: f32,
    fine_amp: f32,
    overlays: Vec<Overlay>,
    /// True when at least one overlay flips the hero colours.
    has_flip: bool,
    pattern: PatternKind,
    pat: PatternParams,
    accent: AccentKind,
    acc: AccentParams,
    hero: HeroDef,
    weight: f32,
    hero_scale: f32,
    hero_off: [f32; 2],
    white: Rgb,
    shade: Rgb,
    hero_a: HeroColors,
    hero_b: HeroColors,
}

/// Fill, outline and shadow colours of the hero.
#[derive(Clone, Copy)]
struct HeroColors {
    top: Rgb,
    bot: Rgb,
    line: Rgb,
    shadow: Rgb,
}

/// Hue (degrees) for `u` in `0..360`, spread over the wheel with the `bands` (sorted, degrees)
/// left out, so the palette can avoid plain blue and, for light glyphs, muddy olive.
fn allowed_hue(u: f32, bands: &[(f32, f32)]) -> f32 {
    let total: f32 = 360.0 - bands.iter().map(|&(lo, hi)| hi - lo).sum::<f32>();
    let mut h = u.rem_euclid(360.0) / 360.0 * total;
    for &(lo, hi) in bands {
        if h >= lo {
            h += hi - lo;
        }
    }
    h
}

const BLUE_BAND: (f32, f32) = (226.0, 296.0);
/// Orange-yellow and yellow: they look best as light tiles with a dark hero.
const WARM_BAND: (f32, f32) = (50.0, 85.0);
/// Olive, khaki and lime-green: dull at tile lightness, so never used.
const OLIVE_BAND: (f32, f32) = (85.0, 150.0);

fn in_band(hue: f32, band: (f32, f32)) -> bool {
    (band.0..band.1).contains(&hue.rem_euclid(360.0))
}

/// Smooth interpolation of an `N` x `N` lattice of values over the tile (`-1..=1` on both axes).
fn lattice<const N: usize>(m: &[[f32; N]; N], tx: f32, ty: f32) -> f32 {
    let last = (N - 1) as f32;
    let u = sat(tx * 0.5 + 0.5) * last;
    let w = sat(ty * 0.5 + 0.5) * last;
    let iu = (u as usize).min(N - 2);
    let iw = (w as usize).min(N - 2);
    let (fu, fw) = (u - iu as f32, w - iw as f32);
    let (su, sw) = (fu * fu * (3.0 - 2.0 * fu), fw * fw * (3.0 - 2.0 * fw));
    let row0 = mix(m[iw][iu], m[iw][iu + 1], su);
    let row1 = mix(m[iw + 1][iu], m[iw + 1][iu + 1], su);
    mix(row0, row1, sw)
}

/// Smoothly resamples a small lattice to the blob lattice size.
fn upsample<const N: usize>(small: &[[f32; N]; N]) -> [[f32; BLOB_N]; BLOB_N] {
    let mut out = [[0.0; BLOB_N]; BLOB_N];
    for (j, row) in out.iter_mut().enumerate() {
        for (i, cell) in row.iter_mut().enumerate() {
            let tx = i as f32 / (BLOB_N - 1) as f32 * 2.0 - 1.0;
            let ty = j as f32 / (BLOB_N - 1) as f32 * 2.0 - 1.0;
            *cell = lattice(small, tx, ty);
        }
    }
    out
}

fn random_lattice<const N: usize>(rng: &mut Rng) -> [[f32; N]; N] {
    let mut m = [[0.0; N]; N];
    for row in m.iter_mut() {
        for cell in row.iter_mut() {
            *cell = rng.range(-1.0, 1.0);
        }
    }
    m
}

/// A random overlay of `kind`. Crisp ones have a hard edge and reach over the hero, soft ones
/// blur their edge and may hug a corner.
fn make_overlay(
    kind: OverlayKind,
    crisp: bool,
    tint: Rgb,
    alpha: f32,
    flips: bool,
    rng: &mut Rng,
) -> Overlay {
    let (sa, ca) = sin_cos(rng.range(0.0, TAU));
    let (corner_x, corner_y) = (rng.sign(), rng.sign());
    let soft = |rng: &mut Rng| if crisp { 0.012 } else { rng.range(0.02, 0.06) };
    let p = match kind {
        OverlayKind::Half => [rng.range(-0.4, 0.4), soft(rng), 0.0, 0.0, 0.0, 0.0],
        OverlayKind::Disc if crisp => [
            rng.range(-0.55, 0.55),
            rng.range(-0.55, 0.55),
            rng.range(0.5, 0.95),
            soft(rng),
            0.0,
            0.0,
        ],
        OverlayKind::Disc => [
            corner_x * rng.range(0.55, 1.0),
            corner_y * rng.range(0.55, 1.0),
            rng.range(0.7, 1.2),
            soft(rng),
            0.0,
            0.0,
        ],
        OverlayKind::Band => [
            rng.range(-0.45, 0.45),
            rng.range(0.14, 0.32),
            soft(rng),
            0.0,
            0.0,
            0.0,
        ],
        OverlayKind::Spot => [
            rng.range(-0.6, 0.6),
            rng.range(-0.6, 0.6),
            rng.range(1.0, 1.5),
            0.0,
            0.0,
            0.0,
        ],
        OverlayKind::Wave => [
            rng.range(-0.35, 0.35),
            rng.range(0.08, 0.2),
            rng.range(2.2, 4.2),
            rng.range(0.0, TAU),
            soft(rng),
            0.0,
        ],
        OverlayKind::Ring => [
            rng.range(-0.35, 0.35),
            rng.range(-0.35, 0.35),
            rng.range(0.55, 0.95),
            rng.range(0.10, 0.2),
            soft(rng),
            0.0,
        ],
        OverlayKind::Plate => [
            0.0,
            0.0,
            rng.range(0.66, 0.80),
            rng.range(0.03, 0.06),
            0.0,
            0.0,
        ],
        OverlayKind::Blob => [
            rng.range(-0.15, 0.3),
            if crisp { 0.04 } else { rng.range(0.2, 0.4) },
            0.0,
            0.0,
            0.0,
            0.0,
        ],
    };
    let lat = if kind == OverlayKind::Blob {
        // a small random lattice, smoothly upsampled: few, large lobes rather than camouflage
        if rng.chance(0.5) {
            upsample::<5>(&random_lattice::<5>(rng))
        } else {
            upsample::<4>(&random_lattice::<4>(rng))
        }
    } else {
        [[0.0; BLOB_N]; BLOB_N]
    };
    Overlay {
        kind,
        dir: [ca, sa],
        p,
        lat,
        tint,
        alpha,
        flips,
    }
}

impl Design {
    fn new(spec: &IconSpec) -> Self {
        let title = spec.title.trim().to_lowercase();
        let base = fnv1a64(title.as_bytes());
        let v = u64::from(spec.variant);
        // Discrete choices come from the title alone and are then rotated by the variant, so two
        // variants always differ in them; continuous numbers use title and variant together.
        let mut pick = Rng::new(mix64(base));
        let mut rng = Rng::new(mix64(
            base ^ v.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x51ed_270b,
        ));
        let vu = v as usize;

        // ---- hue family and polarity: warm hues always make light tiles with a dark hero
        let hue_shift = ((v * 137_508) / 1000 % 360) as f32;
        let hue1 = allowed_hue(pick.unit() * 360.0 + hue_shift, &[OLIVE_BAND, BLUE_BAND]);
        let polarity_roll = pick.unit();
        let light_glyph = !in_band(hue1, WARM_BAND) && polarity_roll >= 0.15;
        let bands: Vec<(f32, f32)> = if light_glyph {
            vec![WARM_BAND, OLIVE_BAND, BLUE_BAND]
        } else {
            vec![OLIVE_BAND, BLUE_BAND]
        };
        let mag = match pick.below(100) {
            0..=37 => rng.range(22.0, 46.0),
            38..=67 => rng.range(52.0, 84.0),
            68..=83 => rng.range(90.0, 112.0),
            _ => rng.range(8.0, 18.0),
        };
        let mut delta = mag * rng.sign();
        let crosses = |delta: f32| {
            (0..=8).any(|k| {
                let h = hue1 + delta * k as f32 / 8.0;
                bands.iter().any(|&band| in_band(h, band))
            })
        };
        if crosses(delta) {
            delta = -delta;
        }
        if crosses(delta) {
            delta = delta.signum() * 14.0;
        }
        if crosses(delta) {
            delta = -delta;
        }
        if crosses(delta) {
            delta = 0.0;
        }
        let hue2 = hue1 + delta;

        // ---- gradient and the two noise lattices
        let tone = pick.below(100);
        let (la, lb, ca, cb) = if !light_glyph {
            (
                rng.range(0.84, 0.90),
                rng.range(0.72, 0.80),
                rng.range(0.14, 0.20),
                rng.range(0.12, 0.18),
            )
        } else if tone < 25 {
            // jewel tones
            (
                rng.range(0.54, 0.62),
                rng.range(0.36, 0.44),
                rng.range(0.15, 0.22),
                rng.range(0.14, 0.20),
            )
        } else if tone < 80 {
            (
                rng.range(0.60, 0.72),
                rng.range(0.40, 0.52),
                rng.range(0.14, 0.20),
                rng.range(0.13, 0.19),
            )
        } else {
            // bright
            (
                rng.range(0.68, 0.76),
                rng.range(0.54, 0.62),
                rng.range(0.14, 0.19),
                rng.range(0.13, 0.18),
            )
        };
        let mut lut = [[0.0; 3]; LUT_N];
        for (i, slot) in lut.iter_mut().enumerate() {
            let t = i as f32 / (LUT_N - 1) as f32;
            *slot = oklch(mix(la, lb, t), mix(ca, cb, t), hue1 + delta * t);
        }
        let (ds, dc) = sin_cos(rng.range(0.0, TAU));
        let dir = [dc, ds];
        let dir_norm = 0.5 / (dc.abs() + ds.abs());
        let coarse = random_lattice::<COARSE_N>(&mut rng);
        let fine = random_lattice::<FINE_N>(&mut rng);
        let coarse_amp = rng.range(0.07, 0.11);
        let fine_amp = rng.range(0.03, 0.05);

        let white = oklch(0.97, 0.03, hue1);
        let shade = oklch(0.22, 0.06, hue2);
        let pale = oklch(0.93, 0.055, hue1 + 20.0);
        let deep = oklch(0.30, 0.09, hue2);

        // ---- composition: two bold organic shapes plus one recipe, so every tile has an idea
        let mut overlays = Vec::new();
        // `helps` darkens under a light hero and lightens under a dark one (plates); `bold_tint`
        // moves the tile towards the hero's own tone, which is what shapes its tonal structure.
        let (helps, sheen) = if light_glyph {
            (shade, white)
        } else {
            (white, shade)
        };
        let bold_tint = if light_glyph {
            oklch(0.80, 0.10, hue2 + 30.0)
        } else {
            oklch(0.42, 0.14, hue2 - 35.0)
        };
        let flip_tint = if light_glyph { pale } else { deep };
        let flip_kinds = [OverlayKind::Half, OverlayKind::Band, OverlayKind::Disc];
        let duo_kinds = [
            OverlayKind::Half,
            OverlayKind::Band,
            OverlayKind::Disc,
            OverlayKind::Ring,
            OverlayKind::Wave,
        ];
        let recipe = (pick.below(100) + (vu % 100) * 37) % 100;
        let kind_seed = pick.below(duo_kinds.len()) + (vu % duo_kinds.len()) * 3;
        let flip_kind =
            flip_kinds[(pick.below(flip_kinds.len()) + vu % flip_kinds.len()) % flip_kinds.len()];
        let first = rng.range(0.35, 0.5);
        overlays.push(make_overlay(
            OverlayKind::Blob,
            true,
            bold_tint,
            first,
            false,
            &mut rng,
        ));
        let (tint, alpha) = if rng.chance(0.5) {
            (sheen, rng.range(0.14, 0.26))
        } else {
            (bold_tint, rng.range(0.2, 0.3))
        };
        overlays.push(make_overlay(
            OverlayKind::Blob,
            true,
            tint,
            alpha,
            false,
            &mut rng,
        ));
        match recipe {
            // split: a hard region that flips the hero colours
            0..=29 => {
                let alpha = rng.range(0.68, 0.80);
                overlays.push(make_overlay(
                    flip_kind, true, flip_tint, alpha, true, &mut rng,
                ));
            }
            // duo: a second crisp geometric shape
            30..=44 => {
                let kind = duo_kinds[kind_seed % duo_kinds.len()];
                let alpha = rng.range(0.26, 0.40);
                overlays.push(make_overlay(kind, true, bold_tint, alpha, false, &mut rng));
            }
            // badge: a plate behind the hero
            45..=69 => {
                let alpha = rng.range(0.24, 0.34);
                overlays.push(make_overlay(
                    OverlayKind::Plate,
                    false,
                    helps,
                    alpha,
                    false,
                    &mut rng,
                ));
            }
            // classic: a soft glow
            _ => {
                let alpha = rng.range(0.10, 0.20);
                overlays.push(make_overlay(
                    OverlayKind::Spot,
                    false,
                    sheen,
                    alpha,
                    false,
                    &mut rng,
                ));
            }
        }
        let has_flip = overlays.iter().any(|o| o.flips);

        // ---- pattern
        let pattern =
            PATTERNS[(pick.below(PATTERNS.len()) + (vu % PATTERNS.len()) * 4) % PATTERNS.len()];
        let (ps, pc) = sin_cos(rng.range(0.0, PI));
        let (pcx, pcy) = if rng.chance(0.5) {
            (0.0, 0.0)
        } else {
            (rng.sign() * 0.9, rng.sign() * 0.9)
        };
        let pat = PatternParams {
            cos: pc,
            sin: ps,
            period: match pattern {
                PatternKind::Stripes => rng.range(0.28, 0.42),
                PatternKind::Dots => rng.range(0.18, 0.28),
                PatternKind::Rays => 1.0,
                PatternKind::Grid => rng.range(0.38, 0.54),
                PatternKind::Checker => rng.range(0.26, 0.42),
                PatternKind::Rings => rng.range(0.22, 0.34),
                PatternKind::Chevrons => rng.range(0.28, 0.42),
                PatternKind::Diamonds => rng.range(0.40, 0.56),
                PatternKind::Waves => rng.range(0.26, 0.38),
            },
            duty: match pattern {
                PatternKind::Grid | PatternKind::Diamonds => rng.range(0.05, 0.09),
                PatternKind::Dots => rng.range(0.55, 0.75),
                _ => rng.range(0.40, 0.55),
            },
            phase: rng.range(0.0, 1.0),
            cx: pcx,
            cy: pcy,
            extra: match pattern {
                PatternKind::Rays => (rng.below(5) * 2 + 8) as f32,
                PatternKind::Waves => rng.range(3.5, 6.0),
                _ => rng.range(0.7, 1.1),
            },
            amp: rng.range(0.05, 0.08),
            alpha: if light_glyph {
                rng.range(0.04, 0.08)
            } else {
                rng.range(0.035, 0.065)
            },
            color: if light_glyph { white } else { shade },
            clear: rng.range(0.28, 0.40),
            fade_dir: {
                let (s, c) = sin_cos(rng.range(0.0, TAU));
                [c, s]
            },
            fade_from: rng.range(-0.5, 0.1),
        };

        // ---- accent
        let accent =
            ACCENTS[(pick.below(ACCENTS.len()) + (vu % ACCENTS.len()) * 2) % ACCENTS.len()];
        let corner = [
            if rng.chance(0.5) { 1.0 } else { -1.0 },
            if rng.chance(0.85) { -1.0 } else { 1.0 },
        ];
        let acc = AccentParams {
            corner,
            size: rng.range(0.0, 1.0),
            angle: rng.range(0.0, TAU),
            gap: rng.range(0.35, 0.7),
            color: if light_glyph {
                oklch(0.96, 0.07, hue1 + 160.0)
            } else {
                oklch(0.30, 0.08, hue2)
            },
        };

        // ---- hero: the first letter or digit, else an emblem
        let emblem = (pick.below(EMBLEM_COUNT) + (vu % EMBLEM_COUNT) * 5) % EMBLEM_COUNT;
        let hero = title
            .chars()
            .next()
            .and_then(letter_def)
            .unwrap_or_else(|| emblem_def(emblem));
        let weight = [0.86, 1.0, 1.18][pick.below(3)];
        let hero_scale = rng.range(0.90, 1.08);
        let hero_off = [rng.range(-0.02, 0.02), rng.range(-0.02, 0.02)];

        let light_set = |top: Rgb, bot: Rgb| HeroColors {
            top,
            bot,
            line: oklch(0.24, 0.07, hue2),
            shadow: oklch(0.14, 0.05, hue2),
        };
        let dark_set = HeroColors {
            top: oklch(0.30, 0.06, hue1),
            bot: oklch(0.22, 0.05, hue2),
            line: oklch(0.98, 0.02, hue1),
            shadow: oklch(0.99, 0.01, hue1),
        };
        let light_fill = match pick.below(100) {
            0..=59 => light_set(oklch(0.99, 0.008, hue1), oklch(0.93, 0.03, hue2)),
            60..=79 => light_set(oklch(0.96, 0.08, 95.0), oklch(0.88, 0.12, 80.0)),
            _ => light_set(
                oklch(0.96, 0.05, hue1 + 170.0),
                oklch(0.90, 0.08, hue1 + 170.0),
            ),
        };
        let (hero_a, hero_b) = if light_glyph {
            (light_fill, dark_set)
        } else {
            (
                dark_set,
                light_set(oklch(0.99, 0.008, hue1), oklch(0.93, 0.03, hue2)),
            )
        };

        Self {
            lut,
            dir,
            dir_norm,
            coarse,
            fine,
            coarse_amp,
            fine_amp,
            overlays,
            has_flip,
            pattern,
            pat,
            accent,
            acc,
            hero,
            weight,
            hero_scale,
            hero_off,
            white,
            shade,
            hero_a,
            hero_b,
        }
    }

    /// Gradient colour at `g` in `0..=1`.
    fn lut_at(&self, g: f32) -> Rgb {
        let p = g * (LUT_N - 1) as f32;
        let i = (p as usize).min(LUT_N - 2);
        mix3(self.lut[i], self.lut[i + 1], p - i as f32)
    }

    /// Brightness factor from the two noise lattices at a tile-space point.
    fn noise_at(&self, tx: f32, ty: f32) -> f32 {
        1.0 + self.coarse_amp * lattice(&self.coarse, tx, ty)
            + self.fine_amp * lattice(&self.fine, tx, ty)
    }
}

// ---------------------------------------------------------------------------------------------
// Scene: a design laid out for one pixel size, and the per-sample shader.
// ---------------------------------------------------------------------------------------------

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn hypot(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

/// Coverage of a periodic stripe pattern: stripes of half width `half` centred in every `period`.
fn line_cov(u: f32, period: f32, half: f32, aa: f32) -> f32 {
    let d = (fract(u / period) - 0.5).abs() * period;
    sat(0.5 - (d - half) / aa)
}

/// Exact signed distance to a rhombus with half extents `(bx, by)` centred on the origin.
fn sd_rhombus(px: f32, py: f32, bx: f32, by: f32) -> f32 {
    let (px, py) = (px.abs(), py.abs());
    let h = (((bx - 2.0 * px) * bx - (by - 2.0 * py) * by) / (bx * bx + by * by)).clamp(-1.0, 1.0);
    let d = hypot(px - 0.5 * bx * (1.0 - h), py - 0.5 * by * (1.0 + h));
    if px * by + py * bx - bx * by > 0.0 {
        d
    } else {
        -d
    }
}

/// A circular arc with its centre, radius, mid direction and half aperture precomputed.
struct ArcC {
    cx: f32,
    cy: f32,
    r: f32,
    mx: f32,
    my: f32,
    cos_a: f32,
    sin_a: f32,
}

fn arc_c(cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> ArcC {
    let deg = PI / 180.0;
    let (my, mx) = sin_cos((a0 + a1) * 0.5 * deg);
    let (sin_a, cos_a) = sin_cos((a1 - a0) * 0.5 * deg);
    ArcC {
        cx,
        cy,
        r,
        mx,
        my,
        cos_a,
        sin_a,
    }
}

fn arc_dist(a: &ArcC, x: f32, y: f32) -> f32 {
    let (qx, qy) = (x - a.cx, y - a.cy);
    let lx = qx * a.mx + qy * a.my;
    let ly = (qy * a.mx - qx * a.my).abs();
    let rp = hypot(lx, ly);
    if lx >= rp * a.cos_a {
        (rp - a.r).abs()
    } else {
        hypot(lx - a.r * a.cos_a, ly - a.r * a.sin_a)
    }
}

/// The hero shape placed on the tile, all in normalised (0..1) image coordinates.
struct HeroScene {
    lines: Vec<[f32; 4]>,
    arcs: Vec<ArcC>,
    polys: Vec<(Vec<[f32; 2]>, f32)>,
    /// Stroke half width.
    hw: f32,
    /// Whether there are strokes at all (some emblems are polygons only).
    stroked: bool,
    /// Bounds of the centre lines and polygons: x min, y min, x max, y max.
    bbox: [f32; 4],
    /// Largest amount any part of the shape is grown by (stroke half width or polygon rounding).
    grow: f32,
}

impl HeroScene {
    /// Signed distance to the shape (negative inside). Points farther than `reach` from it get a
    /// cheap lower bound instead of the exact value.
    fn sd(&self, x: f32, y: f32, reach: f32) -> f32 {
        let dx = (self.bbox[0] - x).max(x - self.bbox[2]).max(0.0);
        let dy = (self.bbox[1] - y).max(y - self.bbox[3]).max(0.0);
        let far = hypot(dx, dy) - self.grow;
        if far > reach {
            return far;
        }
        let mut best = f32::MAX;
        for l in &self.lines {
            best = best.min(segment_dist(l[0], l[1], l[2], l[3], x, y));
        }
        for a in &self.arcs {
            best = best.min(arc_dist(a, x, y));
        }
        let mut sd = if self.stroked {
            best - self.hw
        } else {
            f32::MAX
        };
        for (pts, round) in &self.polys {
            sd = sd.min(poly_sd(pts, x, y) - round);
        }
        sd
    }
}

/// Lays the hero out on the tile for a given pixel size. Letters are fitted to the pixel grid at
/// small sizes (whole-pixel stroke width, even height, stems on pixel boundaries) so they stay crisp.
fn layout_hero(d: &Design, size: f32, tier: usize) -> HeroScene {
    let def = &d.hero;
    let stroked = !def.strokes.is_empty();
    let hw_units = if stroked { def.hw * d.weight } else { 0.0 };
    let outer_w = def.width + 2.0 * hw_units;
    let outer_h = 100.0 + 2.0 * hw_units;
    let target = if def.letter {
        [0.60, 0.55, 0.52][tier]
    } else {
        [0.62, 0.58, 0.55][tier]
    } * d.hero_scale;
    let max_w = if def.letter { 0.58 } else { 0.60 };
    let mut s = target / outer_h;
    if s * outer_w > max_w {
        s = max_w / outer_w;
    }
    let (cx, cy) = (0.5 + d.hero_off[0], 0.5 + d.hero_off[1]);
    let (mut sx, mut sy) = (s, s);
    let mut x0 = cx - s * def.width * 0.5;
    let mut y0 = cy - s * 50.0;
    let mut hw = hw_units * s;
    if def.letter && tier < 2 {
        let stroke = (2.0 * hw * size).round().max(2.0);
        let height = ((100.0 * s * size) / 2.0).round().max(3.0) * 2.0;
        let width = (def.width * s * size).round().max(1.0);
        let top = ((size - (height + stroke)) * 0.5 + d.hero_off[1] * size).round();
        let left = ((size - (width + stroke)) * 0.5 + d.hero_off[0] * size).round();
        y0 = (top + stroke * 0.5) / size;
        x0 = (left + stroke * 0.5) / size;
        sy = height / 100.0 / size;
        sx = width / def.width / size;
        hw = stroke * 0.5 / size;
    }
    let k = 0.5 * (sx + sy);
    let mut lines = Vec::new();
    let mut arcs = Vec::new();
    let mut polys = Vec::new();
    let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    let mut grow = if stroked { hw } else { 0.0 };
    let mut extend = |x: f32, y: f32, r: f32| {
        bbox[0] = bbox[0].min(x - r);
        bbox[1] = bbox[1].min(y - r);
        bbox[2] = bbox[2].max(x + r);
        bbox[3] = bbox[3].max(y + r);
    };
    for p in &def.strokes {
        match *p {
            Line(ax, ay, bx, by) => {
                let l = [x0 + ax * sx, y0 + ay * sy, x0 + bx * sx, y0 + by * sy];
                extend(l[0], l[1], 0.0);
                extend(l[2], l[3], 0.0);
                lines.push(l);
            }
            Arc(acx, acy, r, a0, a1) => {
                let (px, py) = (x0 + acx * sx, y0 + acy * sy);
                extend(px, py, r * k);
                arcs.push(arc_c(px, py, r * k, a0, a1));
            }
        }
    }
    for (pts, round) in &def.polys {
        let moved: Vec<[f32; 2]> = pts
            .iter()
            .map(|p| [x0 + p[0] * sx, y0 + p[1] * sy])
            .collect();
        for p in &moved {
            extend(p[0], p[1], 0.0);
        }
        grow = grow.max(round * k);
        polys.push((moved, round * k));
    }
    HeroScene {
        lines,
        arcs,
        polys,
        hw,
        stroked,
        bbox,
        grow,
    }
}

impl Overlay {
    /// Amount (0..=1) of this overlay at a tile-space point.
    fn mask(&self, tx: f32, ty: f32) -> f32 {
        let p = &self.p;
        match self.kind {
            OverlayKind::Half => {
                let s = tx * self.dir[0] + ty * self.dir[1] - p[0];
                smoothstep(-p[1], p[1], s)
            }
            OverlayKind::Disc | OverlayKind::Plate => {
                let d = hypot(tx - p[0], ty - p[1]) - p[2];
                smoothstep(-p[3], p[3], -d)
            }
            OverlayKind::Band => {
                let s = (tx * self.dir[0] + ty * self.dir[1] - p[0]).abs() - p[1];
                smoothstep(-p[2], p[2], -s)
            }
            OverlayKind::Spot => {
                let m = sat(1.0 - hypot(tx - p[0], ty - p[1]) / p[2]);
                m * m
            }
            OverlayKind::Wave => {
                let along = tx * self.dir[0] + ty * self.dir[1];
                let across = ty * self.dir[0] - tx * self.dir[1];
                let (s, _) = sin_cos(along * p[2] + p[3]);
                smoothstep(-p[4], p[4], across - (p[0] + p[1] * s))
            }
            OverlayKind::Ring => {
                let d = (hypot(tx - p[0], ty - p[1]) - p[2]).abs() - p[3];
                smoothstep(-p[4], p[4], -d)
            }
            OverlayKind::Blob => smoothstep(-p[1], p[1], lattice(&self.lat, tx, ty) - p[0]),
        }
    }
}

struct Scene<'a> {
    d: &'a Design,
    tier: usize,
    /// Samples per pixel edge.
    sq: usize,
    /// One sub-sample in normalised units: the anti-aliasing ramp width.
    aa: f32,
    /// The same in tile space.
    aa_t: f32,
    /// Tile half extent, normalised, and its inverse.
    a: f32,
    inv_a: f32,
    rim_w: f32,
    rim_gain: f32,
    hero: HeroScene,
    line_w: f32,
    /// Shadow offset x and y, blur width and opacity (opacity 0 means no shadow).
    shadow: [f32; 4],
    reach: f32,
    y_top: f32,
    y_span: f32,
}

impl<'a> Scene<'a> {
    fn new(d: &'a Design, size: u32) -> Self {
        let sizef = size as f32;
        let tier = if size <= 32 {
            0
        } else if size <= 64 {
            1
        } else {
            2
        };
        let sq = if size <= 64 { 4 } else { 3 };
        let margin = (if size <= 48 { 1.0 } else { 0.035 * sizef }).min(0.125 * sizef);
        let a = 0.5 - margin / sizef;
        let aa = 1.0 / (sizef * sq as f32);
        let hero = layout_hero(d, sizef, tier);
        let line_w = [0.6 / sizef, 0.9 / sizef, 0.011][tier];
        let shadow = match tier {
            0 => [0.0, 0.0, 1.0, 0.0],
            1 => [0.004, 0.010, 0.014, 0.35],
            _ => [0.006, 0.016, 0.030, 0.38],
        };
        let reach = line_w + shadow[2] + shadow[0].abs() + shadow[1].abs() + 4.0 * aa;
        let y_top = hero.bbox[1] - hero.grow;
        let y_span = (hero.bbox[3] - hero.bbox[1] + 2.0 * hero.grow).max(1e-4);
        Self {
            d,
            tier,
            sq,
            aa,
            aa_t: aa / a,
            a,
            inv_a: 1.0 / a,
            rim_w: (1.0 / sizef).max(0.012),
            rim_gain: [0.55, 0.85, 1.0][tier],
            hero,
            line_w,
            shadow,
            reach,
            y_top,
            y_span,
        }
    }

    /// Fine texture coverage at a tile-space point.
    fn pattern_mask(&self, tx: f32, ty: f32) -> f32 {
        let p = &self.d.pat;
        let aa = self.aa_t;
        let u = tx * p.cos + ty * p.sin;
        let v = ty * p.cos - tx * p.sin;
        let per = p.period;
        let half = p.duty * per * 0.5;
        match self.d.pattern {
            PatternKind::Stripes => line_cov(u + p.phase * per, per, half, aa),
            PatternKind::Dots => {
                let (fu, fv) = (u / per, v / per);
                let stagger = if (fv.floor() as i64) & 1 == 0 {
                    0.0
                } else {
                    0.5
                };
                let lu = (fract(fu + stagger) - 0.5) * per;
                let lv = (fract(fv) - 0.5) * per;
                sat(0.5 - (hypot(lu, lv) - per * 0.3 * p.duty) / aa)
            }
            PatternKind::Rays => {
                let (dx, dy) = (tx - p.cx, ty - p.cy);
                let r = hypot(dx, dy);
                let f = atan2(dy, dx) * (p.extra / TAU) + p.phase;
                let tri = (fract(f) - 0.5).abs();
                let arc = r * TAU / p.extra;
                sat((p.duty * 0.5 - tri) * arc / aa + 0.5)
            }
            PatternKind::Grid => line_cov(u, per, half, aa).max(line_cov(v, per, half, aa)),
            PatternKind::Checker => {
                let (fu, fv) = (u / per + p.phase, v / per);
                let cell = ((fu.floor() as i64) + (fv.floor() as i64)) & 1;
                let du = fract(fu).min(1.0 - fract(fu)) * per;
                let dv = fract(fv).min(1.0 - fract(fv)) * per;
                mix(0.5, cell as f32, sat(du.min(dv) / aa))
            }
            PatternKind::Rings => {
                let r = hypot(tx - p.cx, ty - p.cy);
                line_cov(r + p.phase * per, per, half, aa)
            }
            PatternKind::Chevrons => line_cov(v + u.abs() * p.extra + p.phase * per, per, half, aa),
            PatternKind::Diamonds => {
                let a = (u + v) * FRAC_1_SQRT_2;
                let b = (u - v) * FRAC_1_SQRT_2;
                let step = per * FRAC_1_SQRT_2 * 1.4;
                line_cov(a + p.phase * step, step, half, aa).max(line_cov(b, step, half, aa))
            }
            PatternKind::Waves => {
                let (s, _) = sin_cos(u * p.extra * TAU);
                line_cov(v + p.amp * s + p.phase * per, per, half, aa)
            }
        }
    }

    /// Accent feature blended onto `col` at a tile-space point.
    fn accent(&self, col: Rgb, tx: f32, ty: f32, depth: f32) -> Rgb {
        let d = self.d;
        let acc = &d.acc;
        let aa = self.aa_t;
        let cover = |sd: f32| sat(0.5 - sd / aa);
        match d.accent {
            AccentKind::Fold => {
                let s = tx * acc.corner[0] + ty * acc.corner[1];
                let s0 = 1.06 + 0.2 * acc.size;
                let flap = sat(0.5 + (s - s0) / aa);
                let under = sat((s - (s0 - 0.18)) / 0.18) * (1.0 - flap);
                let edge = cover(((s - s0).abs() - 0.014) * FRAC_1_SQRT_2);
                let mut c = mix3(col, d.shade, 0.24 * under);
                c = mix3(c, d.white, 0.34 * flap);
                mix3(c, d.white, 0.55 * edge)
            }
            AccentKind::Sparkle => {
                let r = 0.20 + 0.10 * acc.size;
                let mut c = col;
                let spot = |c: Rgb, cx: f32, cy: f32, r: f32| {
                    let (px, py) = (tx - cx, ty - cy);
                    let sd = sd_rhombus(px, py, r, r * 0.26).min(sd_rhombus(px, py, r * 0.26, r));
                    let mut c = c;
                    if self.tier >= 2 {
                        let g = sat(1.0 - hypot(px, py) / (r * 2.4));
                        c = mix3(c, d.white, 0.22 * g * g);
                    }
                    mix3(c, acc.color, 0.95 * cover(sd))
                };
                c = spot(c, acc.corner[0] * 0.64, acc.corner[1] * 0.64, r);
                spot(
                    c,
                    acc.corner[0] * 0.64 - acc.corner[0] * 0.30,
                    acc.corner[1] * 0.64 + acc.corner[1] * 0.28,
                    r * 0.4,
                )
            }
            AccentKind::Orbit => {
                let rr = 0.78 + 0.08 * acc.size;
                let r = hypot(tx, ty);
                let mut delta = atan2(ty, tx) - acc.angle;
                if delta > PI {
                    delta -= TAU;
                } else if delta < -PI {
                    delta += TAU;
                }
                let open = smoothstep(acc.gap * 0.5, acc.gap * 0.5 + 0.12, delta.abs());
                let ring = cover((r - rr).abs() - 0.012) * open;
                let (sa, ca) = sin_cos(acc.angle);
                let dot = cover(hypot(tx - rr * ca, ty - rr * sa) - 0.055);
                let c = mix3(col, acc.color, 0.55 * ring);
                mix3(c, acc.color, 0.95 * dot)
            }
            AccentKind::Gloss => {
                let s = (tx + ty) * -FRAC_1_SQRT_2;
                let soft = smoothstep(0.1, 1.1, s);
                let crisp = sat(0.5 + (s - (0.42 + 0.18 * acc.size)) / aa);
                let c = mix3(col, d.white, 0.10 * soft);
                mix3(c, d.white, 0.10 * crisp)
            }
            AccentKind::Keyline => {
                let inset = 0.035 + 0.025 * acc.size;
                let half = 0.004 + 0.003 * acc.size;
                let m = sat(0.5 - ((depth - inset).abs() - half) / self.aa);
                mix3(col, acc.color, 0.6 * m)
            }
            AccentKind::Dots => {
                let horizontal = acc.angle < PI;
                let mut c = col;
                for i in 0..3 {
                    let off = i as f32 * 0.17;
                    let (cx, cy) = if horizontal {
                        (acc.corner[0] * (0.52 - off), acc.corner[1] * 0.80)
                    } else {
                        (acc.corner[0] * 0.80, acc.corner[1] * (0.52 - off))
                    };
                    let dot = cover(hypot(tx - cx, ty - cy) - 0.05);
                    c = mix3(c, acc.color, 0.9 * dot);
                }
                c
            }
        }
    }

    /// Colour of the tile itself (no hero) at a tile-space point, `depth` inside its edge.
    fn tile_color(&self, tx: f32, ty: f32, depth: f32) -> Rgb {
        let d = self.d;
        let g = sat(0.5 + (tx * d.dir[0] + ty * d.dir[1]) * d.dir_norm);
        let base = d.lut_at(g);
        let k = d.noise_at(tx, ty);
        let mut col = [base[0] * k, base[1] * k, base[2] * k];
        for o in &d.overlays {
            let m = o.mask(tx, ty);
            if m > 0.0 {
                col = mix3(col, o.tint, o.alpha * m);
            }
        }
        if self.tier >= 1 {
            let side = smoothstep(
                d.pat.fade_from,
                d.pat.fade_from + 0.9,
                tx * d.pat.fade_dir[0] + ty * d.pat.fade_dir[1],
            );
            let fade = side * smoothstep(d.pat.clear, d.pat.clear + 0.45, hypot(tx, ty));
            if fade > 0.0 {
                let m = self.pattern_mask(tx, ty);
                col = mix3(col, d.pat.color, d.pat.alpha * m * fade);
            }
            col = self.accent(col, tx, ty, depth);
        }
        if self.tier >= 2 {
            col = mix3(col, d.white, 0.10 * sat(1.0 - (ty + 1.0) * 1.1));
            col = mix3(col, d.shade, 0.12 * sat((ty - 0.55) / 0.45));
        }
        // rim: a light edge where the tile faces the light (top left), a dark one opposite
        if depth < self.rim_w {
            let (nx, ny) = (tx * tx * tx, ty * ty * ty);
            let nl = hypot(nx, ny).max(1e-6);
            let lit = (-(0.6 * nx + 0.8 * ny) / nl) * 0.5 + 0.5;
            let edge = 1.0 - smoothstep(0.0, self.rim_w, depth);
            col = mix3(col, d.white, edge * (0.05 + 0.32 * lit) * self.rim_gain);
            col = mix3(col, d.shade, edge * 0.30 * (1.0 - lit) * self.rim_gain);
        }
        col
    }

    /// Draws the hero (shadow, outline, fill, highlight) over `col`.
    fn hero_color(&self, col: Rgb, x: f32, y: f32) -> Rgb {
        let d = self.d;
        let h = &self.hero;
        let sd = h.sd(x, y, self.reach);
        if sd > self.reach {
            return col;
        }
        let flip = if d.has_flip && d.hero.letter {
            let (tx, ty) = ((x - 0.5) * self.inv_a, (y - 0.5) * self.inv_a);
            d.overlays
                .iter()
                .filter(|o| o.flips)
                .fold(0.0, |m, o| o.mask(tx, ty).max(m))
        } else {
            0.0
        };
        let (a, b) = (&d.hero_a, &d.hero_b);
        let pick = |p: Rgb, q: Rgb| if flip > 0.0 { mix3(p, q, flip) } else { p };
        let mut col = col;
        let mut lit = 0.0;
        if self.shadow[3] > 0.0 {
            let ds = h.sd(x - self.shadow[0], y - self.shadow[1], self.reach);
            let s = self.shadow[3] * sat(0.5 - ds / self.shadow[2]);
            col = mix3(col, pick(a.shadow, b.shadow), s);
            if self.tier >= 2 {
                lit = smoothstep(-0.3 * self.shadow[1], 0.5 * self.shadow[1], ds);
            }
        }
        let line = sat(0.5 - (sd - self.line_w) / self.aa);
        col = mix3(col, pick(a.line, b.line), 0.6 * line);
        let fill = sat(0.5 - sd / self.aa);
        if fill > 0.0 {
            let t = sat((y - self.y_top) / self.y_span);
            let mut c = mix3(pick(a.top, b.top), pick(a.bot, b.bot), t);
            if lit > 0.0 {
                c = mix3(c, [1.0, 1.0, 1.0], 0.35 * lit);
            }
            col = mix3(col, c, fill);
        }
        col
    }

    /// Premultiplied RGBA of the icon at a normalised image point.
    fn shade(&self, x: f32, y: f32) -> [f32; 4] {
        let (tx, ty) = ((x - 0.5) * self.inv_a, (y - 0.5) * self.inv_a);
        let (x2, y2) = (tx * tx, ty * ty);
        let f = (x2 * x2 + y2 * y2).sqrt().sqrt();
        let mut depth = 1.0e3;
        if f > 0.6 {
            let grad = ((x2 * x2 * x2 + y2 * y2 * y2).sqrt() / (f * f * f)).max(0.5);
            depth = (1.0 - f) * self.a / grad;
        }
        let alpha = sat(0.5 + depth / self.aa);
        if alpha <= 0.0 {
            return [0.0; 4];
        }
        let col = self.tile_color(tx, ty, depth);
        let col = self.hero_color(col, x, y);
        [
            sat(col[0]) * alpha,
            sat(col[1]) * alpha,
            sat(col[2]) * alpha,
            alpha,
        ]
    }
}

fn to_byte(v: f32) -> u8 {
    (sat(v) * 255.0 + 0.5) as u8
}

/// Draws `design` at `size` pixels, super-sampled (4x4 up to 64 px, 3x3 above).
fn render_design(design: &Design, size: u32) -> Vec<u8> {
    let scene = Scene::new(design, size);
    let s = size as usize;
    let sq = scene.sq;
    let step = 1.0 / (size as f32 * sq as f32);
    let inv = 1.0 / (sq * sq) as f32;
    let mut out = vec![0u8; s * s * 4];
    for (i, px) in out.chunks_exact_mut(4).enumerate() {
        let (ix, iy) = ((i % s) as f32, (i / s) as f32);
        let mut acc = [0.0f32; 4];
        for sy in 0..sq {
            let y = (iy * sq as f32 + sy as f32 + 0.5) * step;
            for sx in 0..sq {
                let x = (ix * sq as f32 + sx as f32 + 0.5) * step;
                let c = scene.shade(x, y);
                acc[0] += c[0];
                acc[1] += c[1];
                acc[2] += c[2];
                acc[3] += c[3];
            }
        }
        let a = acc[3] * inv;
        let alpha = to_byte(a);
        if alpha > 0 {
            let k = inv / a;
            px[0] = to_byte(acc[0] * k);
            px[1] = to_byte(acc[1] * k);
            px[2] = to_byte(acc[2] * k);
            px[3] = alpha;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
            .collect()
    }

    /// Deterministic byte soup for property-style tests.
    fn lcg_bytes(seed: u64, n: usize, modulus: u8) -> Vec<u8> {
        let mut rng = Rng::new(seed);
        (0..n)
            .map(|_| (rng.next_u64() >> 33) as u8 % modulus)
            .collect()
    }

    /// A 13x10 RGBA PNG written by Python (zlib level 9, filter type cycling 0..4 per row).
    const PYTHON_RGBA_PNG: &str = "89504e470d0a1a0a0000000d494844520000000d0000000a08060000006feed4c4000001024944415478da9dd0214803611887f1679eb870dc091b030f2e8803850bc284c1446141380cda0483a04db04c3459deb06011db440c8b0e8330e3c020ab170cae5a0cab26715fd43f625599e1c7975e9eef7d013e8a309a87d765186ec0f31e0c8e213b85fe15f46ea1fb009d27680fa195f3224645f26e1c131a725e9497504a124b5912a9484dea92caa66c396f6a89861f84ce0f0a529248629995b22c48228b5291aa9bfc2a29e9a1122a11ff8969633067e5ac6a697fdd0e7a3b76de6dd85da76983f685bdb76e6ce6ecde569a8fb67bf262cdc337bbdecf152ec98a246e1cff3b4478c4f66f4bfb414d56a52e6b92fe740895f40d0f9550099548bfdfbafb04a9d4a416a05d18bb0000000049454e44ae426082";
    /// The same picture as an RGB PNG.
    const PYTHON_RGB_PNG: &str = "89504e470d0a1a0a0000000d494844520000000d0000000a0802000000e08c4393000000df4944415478da8d8fa14a04011445cf7ac50d8f37c2ca820f3688030a138411165614260883419b4dd06c11b4bfb0c16614f1030483c16ef00f0cfa0506ff40ecae5695150e375d2ef7002cc00a6cc02e1cc1199cc335dcc1233cc31b7414935e772a33939ea2ab28147dc540512a2a45ad18291a45abd853ec6b6e1df3c2bc67de370ff381f9927969be6a5e99af99d7e6c3d9af3dbaa2107d31f80be693e52c87d9eee4f1415e9ce4fd385f2ef3e336171f72f3290f5f73fc9e379dded5c4a39acabf3d8a537e3d6e3e32df326fccb7cddb9f1ea5a8442d46a211ed77369faca22ef50ecfc0d40000000049454e44ae426082";

    fn python_picture(alpha: bool) -> Vec<u8> {
        let mut px = Vec::new();
        for y in 0..10usize {
            for x in 0..13usize {
                px.push(((x * 19 + y * 3) % 256) as u8);
                px.push(((y * 25) % 256) as u8);
                px.push(((x * y * 7) % 256) as u8);
                if alpha {
                    px.push((255 - (x + y) as i32 * 9).rem_euclid(256) as u8);
                }
            }
        }
        px
    }

    #[test]
    fn checksums_match_reference_values() {
        assert_eq!(crc32(&[b"123456789"]), 0xcbf4_3926);
        assert_eq!(crc32(&[b"1234", b"56789"]), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn inflate_reads_real_zlib_streams() {
        let text = [
            b"the quick brown fox jumps over the lazy dog. ".repeat(6),
            b"pack my box with five dozen liquor jugs. ".repeat(3),
        ]
        .concat();
        let cases: [(&str, Vec<u8>); 4] = [
            (
                "7801011200edff68656c6c6f2073746f72656420776f726c6442da070e",
                b"hello stored world".to_vec(),
            ),
            ("789c4b4c4a4e444500417c06e5", b"abcabcabcabcabcabc".to_vec()),
            (
                "78daddcbc11180201043d15652813d8182ae020b082254ef8e3d78f198f92f65334895e6033a730bb07c63af3e9ee0cb6414c94e8d8e85d7e95dbfc65189f31d5a50a3b2c1d265240d13e02855cef25dcf2fe003b9998e30",
                text,
            ),
            ("789c030000000001", Vec::new()),
        ];
        for (stream, expected) in cases {
            let z = hex(stream);
            let (data, used) = inflate(&z[2..], expected.len()).unwrap();
            assert_eq!(data, expected);
            let tail = &z[2 + used..];
            assert_eq!(tail, adler32(&expected).to_be_bytes());
            // one byte too little room must be refused, never overrun
            if !expected.is_empty() {
                assert!(inflate(&z[2..], expected.len() - 1).is_err());
            }
        }
    }

    #[test]
    fn decode_png_reads_files_written_by_python() {
        let (w, h, rgba) = decode_png(&hex(PYTHON_RGBA_PNG)).unwrap();
        assert_eq!((w, h), (13, 10));
        assert_eq!(rgba, python_picture(true));
        let (w, h, rgba) = decode_png(&hex(PYTHON_RGB_PNG)).unwrap();
        assert_eq!((w, h), (13, 10));
        let rgb = python_picture(false);
        let expected: Vec<u8> = rgb
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        assert_eq!(rgba, expected);
    }

    #[test]
    fn deflate_round_trips_every_kind_of_data() {
        let mut inputs: Vec<Vec<u8>> = vec![Vec::new(), vec![7], vec![1, 2], vec![9, 9, 9]];
        inputs.push(vec![0u8; 100_000]);
        inputs.push(vec![0xabu8; MAX_MATCH + 1]);
        inputs.push(lcg_bytes(1, 5_000, 255)); // incompressible
        inputs.push(lcg_bytes(2, 60_000, 4)); // skewed alphabet
        inputs.push(lcg_bytes(3, 70_000, 3));
        let mut repeated = lcg_bytes(4, 40_000, 200);
        let copy = repeated[..30_000].to_vec();
        repeated.extend_from_slice(&copy); // a match at distance close to the 32 KiB window
        inputs.push(repeated);
        inputs.push(
            (0..50_000u32)
                .map(|i| ((i * i) >> 7) as u8 ^ (i % 13) as u8)
                .collect(),
        );
        for data in inputs {
            let z = zlib_compress(&data);
            let (back, used) = inflate(&z[2..z.len() - 4], data.len() + 1).unwrap();
            assert_eq!(back, data, "raw deflate of {} bytes", data.len());
            assert_eq!(used, z.len() - 6);
            assert_eq!(&z[z.len() - 4..], adler32(&data).to_be_bytes());
        }
    }

    #[test]
    fn deflate_really_compresses() {
        let flat = zlib_compress(&vec![0u8; 100_000]);
        assert!(flat.len() < 200, "flat data took {} bytes", flat.len());
        let text = b"a moderately repetitive line of text, again and again. ".repeat(400);
        let z = zlib_compress(&text);
        assert!(z.len() * 20 < text.len(), "text took {} bytes", z.len());
        // skewed literals must benefit from dynamic Huffman codes (fixed ones cost >= 8 bits each)
        let skewed = lcg_bytes(9, 40_000, 3);
        let z = zlib_compress(&skewed);
        assert!(
            z.len() * 8 < skewed.len() * 6,
            "skewed data took {} bytes",
            z.len()
        );
    }

    #[test]
    fn png_round_trips_images() {
        let gradient = |w: usize, h: usize| -> Vec<u8> {
            let mut v = Vec::new();
            for y in 0..h {
                for x in 0..w {
                    v.extend_from_slice(&[
                        (x * 255 / w.max(1)) as u8,
                        (y * 255 / h.max(1)) as u8,
                        ((x + y) % 256) as u8,
                        255 - (x / 2) as u8,
                    ]);
                }
            }
            v
        };
        let mut cases: Vec<(usize, usize, Vec<u8>)> = vec![
            (1, 1, vec![1, 2, 3, 4]),
            (1, 7, gradient(1, 7)),
            (7, 1, gradient(7, 1)),
            (255, 3, gradient(255, 3)),
            (64, 64, gradient(64, 64)),
            (37, 41, lcg_bytes(5, 37 * 41 * 4, 255)),
            (20, 20, vec![0; 20 * 20 * 4]),
            (33, 9, [255u8, 0, 0, 255].repeat(33 * 9)),
        ];
        cases.push((300, 40, lcg_bytes(6, 300 * 40 * 4, 3)));
        for (w, h, rgba) in cases {
            let file = png(&rgba, w as u32, h as u32);
            assert_eq!(&file[..8], &PNG_SIGNATURE);
            let (dw, dh, back) = decode_png(&file).unwrap();
            assert_eq!((dw as usize, dh as usize), (w, h));
            assert_eq!(back, rgba, "{w}x{h} did not round-trip");
        }
        let flat = png(&vec![9u8; 256 * 256 * 4], 256, 256);
        assert!(
            flat.len() < 1200,
            "flat 256x256 PNG is {} bytes",
            flat.len()
        );
    }

    #[test]
    fn png_tolerates_odd_arguments() {
        assert!(png(&[], 0, 5).is_empty());
        assert!(png(&[], 5, 0).is_empty());
        assert!(png(&[1, 2, 3, 4], u32::MAX, u32::MAX).is_empty());
        assert!(png(&[1, 2, 3, 4], 1 << 31, 1).is_empty());
        // too little data counts as transparent black, extra data is ignored
        let (w, h, px) = decode_png(&png(&[9, 8, 7, 6], 2, 2)).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(px, [9, 8, 7, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let (_, _, px) = decode_png(&png(&[1, 2, 3, 4, 5, 6, 7, 8, 99, 99], 1, 2)).unwrap();
        assert_eq!(px, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn decode_png_survives_damage() {
        let img = lcg_bytes(8, 30 * 30 * 4, 255);
        let file = png(&img, 30, 30);
        for cut in (0..file.len()).step_by(7) {
            assert!(
                decode_png(&file[..cut]).is_err(),
                "prefix of {cut} bytes accepted"
            );
        }
        let mut rng = Rng::new(11);
        for _ in 0..300 {
            let mut bad = file.clone();
            let at = rng.below(bad.len());
            bad[at] ^= 1 << rng.below(8);
            // a flipped bit must be caught by a CRC, the zlib header, Adler-32 or the inflater
            assert!(decode_png(&bad).is_err(), "bit flip at {at} accepted");
        }
        for _ in 0..200 {
            let junk = lcg_bytes(rng.next_u64(), rng.below(200), 255);
            let _ = decode_png(&junk);
        }
        assert!(decode_png(&[]).is_err());
        assert!(decode_png(&PNG_SIGNATURE).is_err());
    }

    #[test]
    fn parse_ico_accepts_what_pack_ico_writes_and_rejects_damage() {
        let small = bmp_frame(&[10u8; 16 * 16 * 4], 16);
        let big = png(&[20u8; 64 * 64 * 4], 64, 64);
        let file = pack_ico(&[(16, small.clone()), (64, big.clone())]);
        let frames = parse_ico(&file).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(
            (frames[0].width, frames[0].height, frames[0].bpp),
            (16, 16, 32)
        );
        assert!(!frames[0].png && frames[1].png);
        assert_eq!(frames[0].len as usize, small.len());
        assert_eq!(frames[1].offset as usize, 6 + 32 + small.len());
        assert_eq!(frames[1].len as usize, big.len());
        let huge = pack_ico(&[(256, png(&[1u8; 256 * 256 * 4], 256, 256))]);
        assert_eq!(parse_ico(&huge).unwrap()[0].width, 256);
        assert_eq!(huge[6], 0, "256 is stored as 0");

        for cut in 0..file.len() {
            assert!(parse_ico(&file[..cut]).is_err(), "prefix {cut} accepted");
        }
        let mut rng = Rng::new(21);
        for _ in 0..500 {
            let mut bad = file.clone();
            let at = rng.below(6 + 32);
            bad[at] = (rng.next_u64() >> 8) as u8;
            let _ = parse_ico(&bad); // must not panic whatever it decides
        }
        let mut wrong = file.clone();
        wrong[2] = 2; // cursor, not icon
        assert!(parse_ico(&wrong).is_err());
        let mut wrong = file.clone();
        wrong[6] = 17; // directory width disagrees with the DIB
        assert!(parse_ico(&wrong).is_err());
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "icon_tests_{}_{}_{}",
            name,
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn listing(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| {
                        (
                            e.file_name().to_string_lossy().into_owned(),
                            fs::read(e.path()).unwrap_or_default(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn commit_files_is_all_or_nothing() {
        let names = ["a.bin", "b.bin", "c.bin"];
        let contents = vec![vec![1u8; 10], vec![2u8; 20], vec![3u8; 30]];
        for replace in [false, true] {
            // count the steps of a clean run first
            let dir = scratch_dir("count");
            let mut steps = 0;
            commit_files(&dir, &names, &contents, replace, &mut |_| {
                steps += 1;
                Ok(())
            })
            .unwrap();
            assert_eq!(listing(&dir).len(), 3);
            fs::remove_dir_all(&dir).unwrap();
            assert!(steps >= 6);

            for fail_at in 0..steps {
                // directory that does not exist yet
                let dir = scratch_dir("fresh");
                let r = commit_files(&dir, &names, &contents, replace, &mut |n| {
                    if n == fail_at {
                        Err(std::io::Error::other("injected"))
                    } else {
                        Ok(())
                    }
                });
                assert!(r.is_err(), "step {fail_at} should have failed");
                assert!(
                    !dir.exists(),
                    "fresh dir left behind after failure at {fail_at}"
                );

                // directory holding files we must not lose (only possible when replacing)
                if replace {
                    let dir = scratch_dir("existing");
                    fs::create_dir_all(&dir).unwrap();
                    fs::write(dir.join("b.bin"), b"old b").unwrap();
                    fs::write(dir.join("unrelated.txt"), b"keep").unwrap();
                    let before = listing(&dir);
                    let r = commit_files(&dir, &names, &contents, true, &mut |n| {
                        if n == fail_at {
                            Err(std::io::Error::other("injected"))
                        } else {
                            Ok(())
                        }
                    });
                    assert!(r.is_err());
                    assert_eq!(
                        listing(&dir),
                        before,
                        "state changed after failure at {fail_at}"
                    );
                    fs::remove_dir_all(&dir).unwrap();
                }
            }
        }
    }

    #[test]
    fn commit_files_refuses_to_overwrite_and_replaces_on_request() {
        let names = ["a.bin", "b.bin"];
        let dir = scratch_dir("refuse");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("b.bin"), b"precious").unwrap();
        let err =
            commit_files(&dir, &names, &[vec![1], vec![2]], false, &mut |_| Ok(())).unwrap_err();
        assert!(err.contains("b.bin"), "{err}");
        assert_eq!(
            listing(&dir),
            vec![("b.bin".to_string(), b"precious".to_vec())]
        );
        let paths = commit_files(&dir, &names, &[vec![1], vec![2]], true, &mut |_| Ok(())).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(
            listing(&dir),
            vec![
                ("a.bin".to_string(), vec![1]),
                ("b.bin".to_string(), vec![2])
            ]
        );
        // a directory in the way is refused even when replacing
        fs::remove_file(dir.join("a.bin")).unwrap();
        fs::create_dir(dir.join("a.bin")).unwrap();
        let err =
            commit_files(&dir, &names, &[vec![1], vec![2]], true, &mut |_| Ok(())).unwrap_err();
        assert!(err.contains("a.bin"), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }

    // ------------------------------------------------------------------------------------------
    // Art, rendering and file output.
    // ------------------------------------------------------------------------------------------

    use std::sync::OnceLock;

    /// Two hundred distinct game-like titles with a mix of initials.
    fn sample_titles() -> Vec<String> {
        const ADJECTIVES: [&str; 20] = [
            "Neon", "Crimson", "Silent", "Hyper", "Lunar", "Rusty", "Turbo", "Pixel", "Golden",
            "Frozen", "Wild", "Atomic", "Cosmic", "Iron", "Velvet", "Shadow", "Solar", "Mighty",
            "Tiny", "Electric",
        ];
        const NOUNS: [&str; 20] = [
            "Relay", "Quest", "Rush", "Tide", "Runner", "Forge", "Arena", "Garden", "Voyage",
            "Legends", "Orbit", "Heist", "Tactics", "Siege", "Racer", "Drift", "Valley", "Blitz",
            "Empire", "Dash",
        ];
        (0..200)
            .map(|i| {
                let k = (i * 7 + 3) % 400;
                format!("{} {}", ADJECTIVES[k / 20], NOUNS[k % 20])
            })
            .collect()
    }

    /// All ten frames of the "Bouncer" icon, rendered once for the tests that need them.
    fn bouncer_frames() -> &'static Vec<RenderedFrame> {
        static FRAMES: OnceLock<Vec<RenderedFrame>> = OnceLock::new();
        FRAMES.get_or_init(|| render_all(&IconSpec::new("Bouncer")))
    }

    fn bouncer_ico() -> Vec<u8> {
        let packed: Vec<(u32, Vec<u8>)> = bouncer_frames()
            .iter()
            .map(|f| (f.0, f.2.clone()))
            .collect();
        pack_ico(&packed)
    }

    fn pixel(rgba: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * size + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
    }

    /// Mean absolute channel difference over the pixels that are opaque in both renders.
    fn mean_diff(a: &[u8], b: &[u8]) -> f32 {
        let (mut sum, mut n) = (0u64, 0u64);
        for (p, q) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
            if p[3] == 255 && q[3] == 255 {
                for c in 0..3 {
                    sum += u64::from(p[c].abs_diff(q[c]));
                }
                n += 3;
            }
        }
        sum as f32 / n.max(1) as f32
    }

    #[test]
    fn render_is_deterministic_and_normalises_the_title() {
        let spec = IconSpec::new("Bouncer");
        for size in [16, 20, 32, 64] {
            assert_eq!(render(&spec, size), render(&spec, size));
        }
        assert_eq!(render(&IconSpec::new("  bOUNCER\t"), 32), render(&spec, 32));
        assert_eq!(
            render(&IconSpec::new("Bouncer").with_variant(0), 32),
            render(&spec, 32)
        );
        assert_ne!(render(&IconSpec::new("Bouncers"), 32), render(&spec, 32));
        assert_eq!(IconSpec::new("x").with_variant(3).variant, 3);
        // the .ico bytes and the set written to disk are deterministic too
        assert_eq!(ico(&spec), bouncer_ico());
    }

    /// `(title, variant, size, FNV-1a of the RGBA render)`: covers a letter, an emblem, the empty
    /// title, a variant and all three detail tiers.
    const GOLDEN: [(&str, u32, u32, u64); 6] = [
        ("Bouncer", 0, 16, 0x187a_7965_8e5e_2124),
        ("Bouncer", 0, 32, 0x7f15_b8d1_9a52_2ca5),
        ("Neon Relay", 2, 64, 0x2af1_9921_ce6f_d45a),
        ("\u{1F680} Star Lander", 1, 48, 0xf144_7244_b6f1_b2aa),
        ("", 0, 96, 0xb5e0_ee11_86b3_4b13),
        ("1942", 3, 128, 0x0e63_630b_434b_7627),
    ];

    #[test]
    fn pixels_are_pinned_by_golden_hashes() {
        // If the artwork is changed on purpose, run this test to see the new hashes and update the
        // table: games whose icon is regenerated afterwards would look different.
        let actual: Vec<u64> = GOLDEN
            .iter()
            .map(|&(title, variant, size, _)| {
                fnv1a64(&render(&IconSpec::new(title).with_variant(variant), size))
            })
            .collect();
        for (&(title, variant, size, _), hash) in GOLDEN.iter().zip(&actual) {
            eprintln!("(\"{title}\", {variant}, {size}, {hash:#018x}),");
        }
        for (&(title, variant, size, want), &got) in GOLDEN.iter().zip(&actual) {
            assert_eq!(got, want, "{title:?} variant {variant} at {size}px changed");
        }
    }

    #[test]
    fn render_sizes_lengths_and_transparent_corners() {
        let spec = IconSpec::new("Zed");
        assert!(render(&spec, 0).is_empty());
        assert_eq!(clamp_size(u32::MAX), MAX_RENDER_SIZE);
        assert_eq!(clamp_size(MAX_RENDER_SIZE), MAX_RENDER_SIZE);
        assert_eq!(clamp_size(24), 24);
        for size in [
            1usize, 2, 3, 4, 5, 7, 8, 15, 16, 17, 20, 24, 31, 32, 33, 40, 48, 63, 64, 65, 96,
        ] {
            let px = render(&spec, size as u32);
            assert_eq!(px.len(), size * size * 4, "size {size}");
            if size >= 8 {
                assert_eq!(
                    pixel(&px, size, 0, 0)[3],
                    0,
                    "size {size}: corner must be transparent"
                );
                assert_eq!(pixel(&px, size, size - 1, 0)[3], 0);
                assert_eq!(pixel(&px, size, 0, size - 1)[3], 0);
                assert_eq!(pixel(&px, size, size - 1, size - 1)[3], 0);
                assert_eq!(
                    pixel(&px, size, size / 2, size / 2)[3],
                    255,
                    "size {size}: centre"
                );
            }
            // transparent pixels carry no colour, and nothing is fully invisible garbage
            for p in px.chunks_exact(4) {
                if p[3] == 0 {
                    assert_eq!(&p[..3], &[0, 0, 0]);
                }
            }
        }
        for (size, rgba, _) in bouncer_frames() {
            assert_eq!(rgba.len(), (*size as usize).pow(2) * 4);
        }
    }

    #[test]
    fn frames_are_valid_and_match_the_renders() {
        for (size, rgba, payload) in bouncer_frames() {
            let s = *size as usize;
            if *size >= 64 {
                let (w, h, back) = decode_png(payload).unwrap();
                assert_eq!((w, h), (*size, *size));
                assert_eq!(&back, rgba, "{size}px PNG frame differs from the render");
            } else {
                // a 32-bit DIB: header, bottom-up BGRA rows, then the padded 1-bit AND mask
                assert_eq!(le32(payload, 0), 40);
                assert_eq!(le32(payload, 4), *size);
                assert_eq!(le32(payload, 8), size * 2);
                assert_eq!((le16(payload, 12), le16(payload, 14)), (1, 32));
                assert_eq!(le32(payload, 16), 0, "BI_RGB");
                let mask_row = s.div_ceil(32) * 4;
                assert_eq!(payload.len(), 40 + s * s * 4 + mask_row * s);
                assert_eq!(le32(payload, 20) as usize, s * s * 4 + mask_row * s);
                for y in 0..s {
                    for x in 0..s {
                        let at = 40 + ((s - 1 - y) * s + x) * 4;
                        let want = pixel(rgba, s, x, y);
                        assert_eq!(
                            [
                                payload[at + 2],
                                payload[at + 1],
                                payload[at],
                                payload[at + 3]
                            ],
                            want,
                            "{size}px DIB pixel ({x},{y})"
                        );
                        let bits = &payload[40 + s * s * 4 + (s - 1 - y) * mask_row..];
                        let masked = bits[x / 8] & (0x80 >> (x % 8)) != 0;
                        assert_eq!(masked, want[3] == 0, "{size}px AND mask ({x},{y})");
                    }
                }
            }
        }
    }

    #[test]
    fn ico_has_ten_sizes_valid_layout_and_stays_small() {
        let file = bouncer_ico();
        let frames = parse_ico(&file).unwrap();
        assert_eq!(frames.len(), ICO_SIZES.len());
        let mut next = 6 + 16 * frames.len();
        for (frame, &size) in frames.iter().zip(&ICO_SIZES) {
            assert_eq!((frame.width, frame.height, frame.bpp), (size, size, 32));
            assert_eq!(frame.png, size >= 64, "{size}px frame kind");
            assert_eq!(
                frame.offset as usize, next,
                "frames are packed back to back"
            );
            next += frame.len as usize;
        }
        assert_eq!(next, file.len());
        assert_eq!(file[6], 16);
        assert_eq!(file[6 + 16 * 9], 0, "256 is stored as 0");
        assert!(file.len() < 120_000, ".ico is {} bytes", file.len());
        eprintln!("Bouncer .ico is {} bytes", file.len());
    }

    #[test]
    fn window_icon_blobs_are_the_small_renders() {
        let spec = IconSpec::new("Bouncer");
        let (a, b, c) = window_icon_blobs(&spec);
        let frames = bouncer_frames();
        let raw = |size: u32| &frames.iter().find(|f| f.0 == size).unwrap().1;
        assert_eq!(&a[..], &raw(16)[..]);
        assert_eq!(&b[..], &raw(32)[..]);
        assert_eq!(&c[..], &raw(64)[..]);
    }

    #[test]
    fn write_icon_set_writes_five_files_and_never_overwrites_by_accident() {
        let spec = IconSpec::new("Bouncer");
        let dir = scratch_dir("set");
        let paths = write_icon_set(&spec, &dir, false).unwrap();
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "icon.ico",
                "icon_16.rgba",
                "icon_32.rgba",
                "icon_64.rgba",
                "icon.png"
            ]
        );
        let frames = bouncer_frames();
        let raw = |size: u32| frames.iter().find(|f| f.0 == size).unwrap().1.clone();
        assert_eq!(fs::read(&paths[0]).unwrap(), bouncer_ico());
        assert_eq!(fs::read(&paths[1]).unwrap(), raw(16));
        assert_eq!(fs::read(&paths[2]).unwrap(), raw(32));
        assert_eq!(fs::read(&paths[3]).unwrap(), raw(64));
        let (w, h, preview) = decode_png(&fs::read(&paths[4]).unwrap()).unwrap();
        assert_eq!((w, h), (256, 256));
        assert_eq!(preview, raw(256));
        assert_eq!(listing(&dir).len(), 5, "no temporary files are left behind");

        // refuses to replace anything unless asked, and then writes nothing at all
        let before = listing(&dir);
        let err = write_icon_set(&IconSpec::new("Other"), &dir, false).unwrap_err();
        assert!(err.contains("icon.ico"), "{err}");
        assert_eq!(listing(&dir), before);
        // replacing with the same spec gives the very same bytes
        write_icon_set(&spec, &dir, true).unwrap();
        assert_eq!(listing(&dir), before);
        fs::remove_dir_all(&dir).unwrap();

        // something in the way that cannot be replaced: nothing else may appear
        let dir = scratch_dir("blocked");
        fs::create_dir_all(dir.join("icon_64.rgba")).unwrap();
        for replace in [false, true] {
            let err = write_icon_set(&spec, &dir, replace).unwrap_err();
            assert!(err.contains("icon_64.rgba"), "{err}");
            assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        }
        fs::remove_dir_all(&dir).unwrap();

        // a directory that does not exist is created; one below a plain file is an error
        let nested = scratch_dir("nested").join("a").join("b");
        write_icon_set(&spec, &nested, false).unwrap();
        assert!(nested.join("icon.ico").is_file());
        fs::remove_dir_all(nested.parent().unwrap().parent().unwrap()).unwrap();
        let file_dir = scratch_dir("file");
        fs::create_dir_all(&file_dir).unwrap();
        fs::write(file_dir.join("plain"), b"x").unwrap();
        assert!(write_icon_set(&spec, &file_dir.join("plain").join("sub"), false).is_err());
        fs::remove_dir_all(&file_dir).unwrap();
    }

    #[test]
    fn odd_titles_and_variants_never_panic_and_still_draw() {
        let long_ascii = "A".repeat(10_000);
        let long_emoji = "\u{1F680}\u{FE0F}".repeat(3_000);
        let titles: Vec<&str> = vec![
            "",
            " ",
            "   \t\r\n  ",
            "\u{1F680} Rocket",
            "\u{1F680}",
            "\u{65E5}\u{672C}\u{8A9E}\u{306E}\u{30B2}\u{30FC}\u{30E0}",
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD} \u{5E2}\u{5D5}\u{5DC}\u{5DD}",
            "e\u{301}clair",
            "\u{0}\u{1}\u{2}",
            "***",
            "-",
            "\u{2014} dash first",
            "\u{DF}tone",
            "\u{130}stanbul",
            "\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}",
            "1942",
            "007",
            long_ascii.as_str(),
            long_emoji.as_str(),
        ];
        let variants = [0u32, 1, 2, 7, 4_000_000_000, u32::MAX];
        for title in &titles {
            for &variant in &variants {
                let spec = IconSpec::new(title).with_variant(variant);
                for size in [16u32, 32] {
                    let px = render(&spec, size);
                    assert_eq!(px.len(), (size * size * 4) as usize);
                    let c = (size / 2) as usize;
                    assert_eq!(pixel(&px, size as usize, c, c)[3], 255);
                    assert_eq!(pixel(&px, size as usize, 0, 0)[3], 0);
                }
                let _ = signature(&spec);
            }
        }
        // the packaging entry points survive the extremes too
        for spec in [
            IconSpec::new(""),
            IconSpec::new(&long_emoji).with_variant(u32::MAX),
        ] {
            let file = ico(&spec);
            assert_eq!(parse_ico(&file).unwrap().len(), 10);
            assert!(file.len() < 120_000);
            let (a, b, c) = window_icon_blobs(&spec);
            assert_eq!((a.len(), b.len(), c.len()), (1024, 4096, 16384));
        }
    }

    #[test]
    fn shading_is_always_finite_and_premultiplied() {
        let titles = [
            "",
            "Bouncer",
            "\u{1F680}",
            "Zed",
            "1942",
            "Omega Quest",
            "M",
        ];
        for title in titles {
            for variant in [0, 5] {
                let design = Design::new(&IconSpec::new(title).with_variant(variant));
                for size in [16u32, 96, 256] {
                    let scene = Scene::new(&design, size);
                    for iy in 0..41 {
                        for ix in 0..41 {
                            let (x, y) =
                                (ix as f32 / 40.0 * 1.1 - 0.05, iy as f32 / 40.0 * 1.1 - 0.05);
                            let c = scene.shade(x, y);
                            assert!(
                                c.iter().all(|v| v.is_finite()),
                                "{title:?} {size}px ({x},{y})"
                            );
                            assert!((0.0..=1.0).contains(&c[3]));
                            assert!(c[..3].iter().all(|&v| (0.0..=c[3] + 1e-4).contains(&v)));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn variants_are_substantially_different_designs() {
        let titles = [
            "Bouncer",
            "Neon Relay",
            "Cyber Overdrive",
            "Trigger Happy",
            "My Game",
            "Zed",
        ];
        let mut worst = f32::MAX;
        for title in titles {
            let renders: Vec<Vec<u8>> = (0..6)
                .map(|v| render(&IconSpec::new(title).with_variant(v), 48))
                .collect();
            for a in 0..6 {
                for b in a + 1..6 {
                    let d = mean_diff(&renders[a], &renders[b]);
                    worst = worst.min(d);
                    assert!(d > 12.0, "{title}: variants {a} and {b} differ by only {d}");
                }
            }
            for v in 0..5u32 {
                let x = Design::new(&IconSpec::new(title).with_variant(v));
                let y = Design::new(&IconSpec::new(title).with_variant(v + 1));
                assert_ne!(
                    x.pattern,
                    y.pattern,
                    "{title}: pattern of variants {v}/{}",
                    v + 1
                );
                assert_ne!(
                    x.accent,
                    y.accent,
                    "{title}: accent of variants {v}/{}",
                    v + 1
                );
            }
        }
        eprintln!("smallest mean colour difference between two variants: {worst:.1}");
    }

    #[test]
    fn signature_statistics_over_sample_titles() {
        let titles = sample_titles();
        assert_eq!(titles.len(), 200);
        let mut unique = titles.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 200, "the sample titles must be distinct");
        let sigs: Vec<u64> = titles
            .iter()
            .map(|t| signature(&IconSpec::new(t)))
            .collect();
        let mut dists = Vec::new();
        for i in 0..sigs.len() {
            for j in i + 1..sigs.len() {
                dists.push(hash_distance(sigs[i], sigs[j]));
            }
        }
        dists.sort_unstable();
        let median = dists[dists.len() / 2];
        let mean = dists.iter().map(|&d| f64::from(d)).sum::<f64>() / dists.len() as f64;
        eprintln!(
            "200 sample titles, {} pairs: min {} p1 {} p5 {} median {} mean {:.1} max {}; pairs under 6: {}, under 8: {}",
            dists.len(),
            dists[0],
            dists[dists.len() / 100],
            dists[dists.len() / 20],
            median,
            mean,
            dists[dists.len() - 1],
            dists.iter().filter(|&&d| d < 6).count(),
            dists.iter().filter(|&&d| d < 8).count()
        );
        assert!(median >= 16, "median signature distance {median}");
        assert!(
            dists[0] >= 6,
            "closest pair is only {} bits apart",
            dists[0]
        );
    }

    #[test]
    fn near_identical_titles_still_get_different_signatures() {
        // 100 titles that share their initial and most of their text: the hardest case
        let sigs: Vec<u64> = (1..=100)
            .map(|i| signature(&IconSpec::new(&format!("Game {i}"))))
            .collect();
        let mut dists = Vec::new();
        for i in 0..sigs.len() {
            for j in i + 1..sigs.len() {
                dists.push(hash_distance(sigs[i], sigs[j]));
            }
        }
        dists.sort_unstable();
        let p1 = dists[dists.len() / 100];
        let median = dists[dists.len() / 2];
        eprintln!(
            "100 'Game N' titles: min {} p1 {p1} median {median}",
            dists[0]
        );
        assert!(median >= 16);
        assert!(p1 >= 6);
    }

    #[test]
    fn hash_distance_counts_differing_bits() {
        assert_eq!(hash_distance(0, 0), 0);
        assert_eq!(hash_distance(0, u64::MAX), 64);
        assert_eq!(hash_distance(0b1010, 0b0110), 2);
        let spec = IconSpec::new("Bouncer");
        assert_eq!(signature(&spec), signature(&spec));
        assert_eq!(hash_distance(signature(&spec), signature(&spec)), 0);
    }

    /// Fraction of the tile a hero shape covers and its mask, drawn straight from its geometry.
    fn hero_mask(design: &Design, n: usize) -> Vec<bool> {
        let scene = Scene::new(design, n as u32);
        (0..n * n)
            .map(|i| {
                let (x, y) = (
                    ((i % n) as f32 + 0.5) / n as f32,
                    ((i / n) as f32 + 0.5) / n as f32,
                );
                scene.hero.sd(x, y, 0.0) < 0.0
            })
            .collect()
    }

    #[test]
    fn every_letter_digit_and_emblem_has_a_distinct_readable_shape() {
        let mut design = Design::new(&IconSpec::new("A"));
        design.weight = 1.0;
        design.hero_scale = 1.0;
        design.hero_off = [0.0, 0.0];
        let mut masks: Vec<(String, Vec<bool>)> = Vec::new();
        for c in ('A'..='Z').chain('0'..='9') {
            design.hero = letter_def(c).unwrap();
            let m = hero_mask(&design, 64);
            let cover = m.iter().filter(|&&b| b).count() as f32 / (64.0 * 64.0);
            assert!((0.06..0.40).contains(&cover), "glyph {c} covers {cover}");
            masks.push((c.to_string(), m));
        }
        for k in 0..EMBLEM_COUNT {
            design.hero = emblem_def(k);
            let m = hero_mask(&design, 64);
            let cover = m.iter().filter(|&&b| b).count() as f32 / (64.0 * 64.0);
            assert!((0.06..0.55).contains(&cover), "emblem {k} covers {cover}");
            masks.push((format!("emblem {k}"), m));
        }
        for i in 0..masks.len() {
            for j in i + 1..masks.len() {
                let diff = masks[i]
                    .1
                    .iter()
                    .zip(&masks[j].1)
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(
                    diff > 25,
                    "{} and {} are nearly the same shape ({diff} px)",
                    masks[i].0,
                    masks[j].0
                );
            }
        }
        // letters map case-insensitively, accents fold onto the base letter, the rest has no glyph
        assert_eq!(glyph_char('q'), Some('Q'));
        assert_eq!(glyph_char('\u{E9}'), Some('E'));
        assert_eq!(glyph_char('\u{DF}'), Some('S'));
        assert_eq!(glyph_char('7'), Some('7'));
        for c in ['#', ' ', '\u{1F680}', '\u{65E5}', '\u{3A9}', '\u{0}', '-'] {
            assert_eq!(glyph_char(c), None, "{c:?}");
            assert!(letter_def(c).is_none());
        }
        // title -> hero
        assert!(Design::new(&IconSpec::new("1942")).hero.letter);
        assert!(!Design::new(&IconSpec::new("\u{1F680} Party")).hero.letter);
        assert!(!Design::new(&IconSpec::new("")).hero.letter);
        assert!(Design::new(&IconSpec::new("  \u{C9}clair")).hero.letter);
    }

    #[test]
    fn hero_gets_pixel_fitted_at_small_sizes() {
        // at 16 px an "H" has whole-pixel stems, so its columns are either empty or solid
        let mut design = Design::new(&IconSpec::new("H"));
        design.hero = letter_def('H').unwrap();
        design.weight = 1.0;
        design.hero_scale = 1.0;
        design.hero_off = [0.0, 0.0];
        let scene = Scene::new(&design, 16);
        let h = &scene.hero;
        let stroke = 2.0 * h.hw * 16.0;
        assert!(
            (stroke - stroke.round()).abs() < 1e-3 && stroke >= 2.0,
            "stroke {stroke}"
        );
        assert!(!h.lines.is_empty());
        for l in &h.lines {
            for v in l {
                let px = (v - h.hw) * 16.0;
                assert!(
                    (px - px.round()).abs() < 1e-3,
                    "edge at {px} px is not on the pixel grid"
                );
            }
        }
    }

    #[test]
    fn palettes_avoid_plain_blue_and_extreme_tiles() {
        let hsv = |c: Rgb| {
            let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
            let d = mx - mn;
            let h = if d < 1e-4 {
                0.0
            } else if mx == c[0] {
                60.0 * ((c[1] - c[2]) / d).rem_euclid(6.0)
            } else if mx == c[1] {
                60.0 * ((c[2] - c[0]) / d + 2.0)
            } else {
                60.0 * ((c[0] - c[1]) / d + 4.0)
            };
            (h, if mx > 0.0 { d / mx } else { 0.0 })
        };
        let luma = |c: Rgb| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let (mut lo, mut hi) = (1.0f32, 0.0f32);
        let mut blue = 0;
        for (n, title) in sample_titles().iter().enumerate() {
            for variant in [0u32, 1] {
                let d = Design::new(&IconSpec::new(title).with_variant(variant + n as u32 % 3));
                for i in [0, LUT_N / 2, LUT_N - 1] {
                    let c = d.lut[i];
                    let (h, s) = hsv(c);
                    if (208.0..=256.0).contains(&h) && s > 0.4 {
                        blue += 1;
                        eprintln!("blue-ish tile colour {c:?} hue {h:.0} sat {s:.2} for {title:?} v{variant}");
                    }
                }
                let mean = luma(d.lut[LUT_N / 2]);
                lo = lo.min(mean);
                hi = hi.max(mean);
            }
        }
        eprintln!("tile mid-gradient luma spans {lo:.2} .. {hi:.2}; blue hits: {blue}");
        assert_eq!(blue, 0, "plain blue tiles");
        assert!(lo > 0.15 && hi < 0.85, "tile luminance {lo}..{hi}");
    }

    #[test]
    fn maths_helpers_are_accurate() {
        for i in -400..=400 {
            let x = i as f32 * 0.05;
            let (s, c) = sin_cos(x);
            assert!(
                (s - x.sin()).abs() < 2e-4 && (c - x.cos()).abs() < 2e-4,
                "sin_cos({x})"
            );
        }
        assert_eq!(sin_cos(f32::NAN), (0.0, 1.0));
        assert_eq!(sin_cos(f32::INFINITY), (0.0, 1.0));
        assert!((sin_cos(1.0e9).0).abs() <= 1.0);
        for &(y, x) in &[
            (1.0f32, 1.0f32),
            (-1.0, 2.0),
            (3.0, -0.5),
            (-2.0, -2.0),
            (0.0, -1.0),
            (0.5, 0.0),
        ] {
            assert!((atan2(y, x) - y.atan2(x)).abs() < 3e-4, "atan2({y},{x})");
        }
        assert_eq!(atan2(0.0, 0.0), 0.0);
        assert!(atan2(f32::NAN, 1.0).is_finite());
        for &x in &[1e-6f32, 0.001, 0.1, 0.5, 1.0, 2.0, 10.0, 12345.0] {
            assert!(
                (ln_f(x) - x.ln()).abs() < 1e-4 * (1.0 + x.ln().abs()),
                "ln({x})"
            );
            assert!(
                (powf(x, 0.4167) / x.powf(0.4167) - 1.0).abs() < 2e-4,
                "powf({x})"
            );
        }
        for &x in &[-20.0f32, -1.0, 0.0, 0.3, 1.0, 5.0, 40.0] {
            assert!((exp_f(x) / x.exp() - 1.0).abs() < 2e-4, "exp({x})");
        }
        assert_eq!(powf(-1.0, 2.0), 0.0);
        assert_eq!(sat(f32::NAN), 0.0);
        assert_eq!((sat(-3.0), sat(0.25), sat(9.0)), (0.0, 0.25, 1.0));
        // colour conversion: white, black, and out-of-gamut chroma gets pulled into gamut
        let white = oklch(1.0, 0.0, 0.0);
        assert!(white.iter().all(|&v| (v - 1.0).abs() < 2e-3));
        let black = oklch(0.0, 0.0, 0.0);
        assert!(black.iter().all(|&v| v.abs() < 2e-3));
        for hue in (0..360).step_by(15) {
            for l in [0.2f32, 0.5, 0.8, 0.95] {
                let c = oklch(l, 0.4, hue as f32);
                assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "{c:?}");
            }
        }
    }
}
