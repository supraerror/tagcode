//! # tagcode → Unicode Tags steganography (a.k.a. "3y3" / "secondsight")
//!
//! This crate hides printable ASCII text inside a string by remapping each
//! ASCII byte into the invisible **Unicode Tags block** (`U+E0000..=U+E007F`),
//! and reverses the process to reveal it again.
//!
//! ## Background
//!
//! The technique originates from <https://synthetic.garden/3y3.htm> and was
//! popularized by the JavaScript reference implementation at
//! <https://github.com/ArjixWasTaken/3y3>. A Rust port used in
//! <https://github.com/bitten2up/secondsight> implements the same algorithm.
//! This crate re-implements that exact algorithm (same bounds, same
//! behaviour) in four variants that trade off simplicity, safety and speed
//! differently. All four are drop-in compatible: encoding with one and
//! decoding with another always round-trips correctly, because the bit
//! mapping (`codepoint + 0xE0000` / `codepoint - 0xE0000`) is identical
//! everywhere.
//!
//! ## The algorithm, precisely
//!
//! For every `char` `c` in the input, with `cc = c as u32`:
//!
//! - **encode**: if `0x00 < cc < 0x7F` (i.e. `cc` is in `0x01..=0x7E`,
//!   printable/controlish ASCII excluding NUL and DEL), replace it with the
//!   character at codepoint `cc + 0xE0000`. Otherwise leave `c` unchanged.
//! - **decode**: if `0xE0000 < cc < 0xE007F` (i.e. `cc` is in
//!   `0xE0001..=0xE007E`), replace it with the character at codepoint
//!   `cc - 0xE0000`. Otherwise leave `c` unchanged.
//! - **contains**: `true` if any character's codepoint falls in
//!   `0xE0001..=0xE007E`.
//!
//! Both bounds are **exclusive**, which is intentional and matches the
//! upstream implementation:
//! - `cc = 0x00` (NUL) is excluded from encoding, so `encode` never emits
//!   `U+E0000` (`TAG` "cancel" marker), and `decode`'s exclusive lower bound
//!   correctly never touches it either → the two are consistent by
//!   construction.
//! - `cc = 0x7F` (DEL) is excluded from encoding, so `encode` never emits
//!   `U+E007F` (`CANCEL TAG`), and `decode`'s exclusive upper bound matches.
//!
//! Every codepoint this crate ever produces via `+ 0xE0000` therefore lies
//! strictly inside `U+E0001..=U+E007E`, which:
//! - never collides with the UTF-16 surrogate range (`U+D800..=U+DFFF`),
//! - never exceeds `char::MAX` (`U+10FFFF`),
//! - is always a valid Unicode scalar value.
//!
//! This invariant is what makes the `unsafe` fast paths in
//! [`optimized`], [`cow_optimized`] and [`fast_path`] sound (see the safety
//! comments in each module).
//!
//! ## Which version should I use?
//!
//! | Module          | Allocations         | Unsafe | Best for                                   |
//! |-----------------|----------------------|--------|---------------------------------------------|
//! | [`naive`]        | O(n) small Strings   | no     | Reference / matches the original 1:1        |
//! | [`optimized`]     | 1 String             | yes    | General-purpose default                     |
//! | [`cow_optimized`] | 0 when no-op, else 1 | yes    | Hot paths that often have nothing to encode |
//! | [`fast_path`]     | 1 String, byte-level | yes    | Maximum throughput on ASCII-heavy input     |
//!
//! See each module's documentation for a full explanation and a benchmark
//! you can run yourself with `cargo run --release --example bench`.

pub mod cow_optimized;
pub mod fast_path;
pub mod meta;
pub mod naive;
pub mod optimized;

#[cfg(test)]
mod tests {
    //! Cross-implementation correctness tests: every implementation must
    //! agree with every other one, and every encode/decode pair must
    //! round-trip, across a battery of edge cases.

    use crate::{cow_optimized, fast_path, naive, optimized};

    /// Strings chosen to exercise boundary conditions:
    /// empty input, NUL/DEL (which must NOT be touched), the full ASCII
    /// range, multi-byte UTF-8 (accents, CJK, emoji, astral-plane
    /// characters), and text that already contains real Tag-block
    /// characters before encoding.
    fn corpus() -> Vec<String> {
        vec![
            String::new(),
            "hello, world!".to_string(),
            "Hello, World! 123 #!?".to_string(),
            "\u{0000}\u{007F}".to_string(), // NUL and DEL: must pass through
            (0x01u8..=0x7Eu8).map(|b| b as char).collect::<String>(), // full printable ASCII range
            "café | 日本語 | 🩸☠".to_string(), // multi-byte / astral
            "\u{10FFFF}".to_string(),       // max valid scalar value
            "already \u{E0068}\u{E0069} tagged".to_string(), // pre-existing tag chars
        ]
    }

    #[test]
    fn all_implementations_agree_on_encode() {
        for s in corpus() {
            let a = naive::encode(&s);
            let b = optimized::encode(&s);
            let c = cow_optimized::encode(&s).into_owned();
            let d = fast_path::encode(&s);
            assert_eq!(a, b, "naive vs optimized mismatch for {s:?}");
            assert_eq!(a, c, "naive vs cow_optimized mismatch for {s:?}");
            assert_eq!(a, d, "naive vs fast_path mismatch for {s:?}");
        }
    }

    #[test]
    fn all_implementations_agree_on_decode() {
        for s in corpus() {
            let encoded = naive::encode(&s);
            let a = naive::decode(&encoded);
            let b = optimized::decode(&encoded);
            let c = cow_optimized::decode(&encoded).into_owned();
            let d = fast_path::decode(&encoded);
            assert_eq!(a, b);
            assert_eq!(a, c);
            assert_eq!(a, d);
        }
    }

    #[test]
    fn round_trip_is_lossless() {
        // Note: round-tripping is only lossless for inputs that do not
        // *already* contain real Tag-block characters. If the input has
        // pre-existing tag characters (like the "already tagged" corpus
        // entry), `decode(encode(s))` will also decode those pre-existing
        // characters, which is expected behaviour, not a bug → the
        // algorithm cannot distinguish "a tag character that was always
        // plain text" from "a tag character produced by encoding". This
        // is a property of the *algorithm* (shared with the original
        // upstream implementation), not something any of these Rust
        // ports could fix without changing the wire format.
        for s in corpus().into_iter().filter(|s| !naive::contains(s)) {
            let encoded = optimized::encode(&s);
            let decoded = optimized::decode(&encoded);
            assert_eq!(s, decoded, "round trip failed for {s:?}");
        }
    }

    #[test]
    fn contains_agrees_across_implementations() {
        for s in corpus() {
            let plain_says = naive::contains(&s);
            let opt_says = optimized::contains(&s);
            let fast_says = fast_path::contains(&s);
            assert_eq!(plain_says, opt_says);
            assert_eq!(plain_says, fast_says);

            let encoded = naive::encode(&s);
            // Encoding non-empty printable ASCII must produce something
            // `contains` detects (unless the string had none to encode).
            let had_encodable = s.chars().any(|c| (0x01..0x7F).contains(&(c as u32)));
            assert_eq!(naive::contains(&encoded), had_encodable || plain_says);
        }
    }

    #[test]
    fn nul_and_del_are_never_touched() {
        let s = "\u{0000}A\u{007F}";
        let encoded = optimized::encode(s);
        assert!(encoded.contains('\u{0000}'));
        assert!(encoded.contains('\u{007F}'));
        assert!(encoded.contains('\u{E0041}')); // 'A' (0x41) was encoded
    }
}
