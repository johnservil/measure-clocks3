# The scheduler-accounting pitfall

## Summary

`CLOCK_THREAD_CPUTIME_ID` looks like the ideal benchmark clock: it counts
only the time the measured thread ran, so a preemption mid-sample leaves
no trace. The main table in this repository shows that on an Apple M4
Max it has the same median as every wall clock with a 4–8× smaller
standard deviation, and the summary recommends it for single-threaded
benchmarks.

That recommendation held for 1.4 µs samples. On ~1 ms samples, in a
benchmark that ran three SHA-256 implementations interleaved, the thread
CPU clock produced a result the hardware cannot have delivered: all three
implementations shared one minimum, 12% below their own steady medians.
The same benchmark under the hardware counter showed no such floor.

The cause is what the two kinds of clock *are*. A wall clock is a counter
read. Thread CPU time is scheduler accounting: a ledger the kernel updates
at context switches, with the running slice interpolated. A counter can
only over-count when the thread is interrupted, and a median absorbs that
while a min–max band reports it. A ledger can under-bill a slice, and then
the sample reports the work finishing faster than it did. For a benchmark
that is the worse failure: an impossibly fast minimum looks like a result.

**For benchmarks, read a hardware counter.** On Darwin that is
`CLOCK_UPTIME_RAW` (`mach_absolute_time` in nanoseconds), on Linux
`CLOCK_MONOTONIC`; `std::time::Instant` gives each. Both are free of NTP
slew and stop while the machine sleeps.

## The observation

bench-hashes (github.com/johnservil/bench-hashes) times each
(implementation, input size) cell 80 times, ~1 ms per sample, with the
contenders interleaved in a balanced order. Two runs on the same M4 Max,
twenty minutes apart, differing only in the sample clock.

Under the thread CPU clock (`clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)`),
minimum–median–maximum in ns/B:

```
           sha2 crate           CommonCrypto         ring
16 KiB     0.337–0.382–0.393    0.290–0.328–0.337    0.290–0.327–0.337
32 KiB     0.337–0.382–0.396    0.288–0.327–0.345    0.288–0.327–0.334
64 KiB     0.337–0.382–0.392    0.288–0.327–0.338    0.288–0.326–0.337
```

Under the hardware counter (`Instant`, `CLOCK_UPTIME_RAW`):

```
16 KiB     0.368–0.383–0.393                         0.333–0.335–0.397
64 KiB     0.364–0.382–0.409                         0.323–0.329–0.341
```

Three points. The medians agree between clocks to the third digit, so
both clocks measure the same thing on an undisturbed sample. The CPU
clock's minima sit 12% under those medians for every implementation at
once, at the same sizes — different code, different libraries, one
floor. And the hardware counter's minima stay within 4% of the median.
The 12% is an artifact of the clock, not of the hashing.

## The mechanism, from XNU

On Apple silicon, thread CPU time comes from `osfmk/kern/recount.c`.
Three details matter.

Time is tracked per CPU kind. `recount_thread_plan` uses
`RCT_TOPO_CPU_KIND`: a thread has one track for performance cores and
one for efficiency cores, and its CPU time is the sum. A thread's ledger
is updated when it switches off a processor (`recount_switch_thread`),
which diffs the processor's last snapshot against the current one and
absorbs the slice into the thread's track for that processor's kind.

The current slice is interpolated. `recount_current_thread_usage`
takes a fresh snapshot, sums the thread's tracks, and adds the diff
between the snapshot and the processor's last recorded one. The
timestamp comes from `ml_get_speculative_timebase()`: a timebase read
without the ISB barrier that `mach_absolute_time` applies. The comment
in the source names it speculative.

Migration splits a slice. When the scheduler moves the thread between a
P-core and an E-core during a sample, two `recount_switch_thread` calls
absorb two partial slices into two different tracks, reconciled from
two processors' last snapshots.

A sample that straddles a migration is therefore assembled from three
pieces read on two cores against per-processor baselines, with the
running piece from an unbarriered read. That is where a slice can come
up short. The hardware counter reads one register once at each end.

Linux tracks thread runtime as a single accumulator updated from one
clock; its `CLOCK_THREAD_CPUTIME_ID` did not show this behaviour under
test (below).

## Demonstrating it: `--pitfall`

```sh
cargo +nightly run --release -- --pitfall --iters=10000
```

Runs a fixed ~1 ms integer workload thousands of times, reading both
`Instant` and `CLOCK_THREAD_CPUTIME_ID` around each run, with one busy
thread per core competing so the measuring thread is preempted and
migrated as on a busy machine. `--pitfall=250` uses 250 µs samples.

Each sample is classified by its CPU reading against its own wall
reading:

- **uninterrupted**: cpu within 1% of wall;
- **descheduled**: wall at least 1.5× its minimum, so the thread lost
  the core; CPU time correctly omits the gap (this is the case that
  motivates CPU clocks, and it is fine);
- **UNDER-BILLED**: wall within 5% of its minimum, so the thread was
  not descheduled, yet cpu under 97% of wall — a slice billed short;
- **cpu > wall**: impossible for one thread; a skewed or badly
  interpolated read.

The verdict compares each clock's minimum to its median. A CPU-clock
floor well under the hardware counter's floor means the CPU clock
reported the fixed work finishing faster than the counter ever saw it
finish.

### Results so far

**Linux, 2-vCPU AArch64 VM (Apple M4 Max host), kernel 6.18**: no
under-billed samples in 9,000, with 25% of samples preempted. Both
clocks' floors at 88% of median (the workload's own variation). Linux's
thread clock is honest here.

**macOS, Apple M4 Max**: the anomaly above was observed in bench-hashes.
The `--pitfall` mode has not yet been run on this machine; its output
belongs here.

## What a benchmark should do

Use the hardware counter. Accept that a preempted sample runs long, and
report it: a median over enough samples is unmoved, and a min–max band
that widens *upward* under load tells the truth about the run. A band
that widens *downward* under a CPU clock tells a falsehood, and it is
the falsehood a reader most wants to believe.
