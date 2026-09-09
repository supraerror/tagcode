//! # `optimized` → single allocation, no redundant validity checks
//!
//! This module fixes the two main inefficiencies of [`crate::naive`]:
//!
//! 1. **One allocation instead of N.** [`crate::naive::encode`] allocates a
//!    throwaway [`String`] for *every single character* via
//!    [`char::to_string`]. Since [`String`] implements
//!    `FromIterator<char>`, we can instead build a `char` iterator and
//!    `collect()` it directly into one `String`, with no intermediate
//!    allocations. We further call [`String::with_capacity`] up front to
//!    avoid the reallocate-and-copy cycle `Vec`/`String` growth normally
//!    does, sized for the worst case (every character becoming a 4-byte Tag
//!    character).
//! 2. **No redundant validity re-checks.** [`char::from_u32`] returns an
//!    `Option<char>` because, in general, not every `u32` is a valid
//!    Unicode scalar value (surrogates `0xD800..=0xDFFF` and anything above
//!    `0x10FFFF` are excluded). But *our* transformation never produces
//!    such a value - see the safety comment below - so paying for that
//!    check (and for the `unwrap()` panic path) on every character is pure
//!    overhead. We use [`char::from_u32_unchecked`] instead.
//!
//! ## Safety invariant (read this before touching the bounds!)
//!
//! Both `encode` and `decode` only transform a character when its codepoint
//! `cc` falls in a range that guarantees the arithmetic result is a valid
//! `char`:
//!
//! - `encode` only touches `cc` in `0x01..=0x7E`, producing
//!   `cc + 0xE0000`, i.e. a result in `0xE0001..=0xE007E`.
//! - `decode` only touches `cc` in `0xE0001..=0xE007E`, producing
//!   `cc - 0xE0000`, i.e. a result in `0x01..=0x7E`.
//!
//! Both of those output ranges are:
//! - entirely below `char::MAX` (`0x10FFFF`), and
//! - entirely outside the surrogate range `0xD800..=0xDFFF`
//!   (`0x7E < 0xD800` and `0xE0001 > 0xDFFF`).
//!
//! So the result of the `+`/`-` is *always* a valid Unicode scalar value.
//! **If you ever change the range bounds in this file, you must re-verify
//! this invariant, or drop back to the checked, safe [`char::from_u32`].**

/// Hides `text` by remapping every printable ASCII character (codepoints
/// `0x01..=0x7E`) into the invisible Unicode Tags block
/// (`U+E0001..=U+E007E`). Any character outside that range (including NUL,
/// DEL, and all non-ASCII characters) is left untouched.
///
/// # Complexity
///
/// `O(n)` time, **one** heap allocation total (the capacity is
/// pre-reserved for the worst case, where every input character becomes a
/// 4-byte-in-UTF-8 Tag character), versus `O(n)` allocations in
/// [`crate::naive::encode`].
///
/// # Examples
///
/// ```
/// use tagcode::optimized::{encode, decode};
///
/// let hidden = encode("hi");
/// assert_eq!(hidden.chars().count(), 2);
/// assert_eq!(decode(&hidden), "hi");
/// ```
pub fn encode(text: &str) -> String {
    // Worst case: every byte of `text` is an ASCII char (1 byte in UTF-8)
    // that becomes a Tag char (4 bytes in UTF-8) -> up to 4x growth.
    let mut out = String::with_capacity(text.len() * 4);
    for c in text.chars() {
        let cc = c as u32;
        if (0x01..0x7f).contains(&cc) {
            // SAFETY: cc is in 0x01..=0x7E, so cc + 0xE0000 is in
            // 0xE0001..=0xE007E, which is < char::MAX and outside the
            // surrogate range. See the module-level safety invariant.
            out.push(unsafe { char::from_u32_unchecked(cc + 0xe0000) });
        } else {
            out.push(c);
        }
    }
    out
}

/// Reverses [`encode`]: every character whose codepoint falls in
/// `U+E0001..=U+E007E` is mapped back down to its original ASCII value.
/// Characters outside that range (including plain visible text) pass
/// through unchanged.
///
/// # Complexity
///
/// `O(n)` time, **one** heap allocation total (capacity reserved for the
/// output's byte length, which is always `<=` the input's byte length,
/// since decoding only ever shrinks characters from 4 bytes down to 1).
///
/// # Examples
///
/// ```
/// use tagcode::optimized::{encode, decode};
///
/// let hidden = encode("secret");
/// assert_eq!(decode(&hidden), "secret");
/// assert_eq!(decode("plain text"), "plain text");
/// ```
pub fn decode(text: &str) -> String {
    // Decoding only ever shrinks bytes (4-byte Tag char -> 1-byte ASCII),
    // so the input's byte length is always a safe upper bound.
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let cc = c as u32;
        if (0xe0001..0xe007f).contains(&cc) {
            // SAFETY: cc is in 0xE0001..=0xE007E, so cc - 0xE0000 is in
            // 0x01..=0x7E, trivially a valid, non-surrogate scalar value.
            out.push(unsafe { char::from_u32_unchecked(cc - 0xe0000) });
        } else {
            out.push(c);
        }
    }
    out
}

/// Returns `true` if `text` contains at least one character in the
/// invisible Tags range (`U+E0001..=U+E007E`), i.e. whether `text` has a
/// hidden payload produced by [`encode`].
///
/// # Complexity
///
/// `O(n)`, short-circuits on the first match, no allocation. Identical cost
/// to [`crate::naive::contains`] → there was nothing to optimize here, the
/// original was already allocation-free.
///
/// # Examples
///
/// ```
/// use tagcode::optimized::{encode, contains};
///
/// assert!(!contains("just some regular text"));
/// assert!(contains(&encode("regular text")));
/// ```
pub fn contains(text: &str) -> bool {
    text.chars()
        .any(|x| (0xe0001..0xe007f).contains(&(x as u32)))
}
