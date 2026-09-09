//! "Heavy" benchmark: unlike `examples/bench.rs` (one small input, one
//! sample per implementation), this one sweeps input **size** (64 B to
//! 2 MiB) and **content profile** (pure ASCII / mixed UTF-8 / nothing to
//! hide) and reports proper statistics min / median / mean / max over
//! many repeats, plus throughput in MiB/s for all four implementations.
//!
//! This is what actually shows the optimizations paying off (or not): at
//! small sizes, fixed overhead (function call, `is_ascii()` scan, branch
//! prediction warm-up) dominates and the gaps are noisy; at large sizes,
//! the allocation strategy dominates and the `naive` module's
//! one-allocation-per-character behaviour becomes dramatically,
//! unmissably worse.
//!
//! Run with `cargo run --release --example heavy_bench` (or `just
//! heavy-bench`). Expect it to take on the order of several seconds,
//! most of it spent waiting on `naive` at the largest sizes.

use std::time::{Duration, Instant};
use tagcode::{cow_optimized, fast_path, naive, optimized};

/// Which kind of content to generate for a given size.
#[derive(Clone, Copy)]
enum Profile {
    /// Pure printable ASCII: every character is encodable.
    Ascii,
    /// A mix of encodable ASCII and passthrough multi-byte UTF-8
    /// (accents, CJK, emoji) for a more realistic "real chat message" shape.
    Mixed,
    /// Multi-byte UTF-8 only, nothing encodable at all.
    /// The case `cow_optimized` is specifically built to win.
    NoOp,
}

impl Profile {
    fn label(self) -> &'static str {
        match self {
            Profile::Ascii => "pure ASCII",
            Profile::Mixed => "mixed ASCII+UTF-8",
            Profile::NoOp => "no-op (nothing to hide)",
        }
    }

    fn base_pattern(self) -> &'static str {
        match self {
            Profile::Ascii => "The quick brown fox jumps over the lazy dog! 0123456789 #!?",
            Profile::Mixed => "café | 日本語 | 🩸☠ The quick brown fox! 0123456789",
            Profile::NoOp => "ただの日本語の文章です。隠すようなことは何もありません。",
        }
    }
}

/// Builds a `String` of approximately `target_bytes` bytes by repeating
/// `profile`'s base pattern, cut on the nearest character boundary at or
/// below `target_bytes` (never splits a multi-byte character).
fn make_text(profile: Profile, target_bytes: usize) -> String {
    let base = profile.base_pattern();
    let mut s = String::with_capacity(target_bytes + base.len());
    while s.len() < target_bytes {
        s.push_str(base);
    }
    match s
        .char_indices()
        .take_while(|&(i, _)| i <= target_bytes)
        .last()
    {
        Some((i, c)) => s[..i + c.len_utf8()].to_string(),
        None => String::new(),
    }
}

/// Summary statistics for one (implementation, size, profile) combination.
struct Stats {
    min: Duration,
    median: Duration,
    mean: Duration,
    max: Duration,
    mib_per_sec: f64,
}

fn measure(mut f: impl FnMut() -> usize, input_bytes: usize, repeats: usize) -> Stats {
    // Warm-up: let the allocator settle and branch predictors warm up
    // before any timed sample.
    for _ in 0..3.min(repeats) {
        std::hint::black_box(f());
    }

    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        std::hint::black_box(f());
        samples.push(start.elapsed());
    }
    samples.sort();

    let min = samples[0];
    let max = samples[samples.len() - 1];
    let median = samples[samples.len() / 2];
    let mean = samples.iter().sum::<Duration>() / samples.len() as u32;

    let mib_per_sec = if mean.as_secs_f64() > 0.0 {
        (input_bytes as f64 / (1024.0 * 1024.0)) / mean.as_secs_f64()
    } else {
        f64::INFINITY
    };

    Stats {
        min,
        median,
        mean,
        max,
        mib_per_sec,
    }
}

/// Repeats scale down as size grows, so the whole sweep finishes in a
/// reasonable time even though `naive` gets very slow at large sizes
/// (one allocation per character).
fn repeats_for(size: usize) -> usize {
    match size {
        0..=1024 => 300,
        1025..=16_384 => 100,
        16_385..=262_144 => 30,
        _ => 8,
    }
}

fn print_header(op: &str, profile: Profile, size: usize) {
    println!("\n### {op} — {} — {} bytes", profile.label(), size);
    println!(
        "{:<16} {:>10} {:>10} {:>10} {:>10} {:>12}",
        "impl", "min", "median", "mean", "max", "MiB/s"
    );
}

fn print_row(name: &str, s: Stats) {
    println!(
        "{name:<16} {:>10.2?} {:>10.2?} {:>10.2?} {:>10.2?} {:>12.1}",
        s.min, s.median, s.mean, s.max, s.mib_per_sec
    );
}

fn bench_encode(profile: Profile, size: usize) {
    let text = make_text(profile, size);
    let n = text.len();
    let repeats = repeats_for(n);
    print_header("encode", profile, n);

    print_row("naive", measure(|| naive::encode(&text).len(), n, repeats));
    print_row(
        "optimized",
        measure(|| optimized::encode(&text).len(), n, repeats),
    );
    print_row(
        "cow_optimized",
        measure(|| cow_optimized::encode(&text).len(), n, repeats),
    );
    print_row(
        "fast_path",
        measure(|| fast_path::encode(&text).len(), n, repeats),
    );
}

fn bench_decode(profile: Profile, size: usize) {
    // For decode, feed already-encoded text (except for the NoOp profile,
    // which by definition has nothing to decode either way).
    let plain = make_text(profile, size);
    let text = match profile {
        Profile::NoOp => plain,
        _ => optimized::encode(&plain),
    };
    let n = text.len();
    let repeats = repeats_for(n);
    print_header("decode", profile, n);

    print_row("naive", measure(|| naive::decode(&text).len(), n, repeats));
    print_row(
        "optimized",
        measure(|| optimized::decode(&text).len(), n, repeats),
    );
    print_row(
        "cow_optimized",
        measure(|| cow_optimized::decode(&text).len(), n, repeats),
    );
    print_row(
        "fast_path",
        measure(|| fast_path::decode(&text).len(), n, repeats),
    );
}

fn main() {
    let sizes = [64usize, 1024, 16 * 1024, 256 * 1024, 2 * 1024 * 1024];
    let profiles = [Profile::Ascii, Profile::Mixed, Profile::NoOp];

    println!("# tagcode heavy benchmark");
    println!(
        "sizes: {:?} bytes, profiles: ascii / mixed / no-op, release build required",
        sizes
    );

    for &size in &sizes {
        for &profile in &profiles {
            bench_encode(profile, size);
        }
    }

    for &size in &sizes {
        for &profile in &profiles {
            bench_decode(profile, size);
        }
    }
}
