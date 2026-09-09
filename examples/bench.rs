//! Minimal, dependency-free micro-benchmark comparing the four
//! implementations in this crate.
//!
//! This intentionally avoids pulling in `criterion` so it runs anywhere
//! with just `cargo run --release --example bench`. It is not as rigorous
//! as a proper statistical benchmark harness, but it's more than enough to
//! see the relative ordering and rough magnitude of the differences
//! between implementations.
//!
//! Run with `--release`; in debug mode the `unsafe` fast paths and the
//! naive per-char allocation path will not show a realistic gap because
//! nothing is optimized.

use std::time::Instant;
use tagcode::{cow_optimized, fast_path, naive, optimized};

const ITERATIONS: u32 = 20_000;

fn bench(name: &str, f: impl Fn() -> usize) {
    // Warm-up run so allocator/caches are in a steady state before timing.
    let _ = f();

    let start = Instant::now();
    let mut sink = 0usize; // prevents the optimizer from eliding the calls
    for _ in 0..ITERATIONS {
        sink = sink.wrapping_add(f());
    }
    let elapsed = start.elapsed();
    println!(
        "{name:<28} {:>10.2?} total   {:>8.1?} / iter   (sink={sink})",
        elapsed,
        elapsed / ITERATIONS
    );
}

fn main() {
    let ascii_text = "The quick brown fox jumps over the lazy dog! 0123456789 #!?".repeat(5);
    let mixed_text = "café | 日本語 | 🩸☠ The quick brown fox! 0123456789".repeat(5);
    let plain_no_op = "ただの日本語の文章です。隠すようなことは何もありません。".repeat(5);

    println!(
        "=== encode: pure ASCII input ({} bytes) ===",
        ascii_text.len()
    );
    bench("naive::encode", || naive::encode(&ascii_text).len());
    bench("optimized::encode", || optimized::encode(&ascii_text).len());
    bench("cow_optimized::encode", || {
        cow_optimized::encode(&ascii_text).len()
    });
    bench("fast_path::encode", || fast_path::encode(&ascii_text).len());

    println!(
        "\n=== encode: mixed ASCII + multi-byte UTF-8 ({} bytes) ===",
        mixed_text.len()
    );
    bench("naive::encode", || naive::encode(&mixed_text).len());
    bench("optimized::encode", || optimized::encode(&mixed_text).len());
    bench("cow_optimized::encode", || {
        cow_optimized::encode(&mixed_text).len()
    });
    bench("fast_path::encode", || fast_path::encode(&mixed_text).len());

    println!(
        "\n=== encode: no-op input, nothing to hide ({} bytes) ===",
        plain_no_op.len()
    );
    bench("naive::encode", || naive::encode(&plain_no_op).len());
    bench("optimized::encode", || {
        optimized::encode(&plain_no_op).len()
    });
    bench("cow_optimized::encode", || {
        cow_optimized::encode(&plain_no_op).len()
    });
    bench("fast_path::encode", || {
        fast_path::encode(&plain_no_op).len()
    });

    let encoded_ascii = optimized::encode(&ascii_text);
    println!(
        "\n=== decode: payload present ({} bytes) ===",
        encoded_ascii.len()
    );
    bench("naive::decode", || naive::decode(&encoded_ascii).len());
    bench("optimized::decode", || {
        optimized::decode(&encoded_ascii).len()
    });
    bench("cow_optimized::decode", || {
        cow_optimized::decode(&encoded_ascii).len()
    });
    bench("fast_path::decode", || {
        fast_path::decode(&encoded_ascii).len()
    });

    println!(
        "\n=== decode: no payload, plain text ({} bytes) ===",
        plain_no_op.len()
    );
    bench("naive::decode", || naive::decode(&plain_no_op).len());
    bench("optimized::decode", || {
        optimized::decode(&plain_no_op).len()
    });
    bench("cow_optimized::decode", || {
        cow_optimized::decode(&plain_no_op).len()
    });
    bench("fast_path::decode", || {
        fast_path::decode(&plain_no_op).len()
    });
}
