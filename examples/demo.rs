//! Backs `just demo`: encodes/decodes a sample string with every strategy,
//! including the two `meta` dispatch modes, and prints what happened.

use tagcode::{cow_optimized, fast_path, meta, naive, optimized};

fn main() {
    let sample = "Hello, hidden world! 42";
    println!("input: {sample:?}\n");

    println!("-- fixed implementations --");
    for (name, hidden) in [
        ("naive", naive::encode(sample)),
        ("optimized", optimized::encode(sample)),
        ("cow_optimized", cow_optimized::encode(sample).into_owned()),
        ("fast_path", fast_path::encode(sample)),
    ] {
        println!(
            "{name:<14} encoded_len={:<4} round_trip_ok={}",
            hidden.chars().count(),
            match name {
                "naive" => naive::decode(&hidden) == sample,
                "optimized" => optimized::decode(&hidden) == sample,
                "cow_optimized" => cow_optimized::decode(&hidden) == sample,
                _ => fast_path::decode(&hidden) == sample,
            }
        );
    }

    println!("\n-- meta: cheap heuristic dispatch --");
    let hidden = meta::encode(sample);
    let picked = meta::choose_encode_algo(sample);
    println!("chosen algo: {}", picked.name());
    println!("round_trip_ok={}", meta::decode(&hidden) == sample);

    println!("\n-- meta: empirical auto-tuning (benchmarks all 4, picks the winner) --");
    let (hidden, winner) = meta::encode_auto_tuned(sample, 500);
    println!("measured winner: {}", winner.name());
    let (revealed, winner2) = meta::decode_auto_tuned(&hidden, 500);
    println!("measured winner: {}", winner2.name());
    println!("round_trip_ok={}", revealed == sample);

    println!("\nfull timing breakdown for this input:");
    for t in meta::benchmark_encode(sample, 500) {
        println!("  {:<14} {:?}/call", t.algo.name(), t.per_call);
    }
}
