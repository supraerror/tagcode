# Tagcode

Four Rust implementations of the "3y3" / "secondsight" Unicode Tags
steganography technique (hiding ASCII text as invisible characters in the
`U+E0000..=U+E007F` block), from a direct 1:1 port of the original algorithm
up to a byte-level "ultra opti" version.

- Original JS algorithm: <https://synthetic.garden/3y3.htm>,
  popularized at <https://github.com/ArjixWasTaken/3y3>
- Rust port this crate started from: <https://github.com/bitten2up/secondsight/blob/master/src/lib.rs>

All documentation is inline as Rust doc comments (`///` / `//!`). Run
`cargo doc --open --no-deps` for a browsable, cross-linked version.

## Modules

| Module          | File                   | Summary                                                                                                                 |
|-----------------|------------------------|-------------------------------------------------------------------------------------------------------------------------|
| `naive`         | `src/naive.rs`         | Direct 1:1 port of the original. Correct, but allocates a `String` per character.                                       |
| `optimized`     | `src/optimized.rs`     | One allocation total, no redundant validity checks (`char::from_u32_unchecked` with a documented safety invariant).     |
| `cow_optimized` | `src/cow_optimized.rs` | Same as `optimized`, but returns `Cow<str>` — **zero allocation** when there's nothing to encode/decode.                |
| `fast_path`     | `src/fast_path.rs`     | Byte-level implementation that skips `char` decoding entirely, exploiting the fixed UTF-8 byte shape of the Tags block. |

## Running things

```bash
cargo test                          # correctness: all 4 impls cross-checked + doctests
cargo doc --open --no-deps          # full docs, browsable
cargo run --release --example bench # small micro-benchmark comparing all 4
just heavy-bench                    # heavy benchmark: 64B..2MiB x 3 content profiles, real stats
just demo                           # end-to-end demo including the `meta` dispatcher
```

A `justfile` is included (`test`, `doctest`, `doc`, `bench`, `heavy-bench`, `fmt`, `lint`, `check`, `build`, `clean`,
`ci`, `demo`, you can run `just` with no arguments to list them all).

## Safety

`optimized`, `cow_optimized`, and `fast_path` use `unsafe` (mainly
`char::from_u32_unchecked` and, in `fast_path`,
`String::from_utf8_unchecked`). Every `unsafe` block has a `// SAFETY:`
comment explaining exactly why it's sound, and the module-level docs in
`optimized.rs` spell out the invariant all three modules rely on. All four
modules are cross-checked against each other in `src/lib.rs`'s test suite,
including edge cases (NUL, DEL, the full ASCII range, multi-byte/astral
UTF-8, and text that already contains real Tag-block characters before
encoding).

