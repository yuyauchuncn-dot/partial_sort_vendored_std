//! Focused probe: short-prefix regime of `partial_sort_unstable`.
//! Compares vendored std partial_sort (ipnsort/quickselect path) against the
//! heap-based partial_sort 1.0.0 crate.
//!
//! Run: RUSTC_BOOTSTRAP=1 cargo run --release --example short_prefix_probe

use partial_sort_1_0_0 as heap_ps;
use partial_sort_vendored_std as vstd;
use std::cmp::Ordering;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering as AOrd};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- element types

/// 1 KiB payload. `cmp` reads only the first u64, so comparison cost is
/// identical to a plain u64 — only the payload size differs.
#[derive(Clone)]
#[repr(C)]
struct Block([u64; 128]);

impl Ord for Block {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0[0].cmp(&o.0[0])
    }
}
impl PartialOrd for Block {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl PartialEq for Block {
    fn eq(&self, o: &Self) -> bool {
        self.0[0] == o.0[0]
    }
}
impl Eq for Block {}

static CMPS: AtomicUsize = AtomicUsize::new(0);

/// Same payload as Block, but every comparison bumps a global counter.
#[derive(Clone)]
#[repr(C)]
struct CountedBlock([u64; 128]);

impl Ord for CountedBlock {
    fn cmp(&self, o: &Self) -> Ordering {
        CMPS.fetch_add(1, AOrd::Relaxed);
        self.0[0].cmp(&o.0[0])
    }
}
impl PartialOrd for CountedBlock {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl PartialEq for CountedBlock {
    fn eq(&self, o: &Self) -> bool {
        self.0[0] == o.0[0]
    }
}
impl Eq for CountedBlock {}

// ---------------------------------------------------------------- input gen

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

const SEED: u64 = 0x243F_6A88_85A3_08D3;

fn gen_u64(len: usize) -> Vec<u64> {
    let mut r = Rng(SEED | 1);
    (0..len).map(|_| r.next()).collect()
}

fn gen_block(len: usize) -> Vec<Block> {
    let mut r = Rng(SEED | 1);
    (0..len).map(|_| Block([r.next(); 128])).collect()
}

fn gen_counted(len: usize) -> Vec<CountedBlock> {
    let mut r = Rng(SEED | 1);
    (0..len).map(|_| CountedBlock([r.next(); 128])).collect()
}

// ---------------------------------------------------------------- timing

/// Best-of-N wall time. Input cloning happens outside the timed region.
fn best_of<T, F>(input: &[T], reps: usize, mut f: F) -> Duration
where
    T: Clone + Ord,
    F: FnMut(&mut Vec<T>),
{
    let mut best = Duration::MAX;
    for _ in 0..reps {
        let mut v = input.to_vec();
        let t = Instant::now();
        f(&mut v);
        let e = t.elapsed();
        black_box(&v);
        if e < best {
            best = e;
        }
    }
    best
}

fn reps_for(len: usize) -> usize {
    (1_000_000 / len).clamp(10, 300)
}

const KS: &[usize] = &[1, 2, 4, 8, 16, 32, 64, 128, 256];
const LENS: &[usize] = &[1_000, 10_000, 100_000];

fn main() {
    println!("# short-prefix probe — Apple M1 (aarch64-apple-darwin)");
    println!("# ratio = heap/std;  ratio > 1 means vendored std is FASTER");
    println!("# best-of-N wall time, clone excluded from timing");
    println!();

    println!("## u64 elements");
    println!("{:>8} {:>6} {:>12} {:>12} {:>9}", "len", "k", "std_us", "heap_us", "ratio");
    for &len in LENS {
        let u = gen_u64(len);
        let reps = reps_for(len);
        for &k in KS {
            if k > len {
                continue;
            }
            let s = best_of(&u, reps, |v| {
                vstd::partial_sort(v, ..k, |a, b| a.lt(b));
            });
            let h = best_of(&u, reps, |v| {
                heap_ps::partial_sort(v, k, |a, b| a.lt(b));
            });
            let (su, hu) = (s.as_secs_f64() * 1e6, h.as_secs_f64() * 1e6);
            println!("{:>8} {:>6} {:>12.2} {:>12.2} {:>9.2}", len, k, su, hu, hu / su);
        }
        println!();
    }

    println!("## 1 KiB elements (cmp cost identical to u64)");
    println!("{:>8} {:>6} {:>12} {:>12} {:>9}", "len", "k", "std_us", "heap_us", "ratio");
    for &len in LENS {
        let b = gen_block(len);
        let reps = reps_for(len);
        for &k in KS {
            if k > len {
                continue;
            }
            let s = best_of(&b, reps, |v| {
                vstd::partial_sort(v, ..k, |a, b| a.lt(b));
            });
            let h = best_of(&b, reps, |v| {
                heap_ps::partial_sort(v, k, |a, b| a.lt(b));
            });
            let (su, hu) = (s.as_secs_f64() * 1e6, h.as_secs_f64() * 1e6);
            println!("{:>8} {:>6} {:>12.2} {:>12.2} {:>9.2}", len, k, su, hu, hu / su);
        }
        println!();
    }

    // ------------------------------------------------------------ comparison counts
    // Single untimed run each: are the two algorithms even doing comparable
    // amounts of *comparison* work? If yes, the time gap is memory movement.
    println!("## comparison counts, 1 KiB, len=10000, single untimed run");
    println!("{:>6} {:>14} {:>14} {:>9}", "k", "std_cmps", "heap_cmps", "heap/std");
    let len = 10_000usize;
    for &k in KS {
        if k > len {
            continue;
        }
        let mut v = gen_counted(len);
        CMPS.store(0, AOrd::Relaxed);
        vstd::partial_sort(&mut v, ..k, |a, b| a.lt(b));
        let sc = CMPS.load(AOrd::Relaxed);

        let mut v = gen_counted(len);
        CMPS.store(0, AOrd::Relaxed);
        heap_ps::partial_sort(&mut v, k, |a, b| a.lt(b));
        let hc = CMPS.load(AOrd::Relaxed);

        black_box(&v);
        println!("{:>6} {:>14} {:>14} {:>9.2}", k, sc, hc, hc as f64 / sc as f64);
    }
}
