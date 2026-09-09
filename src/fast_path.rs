//! # `fast_path` → byte-level implementation, no `char` decoding at all
//!
//! **This is the "ultra opti" version.** [`crate::optimized`] already does
//! one allocation and avoids redundant validity checks, but it still pays
//! for `str::chars()` decoding every character of the input into a `char`
//! and then re-encoding that `char` back into UTF-8 on `push`. Since we
//! know exactly what shape the bytes we care about have, we can skip `char`
//! entirely and manipulate raw UTF-8 bytes directly.
//!
//! ## Why this works: the byte pattern of the Tags block
//!
//! Every codepoint in `U+E0000..=U+E007F` encodes to **exactly 4 UTF-8
//! bytes**, and because the whole block spans only 128 contiguous
//! codepoints (a single 7-bit range), three of those four bytes barely
//! vary. Working out the standard 4-byte UTF-8 encoding
//! (`11110uuu 10uuzzzz 10zzzzzz 10zzzzzz`) for `cc = 0xE0000 + b`, where
//! `b` is the original ASCII byte (`0x00..=0x7F`), gives:
//!
//! | byte | value                     | varies? |
//! |------|---------------------------|---------|
//! | 1    | `0xF3`                    | no      |
//! | 2    | `0xA0`                    | no      |
//! | 3    | `0x80 \| (b >> 6)` → `0x80` or `0x81` | only 1 bit |
//! | 4    | `0x80 \| (b & 0x3F)`      | yes (6 bits) |
//!
//! In other words: **`b` is recoverable directly from bytes 3 and 4**
//! (`b = ((byte3 & 1) << 6) | (byte4 & 0x3F)`), and **encoding `b` is just
//! writing 4 fixed-shape bytes** → no `char` construction, no UTF-8
//! encoding logic, no validity branching.
//!
//! ## What this buys you
//!
//! - `encode`: when the input is pure ASCII (checked once via
//!   [`str::is_ascii`], which itself is a fast, vectorizable scan), we
//!   operate directly on `&[u8]` instead of decoding/re-encoding UTF-8
//!   `char`s, and build the output bytes with simple bit arithmetic.
//! - `decode`: we scan the input byte slice for the fixed 4-byte prefix
//!   `F3 A0 (80|81) xx`. On a match we emit one reconstructed ASCII byte;
//!   otherwise we copy that character's bytes through verbatim, using the
//!   leading byte's high bits to know how many bytes to copy, and again, no
//!   `char` decoding is required.
//! - `contains`: a pure byte scan for the same fixed prefix.
//!
//! ## Correctness of the "copy verbatim" byte-length trick
//!
//! When `decode` doesn't find the Tag pattern at position `i`, it must skip
//! over the *whole* character starting there (which may be 1–4 bytes) so it
//! doesn't split a multi-byte character in half. It infers the length from
//! the leading byte's high bits (`0xxxxxxx`→1, `110xxxxx`→2, `1110xxxx`→3,
//! `11110xxx`→4) rather than fully decoding the character. This is sound
//! *specifically because* `text: &str` is already guaranteed valid UTF-8 by
//! Rust's type system → we are not validating untrusted bytes, only reading
//! the length prefix of a sequence we already know is well-formed.
//!
//! ## When to reach for this module
//!
//! Use it when you are encoding/decoding large volumes of text and have
//! measured that [`crate::optimized`] is a bottleneck. It is meaningfully
//! more complex than the other three modules, you could prefer
//! [`crate::optimized`] or [`crate::cow_optimized`] unless you actually
//! need the extra throughput (see `examples/bench.rs` in this crate for a
//! way to check).

