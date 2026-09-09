//! # `naive` → direct, unoptimized port
//!
//! This is a straight Rust translation of the code from
//! <https://github.com/bitten2up/secondsight/blob/master/src/lib.rs>, which
//! is itself a port of the JavaScript reference implementation at
//! <https://github.com/ArjixWasTaken/3y3>.
//!
//! It is correct and easy to audit line-by-line against the original, but
//! it is **not** optimized: every character is individually converted to an
//! owned [`String`] via [`char::to_string`] before being collected, which
//! means one small heap allocation per character instead of one allocation
//! for the whole output.
//!
//! Use this module as the reference implementation for correctness testing,
//! or when you specifically want code that mirrors the upstream source as
//! closely as possible. For anything performance-sensitive, prefer
//! [`crate::optimized`] or [`crate::fast_path`].

/// Hides `text` by remapping every printable ASCII character (codepoints
/// `0x01..=0x7E`) into the invisible Unicode Tags block
/// (`U+E0001..=U+E007E`). Any character outside that range (including NUL,
/// DEL, and all non-ASCII characters) is left untouched.
///
/// # Complexity
///
/// `O(n)` time, but performs one heap allocation *per character* (via
/// [`char::to_string`]) plus the allocations made while [`collect`]ing the
/// resulting `String`. See [`crate::optimized::encode`] for a
/// single-allocation version.
///
/// [`collect`]: Iterator::collect
///
/// # Examples
///
/// ```
/// use tagcode::naive::{encode, decode};
///
/// let hidden = encode("hi");
/// // The output *looks* like two invisible characters when printed, but
/// // it actually contains real, distinct codepoints in the Tags block.
/// assert_eq!(hidden.chars().count(), 2);
/// assert_eq!(decode(&hidden), "hi");
/// ```
pub fn encode(text: &str) -> String {
    text.chars()
        .map(|x| {
            if 0x00 < (x as u32) && (x as u32) < 0x7f {
                char::from_u32((x as u32) + 0xe0000).unwrap().to_string()
            } else {
                x.to_string()
            }
        })
        .collect()
}

/// Reverses [`encode`]: every character whose codepoint falls in
/// `U+E0001..=U+E007E` is mapped back down to its original ASCII value.
/// Characters outside that range (including plain visible text) pass
/// through unchanged.
///
/// # Complexity
///
/// Same allocation profile as [`encode`]: one small `String` per character.
///
/// # Examples
///
/// ```
/// use tagcode::naive::{encode, decode};
///
/// let hidden = encode("secret");
/// assert_eq!(decode(&hidden), "secret");
/// // Text with nothing hidden decodes to itself, unchanged.
/// assert_eq!(decode("plain text"), "plain text");
/// ```
pub fn decode(text: &str) -> String {
    text.chars()
        .map(|x| {
            if 0xe0000 < (x as u32) && (x as u32) < 0xe007f {
                char::from_u32((x as u32) - 0xe0000).unwrap().to_string()
            } else {
                x.to_string()
            }
        })
        .collect()
}

/// Returns `true` if `text` contains at least one character in the
/// invisible Tags range (`U+E0001..=U+E007E`), i.e. whether `text` has a
/// hidden payload produced by [`encode`].
///
/// # Complexity
///
/// `O(n)` and short-circuits on the first match thanks to [`Iterator::any`];
/// no allocation.
///
/// # Examples
///
/// ```
/// use tagcode::naive::{encode, contains};
///
/// assert!(!contains("just some regular text"));
/// assert!(contains(&encode("regular text")));
/// ```
pub fn contains(text: &str) -> bool {
    text.chars()
        .any(|x| 0xe0000 < (x as u32) && (x as u32) < 0xe007f)
}
