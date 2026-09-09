//! # `meta` → pick (or *discover*) the fastest implementation for a given input
//!
//! This module offers two different notions of "meta-algorithm", because
//! they solve different problems:
//!
//! 1. [`choose_encode_algo`] / [`choose_decode_algo`] and their
//!    convenience wrappers [`encode`] / [`decode`]: a **cheap heuristic
//!    dispatcher**. It looks at trivial, O(n)-but-allocation-free
//!    properties of the input (does it have anything to encode/decode at
//!    all?) and picks an implementation based on what
//!    `examples/bench.rs` measured about this crate's own algorithms. No
//!    timing happens at call time. This is meant to be used on every
//!    call, in production, at effectively zero overhead beyond the
//!    dispatch itself.
//!
//! 2. [`benchmark_encode`] / [`benchmark_decode`] and the auto-tuned
//!    wrappers [`encode_auto_tuned`] / [`decode_auto_tuned`]: the
//!    **literal** meta-algorithm the request asked for, it actually runs
//!    all four candidate implementations against *this specific input*,
//!    times them, and picks whichever one measured fastest, with no
//!    assumptions baked in. This costs `iterations × 4` real encode/decode
//!    calls before it returns anything, so it's meant for calibration
//!    (e.g. "run this once at startup against a representative sample of
//!    my real workload, then hard-code the winner") rather than for
//!    calling on every request.
//!
//! Both approaches are provided because they answer different questions:
//! heuristics are what you actually want in a hot path; empirical
//! benchmarking is what you want when you don't trust or don't yet know;
//! The heuristic for a new workload shape.

use crate::{cow_optimized, fast_path, naive, optimized};
use std::hint::black_box;
use std::time::{Duration, Instant};

/// Identifies which of the four sibling implementations
/// ([`crate::naive`], [`crate::optimized`], [`crate::cow_optimized`],
/// [`crate::fast_path`]) a decision landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Algo {
    Naive,
    Optimized,
    CowOptimized,
    FastPath,
}

impl Algo {
    /// Short, stable, human-readable name → handy for logging which
    /// algorithm a call picked.
    pub const fn name(self) -> &'static str {
        match self {
            Algo::Naive => "naive",
            Algo::Optimized => "optimized",
            Algo::CowOptimized => "cow_optimized",
            Algo::FastPath => "fast_path",
        }
    }
}

type AlgoLambda = (Algo, fn(&str) -> usize);

// ---------------------------------------------------------------------
// 1. Cheap heuristic dispatch (no per-call timing)
// ---------------------------------------------------------------------

/// Picks an [`Algo`] for encoding `text`, using only properties measurable
/// without allocating.
///
/// Rationale, straight from the measurements in `examples/bench.rs`:
/// - If `text` has **nothing encodable** (no char in `0x01..=0x7E`),
///   [`crate::cow_optimized::encode`] returns a borrow with zero
///   allocation, a strictly-dominant win with no downside, so it's
///   always picked in that case.
/// - Otherwise, [`crate::optimized::encode`] is the measured, reliable
///   default: in this crate's own benchmark, [`crate::fast_path::encode`]
///   did **not** show a consistent edge over it (see the crate `README`),
///   so there is no evidence-based reason to prefer the extra complexity
///   of the byte-level path by default.
pub fn choose_encode_algo(text: &str) -> Algo {
    let has_encodable = text.chars().any(|c| (0x01..0x7f).contains(&(c as u32)));
    if has_encodable {
        Algo::Optimized
    } else {
        Algo::CowOptimized
    }
}

/// Picks an [`Algo`] for decoding `text`, using the same reasoning as
/// [`choose_encode_algo`], but checking for an existing hidden payload
/// instead of encodable characters.
pub fn choose_decode_algo(text: &str) -> Algo {
    if naive::contains(text) {
        Algo::Optimized
    } else {
        Algo::CowOptimized
    }
}

