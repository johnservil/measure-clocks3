# The scheduler-accounting pitfall

## Summary

Status: an anomaly observed once and recorded, a proposed mechanism
tested and ruled out, the trigger still open. The practical
recommendation stands on the observation alone.

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

The two kinds of clock differ in what they *are*. A wall clock is a
counter read. Thread CPU time is scheduler accounting: a ledger the
kernel updates at context switches, with the running slice interpolated.
A counter can only over-count when the thread is interrupted, and a
median absorbs that while a min–max band reports it. A ledger can in
principle under-bill a slice, and then the sample reports the work
finishing faster than it did. For a benchmark that is the worse
failure: an impossibly fast minimum looks like a result. The
observation below is such a minimum; what produced it is not yet known.

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

## What is known about the mechanism

Thread CPU time on Apple silicon comes from `osfmk/kern/recount.c`. A
thread's time is kept per CPU kind (`RCT_TOPO_CPU_KIND`: one track for
performance cores, one for efficiency cores) and reconciled at each
context switch by `recount_switch_thread`, which diffs the processor's
last snapshot against the current one. A read of the clock sums the
tracks and adds the running slice, timestamped with
`ml_get_speculative_timebase()`, a timebase read without the ISB barrier
that `mach_absolute_time` applies.

That structure makes under-billing *possible* in principle, and the
first hypothesis was that migration between cores mid-sample was the
trigger. The `--pitfall` test below was built to provoke exactly that,
and on the M4 Max it did not: under contention heavy enough to preempt
19% of samples, the thread clock under-billed one sample in ten
thousand, by 3.7%. Preemption and migration under load are accounted
correctly.

So the trigger in the bench-hashes run is something else, and it is
still open. Two facts about that run narrow it: the floor appeared
only at 16 KiB through 128 KiB, with smaller and larger inputs clean;
and the process was running six hash implementations interleaved,
including two that dispatch into system libraries. What differs about
those sizes or that mix has not been identified.

Linux tracks thread runtime as a single accumulator from one clock; its
`CLOCK_THREAD_CPUTIME_ID` showed no under-billing under the same test.

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

**Linux, 2-vCPU AArch64 VM (Apple M4 Max host), kernel 6.18**, 9,000
samples, 25% preempted: zero under-billed. Both floors at 88% of median
(the workload's own variation in the VM).

**macOS, Apple M4 Max**, 10,000 samples, 19% preempted, 33,929
involuntary context switches: one under-billed sample (cpu/wall 963‰).
Wall floor 998‰ of median, CPU floor 997‰. The test does not reproduce
the bench-hashes anomaly on this machine.

```
                 clock          min       perc50       perc95          max   min/perc50
   wall (hardware ctr)    1,027,000    1,029,042    1,802,958   17,528,042          99%
       thread CPU time    1,027,166    1,029,250    1,673,917    1,777,292          99%

    8,131  uninterrupted
    1,868  descheduled
        1  UNDER-BILLED (963‰)
        0  cpu > wall
```

The anomaly is therefore real (it is in a saved result file, with the
control run twenty minutes earlier showing none of it) and its trigger
is unknown. The next step is to reproduce it in bench-hashes itself
with both clocks recorded per sample, so the offending samples can be
inspected rather than inferred.

## What a benchmark should do

Use the hardware counter. Accept that a preempted sample runs long, and
report it: a median over enough samples is unmoved, and a min–max band
that widens *upward* under load tells the truth about the run. A band
that widens *downward* under a CPU clock tells a falsehood, and it is
the falsehood a reader most wants to believe.