/// Hides `text` the same way as [`crate::optimized::encode`], but via raw
/// byte manipulation. Falls back to [`crate::optimized::encode`]
/// automatically for non-ASCII input, since the byte-level fast path only
/// pays off when there's no multi-byte UTF-8 to worry about.
///
/// # Examples
///
/// ```
/// use tagcode::fast_path::{encode, decode};
///
/// let hidden = encode("hi");
/// assert_eq!(decode(&hidden), "hi");
///
/// // Non-ASCII input transparently falls back to the char-based path and
/// // still produces byte-identical output to `optimized::encode`.
/// let mixed = encode("café secret");
/// assert_eq!(mixed, tagcode::optimized::encode("café secret"));
/// ```
pub fn encode(text: &str) -> String {
    if !text.is_ascii() {
        return crate::optimized::encode(text);
    }

    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len() * 4);
    for &b in bytes {
        if 0x00 < b && b < 0x7f {
            out.push(0xF3);
            out.push(0xA0);
            out.push(0x80 | (b >> 6)); // b>>6 is 0 or 1 since b < 0x80
            out.push(0x80 | (b & 0x3F));
        } else {
            out.push(b);
        }
    }

    // SAFETY: every byte we pushed came from one of two sources, both
    // guaranteed to be valid UTF-8:
    //  1. `b` unchanged: `text` was verified ASCII (`is_ascii()`), and
    //     every ASCII byte (`< 0x80`) is trivially valid, standalone UTF-8.
    //  2. The 4-byte sequence `F3 A0 (80|81) (80..=BF)`: this is exactly
    //     the standard UTF-8 encoding of `0xE0000 + b` for `b` in
    //     `0x01..=0x7E`, which is a valid Unicode scalar value (see the
    //     module-level table and `crate::optimized`'s safety invariant).
    unsafe { String::from_utf8_unchecked(out) }
}

/// Reveals `text` the same way as [`crate::optimized::decode`], but via raw
/// byte scanning instead of decoding each character. Handles arbitrary
/// input (ASCII, multi-byte UTF-8, and Tag-block characters mixed
/// together) directly → no fallback needed.
///
/// # Examples
///
/// ```
/// use tagcode::fast_path::{encode, decode};
///
/// let hidden = encode("secret");
/// assert_eq!(decode(&hidden), "secret");
/// assert_eq!(decode("plain text"), "plain text");
/// assert_eq!(decode("café 🎉 plain"), "café 🎉 plain");
/// ```
pub fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if i + 3 < bytes.len()
            && bytes[i] == 0xF3
            && bytes[i + 1] == 0xA0
            && (bytes[i + 2] == 0x80 || bytes[i + 2] == 0x81)
        {
            let hi = bytes[i + 2] & 0x01;
            let lo = bytes[i + 3] & 0x3F;
            let b = (hi << 6) | lo;
            // Exclude the block's exact boundary codepoints (U+E0000 and
            // U+E007F), matching `decode`'s exclusive bounds everywhere
            // else in this crate: b == 0 -> U+E0000, b == 0x7F -> U+E007F.
            if b != 0x00 && b != 0x7f {
                // SAFETY: `b` is in `0x01..=0x7E`, always a valid,
                // non-surrogate Unicode scalar value.
                out.push(unsafe { char::from_u32_unchecked(b as u32) });
                i += 4;
                continue;
            }
        }

        // No Tag-char match at `i`: copy this character's bytes through
        // unchanged. `text` is a valid `&str`, so the leading byte's high
        // bits alone are enough to know the sequence length safely.
        let len = utf8_len(bytes[i]);
        out.push_str(&text[i..i + len]);
        i += len;
    }

    out
}

/// Returns `true` if `text` contains at least one hidden Tag-block
/// character, using a raw byte scan for the fixed `F3 A0 (80|81) xx`
/// prefix instead of decoding each character.
///
/// # Examples
///
/// ```
/// use tagcode::fast_path::{encode, contains};
///
/// assert!(!contains("just some regular text"));
/// assert!(contains(&encode("regular text")));
/// ```
pub fn contains(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 4 {
        return false;
    }
    for i in 0..=bytes.len() - 4 {
        if bytes[i] == 0xF3
            && bytes[i + 1] == 0xA0
            && (bytes[i + 2] == 0x80 || bytes[i + 2] == 0x81)
        {
            let hi = bytes[i + 2] & 0x01;
            let lo = bytes[i + 3] & 0x3F;
            let b = (hi << 6) | lo;
            if b != 0x00 && b != 0x7f {
                return true;
            }
        }
    }
    false
}

/// Number of bytes in the UTF-8 sequence starting with leading byte `b0`.
/// Only meaningful when `b0` is known to be a valid UTF-8 leading byte
/// (true for every byte at a char boundary in a valid `&str`).
#[inline]
fn utf8_len(b0: u8) -> usize {
    if b0 & 0x80 == 0x00 {
        1
    } else if b0 & 0xE0 == 0xC0 {
        2
    } else if b0 & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}