/// Runs `text` through [`choose_encode_algo`]'s chosen implementation.
///
/// # Examples
///
/// ```
/// use tagcode::meta;
///
/// assert_eq!(meta::decode(&meta::encode("hi")), "hi");
/// ```
pub fn encode(text: &str) -> String {
    match choose_encode_algo(text) {
        Algo::Naive => naive::encode(text),
        Algo::Optimized => optimized::encode(text),
        Algo::CowOptimized => cow_optimized::encode(text).into_owned(),
        Algo::FastPath => fast_path::encode(text),
    }
}

/// Runs `text` through [`choose_decode_algo`]'s chosen implementation.
///
/// # Examples
///
/// ```
/// use tagcode::meta;
///
/// assert_eq!(meta::decode("plain text"), "plain text");
/// ```
pub fn decode(text: &str) -> String {
    match choose_decode_algo(text) {
        Algo::Naive => naive::decode(text),
        Algo::Optimized => optimized::decode(text),
        Algo::CowOptimized => cow_optimized::decode(text).into_owned(),
        Algo::FastPath => fast_path::decode(text),
    }
}

// ---------------------------------------------------------------------
// 2. Empirical auto-tuning: actually benchmark the candidates
// ---------------------------------------------------------------------

/// One candidate's measured cost: which [`Algo`] it was, and the average
/// wall-clock time per call, over however many iterations were requested.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub algo: Algo,
    pub per_call: Duration,
}

/// The literal meta-algorithm: benchmarks [`naive::encode`],
/// [`optimized::encode`], [`cow_optimized::encode`], and
/// [`fast_path::encode`] against **this exact `text`**, right now, and
/// returns their timings sorted fastest-first (`result[0]` is the winner).
///
/// Each candidate gets one untimed warm-up call (to normalize allocator
/// state) followed by `iterations` timed calls, whose total time is
/// divided by `iterations` to get [`Timing::per_call`].
/// [`std::hint::black_box`] is used on both the input and the output of
/// each call so the optimizer can't fold the loop away or skip work based
/// on the fact the result is unused.
///
/// # Cost
///
/// This function performs `iterations * 4` real encode calls before
/// returning. Use a modest `iterations` (a few hundred to a few thousand
/// are usually enough to get a stable ordering) and treat this as a
/// calibration step, not something to call on every request. You can see
/// [`encode_auto_tuned`] for a convenience wrapper and prefer
/// [`choose_encode_algo`] / [`encode`] for actual hot-path use once you
/// know which algorithm wins for your typical input shape.
///
/// # Examples
///
/// ```
/// use tagcode::meta;
///
/// let timings = meta::benchmark_encode("hello world", 200);
/// assert_eq!(timings.len(), 4);
/// // Fastest first:
/// assert!(timings[0].per_call <= timings[3].per_call);
/// ```
pub fn benchmark_encode(text: &str, iterations: u32) -> Vec<Timing> {
    let candidates: [AlgoLambda; 4] = [
        (Algo::Naive, |t| naive::encode(t).len()),
        (Algo::Optimized, |t| optimized::encode(t).len()),
        (Algo::CowOptimized, |t| cow_optimized::encode(t).len()),
        (Algo::FastPath, |t| fast_path::encode(t).len()),
    ];
    time_candidates(text, iterations, &candidates)
}

/// Same as [`benchmark_encode`], but for the four `decode` implementations.
///
/// # Examples
///
/// ```
/// use tagcode::meta;
///
/// let hidden = meta::encode("secret");
/// let timings = meta::benchmark_decode(&hidden, 200);
/// assert_eq!(timings.len(), 4);
/// ```
pub fn benchmark_decode(text: &str, iterations: u32) -> Vec<Timing> {
    let candidates: [AlgoLambda; 4] = [
        (Algo::Naive, |t| naive::decode(t).len()),
        (Algo::Optimized, |t| optimized::decode(t).len()),
        (Algo::CowOptimized, |t| cow_optimized::decode(t).len()),
        (Algo::FastPath, |t| fast_path::decode(t).len()),
    ];
    time_candidates(text, iterations, &candidates)
}

