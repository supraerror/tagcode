//! # `cow_optimized` → zero allocation on the "nothing to do" path
//!
//! [`crate::optimized`] always allocates a fresh [`String`], even when the
//! input has nothing to encode or decode (e.g. calling `encode` on a string
//! that is already pure non-ASCII text, or calling `decode` on text with no
//! hidden payload). If that's a common case in your workload, e.g. you
//! `decode()` every incoming message just in case, but most messages carry
//! no hidden payload → that allocation is pure waste.
//!
//! This module returns [`std::borrow::Cow<str>`]: a [`Cow::Borrowed`]
//! pointing straight back into the input with **zero allocation** when
//! nothing needs changing, or a [`Cow::Owned`] `String` (built the same way
//! as [`crate::optimized`]) when a transformation actually happens.
//!
//! ## The trade-off
//!
//! Detecting the "nothing to do" case requires scanning the string once to
//! check; if a transformation *is* needed, we then scan it a second time
//! (implicitly, via the encode/decode loop) to build the output. So:
//!
//! - **Best case** (nothing to encode/decode): one cheap scan, zero
//!   allocations. Strictly faster than [`crate::optimized`].
//! - **Worst case** (everything needs transforming): one extra `O(n)` scan
//!   compared to [`crate::optimized`], for the same single final
//!   allocation.
//!
//! Prefer this module when you expect the "no-op" case to be common;
//! prefer [`crate::optimized`] when you know a transformation will almost
//! always happen (the pre-scan would then just be wasted work).

use std::borrow::Cow;

/// Like [`crate::optimized::encode`], but returns a borrowed [`Cow`] with no
/// allocation when `text` contains no printable-ASCII characters (`0x01..=0x7E`)
/// to hide.
///
/// # Examples
///
/// ```
/// use tagcode::cow_optimized::{encode, decode};
/// use std::borrow::Cow;
///
/// // Nothing to encode: zero allocation, we get the input back borrowed.
/// let untouched = encode("日本語");
/// assert!(matches!(untouched, Cow::Borrowed(_)));
///
/// // Something to encode: a new owned String is allocated.
/// let hidden = encode("hi");
/// assert!(matches!(hidden, Cow::Owned(_)));
/// assert_eq!(decode(&hidden), "hi");
/// ```
pub fn encode(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| (0x01..0x7f).contains(&(c as u32))) {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len() * 4);
    for c in text.chars() {
        let cc = c as u32;
        if (0x01..0x7f).contains(&cc) {
            // SAFETY: see the invariant documented in `crate::optimized`.
            out.push(unsafe { char::from_u32_unchecked(cc + 0xe0000) });
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// Like [`crate::optimized::decode`], but returns a borrowed [`Cow`] with no
/// allocation when `text` contains no hidden Tag-block payload
/// (`U+E0001..=U+E007E`) to reveal.
///
/// # Examples
///
/// ```
/// use tagcode::cow_optimized::{encode, decode};
/// use std::borrow::Cow;
///
/// // Nothing hidden: zero allocation.
/// let untouched = decode("plain text");
/// assert!(matches!(untouched, Cow::Borrowed(_)));
///
/// let hidden = encode("secret");
/// let revealed = decode(&hidden);
/// assert!(matches!(revealed, Cow::Owned(_)));
/// assert_eq!(revealed, "secret");
/// ```
pub fn decode(text: &str) -> Cow<'_, str> {
    if !text
        .chars()
        .any(|c| (0xe0001..0xe007f).contains(&(c as u32)))
    {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let cc = c as u32;
        if (0xe0001..0xe007f).contains(&cc) {
            // SAFETY: see the invariant documented in `crate::optimized`.
            out.push(unsafe { char::from_u32_unchecked(cc - 0xe0000) });
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}
