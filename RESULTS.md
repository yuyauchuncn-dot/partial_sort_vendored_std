# Short-prefix probe: `partial_sort_unstable` vs heap-based `partial_sort`

Measurements taken 2026-10-15 on Apple M1 (aarch64-apple-darwin, fanless
MacBook Air), against the vendored std sort code in this repo and
`partial_sort` 1.0.0.

Reproduce with:

```
RUSTC_BOOTSTRAP=1 cargo run --release --example short_prefix_probe
```

`RUSTC_BOOTSTRAP=1` is needed because this repo vendored std code that uses
feature gates, and `rust-toolchain.toml` is ignored when rustup is not
installed.

## Headline

At `len = 10000`, 1 KiB elements, `k = 2`, the std path performs **1.95x** the
comparisons of the heap path but takes **29x** the time. With `u64` — identical
comparison cost (`Block::cmp` reads only `data[0]`), 128x smaller payload — the
same point is only 2.7x. The gap is element movement, not comparison work.

std's cost is also close to flat in `k` over this range, while the heap path
grows with `k`. The `k = 1` row is the exception: it takes the
`end - start <= 1` early path through `partition_at_index(v, start)` ->
`min_index`, a single scan with no partitioning, and lands at rough parity.
`k >= 2` enters `partition_at_index_loop`.

## Timing, `len = 10000`

1 KiB elements:

| k | std (us) | heap (us) | heap/std |
|---|---|---|---|
| 1 | 18.3 | 16.3 | 0.89 |
| 2 | 485.1 | 16.5 | 0.03 |
| 8 | 487.5 | 29.7 | 0.06 |
| 32 | 490.0 | 63.0 | 0.13 |
| 128 | 502.9 | 217.4 | 0.43 |
| 256 | 531.8 | 407.9 | 0.77 |

`u64` elements:

| k | std (us) | heap (us) | heap/std |
|---|---|---|---|
| 1 | 10.4 | 5.3 | 0.51 |
| 2 | 15.3 | 5.7 | 0.37 |
| 8 | 15.3 | 6.7 | 0.44 |
| 32 | 16.0 | 10.3 | 0.64 |
| 128 | 16.0 | 19.4 | 1.21 |
| 256 | 16.8 | 26.4 | 1.57 |

## Comparison counts, 1 KiB, `len = 10000`, untimed pass

| k | std cmps | heap cmps | heap/std |
|---|---|---|---|
| 1 | 9999 | 9999 | 1.00 |
| 2 | 19497 | 10016 | 0.51 |
| 8 | 19578 | 10240 | 0.52 |
| 32 | 19777 | 11558 | 0.58 |
| 128 | 20384 | 17650 | 0.87 |
| 256 | 21926 | 25139 | 1.15 |

## Crossover

First `k` at which the std path is faster than the heap path:

| len | u64 | 1 KiB |
|---|---|---|
| 1000 | 32 | 64 |
| 10000 | 128 | > 256 |
| 100000 | > 256 | > 256 |

## Caveats

The machine was not idle — background load average around 2.6 during the runs.
Repeating three times moved absolute times by up to 50%. The ratios were stable
to about 15%; the absolute microseconds were not. Two of the three runs agreed
closely and the tables above are from one of those.

Inputs come from a xorshift generator, not `rand`, so this is not a
like-for-like reproduction of the benchmark in
[niklasf/partial_sort_vendored_std](https://github.com/niklasf/partial_sort_vendored_std).
Method is best-of-N wall time with the input clone outside the timed region,
rather than criterion.

The mechanism — that the cost is a fixed full-slice partition independent of
`k` — is inferred from the timing and comparison data above. No byte-movement
or swap-count instrumentation was done to confirm it directly.