fn time_candidates(text: &str, iterations: u32, candidates: &[AlgoLambda; 4]) -> Vec<Timing> {
    let iterations = iterations.max(1);
    let mut results: Vec<Timing> = candidates
        .iter()
        .map(|&(algo, f)| {
            let _ = black_box(f(black_box(text))); // warm-up, not timed
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(f(black_box(text)));
            }
            Timing {
                algo,
                per_call: start.elapsed() / iterations,
            }
        })
        .collect();

    results.sort_by_key(|t| t.per_call);
    results
}

/// Benchmarks all four `encode` implementations on `text` (see
/// [`benchmark_encode`]), then actually encodes `text` using whichever one
/// won, returning both the output and which [`Algo`] produced it so
/// callers can log/inspect the decision.
///
/// # Examples
///
/// ```
/// use tagcode::meta;
///
/// let (hidden, winner) = meta::encode_auto_tuned("hello", 200);
/// println!("fastest for this input was: {}", winner.name());
/// assert_eq!(meta::decode(&hidden), "hello");
/// ```
pub fn encode_auto_tuned(text: &str, iterations: u32) -> (String, Algo) {
    let winner = benchmark_encode(text, iterations)[0].algo;
    let out = match winner {
        Algo::Naive => naive::encode(text),
        Algo::Optimized => optimized::encode(text),
        Algo::CowOptimized => cow_optimized::encode(text).into_owned(),
        Algo::FastPath => fast_path::encode(text),
    };
    (out, winner)
}

/// Decode counterpart to [`encode_auto_tuned`]: benchmarks all four
/// `decode` implementations on `text`, then decodes using the winner.
///
/// # Examples
///
/// ```
/// use tagcode::meta;
///
/// let hidden = meta::encode("hello");
/// let (revealed, winner) = meta::decode_auto_tuned(&hidden, 200);
/// println!("fastest for this input was: {}", winner.name());
/// assert_eq!(revealed, "hello");
/// ```
pub fn decode_auto_tuned(text: &str, iterations: u32) -> (String, Algo) {
    let winner = benchmark_decode(text, iterations)[0].algo;
    let out = match winner {
        Algo::Naive => naive::decode(text),
        Algo::Optimized => optimized::decode(text),
        Algo::CowOptimized => cow_optimized::decode(text).into_owned(),
        Algo::FastPath => fast_path::decode(text),
    };
    (out, winner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_dispatch_round_trips() {
        for s in ["hello", "日本語のみ", "", "mix café hi"] {
            assert_eq!(decode(&encode(s)), s);
        }
    }

    #[test]
    fn heuristic_picks_cow_for_no_op_encode() {
        assert_eq!(
            choose_encode_algo("日本語のみ、何も隠さない"),
            Algo::CowOptimized
        );
        assert_eq!(choose_encode_algo("hello"), Algo::Optimized);
    }

    #[test]
    fn heuristic_picks_cow_for_no_op_decode() {
        assert_eq!(choose_decode_algo("plain text"), Algo::CowOptimized);
        let hidden = encode("hello");
        assert_eq!(choose_decode_algo(&hidden), Algo::Optimized);
    }

    #[test]
    fn benchmark_encode_covers_all_four_and_sorts_ascending() {
        let timings = benchmark_encode("hello world", 50);
        assert_eq!(timings.len(), 4);
        let mut seen: Vec<Algo> = timings.iter().map(|t| t.algo).collect();
        seen.sort_by_key(|a| a.name());
        let mut expected = vec![
            Algo::Naive,
            Algo::Optimized,
            Algo::CowOptimized,
            Algo::FastPath,
        ];
        expected.sort_by_key(|a| a.name());
        assert_eq!(seen, expected);
        for w in timings.windows(2) {
            assert!(w[0].per_call <= w[1].per_call);
        }
    }

    #[test]
    fn auto_tuned_round_trips() {
        let (hidden, _winner) = encode_auto_tuned("some text to hide", 20);
        let (revealed, _winner2) = decode_auto_tuned(&hidden, 20);
        assert_eq!(revealed, "some text to hide");
    }
}
