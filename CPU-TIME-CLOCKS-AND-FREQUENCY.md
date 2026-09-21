# Thread CPU-time clocks, examined; and the variable that was actually moving

## Summary

This repository's main table shows that on an Apple M4 Max,
`CLOCK_THREAD_CPUTIME_ID` has the same median as every wall clock with a
4–8× smaller standard deviation over 1.4 µs samples, and recommends it
for single-threaded benchmarks. A benchmark that adopted it on the same
machine then produced a result that looked like the clock inventing
speed: three SHA-256 implementations sharing one minimum, 12% below
their steady medians, at four consecutive input sizes.

Two experiments later, the clock is exonerated. `--pitfall` ran ten
thousand 1 ms samples under contention that preempted a fifth of them:
the CPU clock and the hardware counter agreed on every sample but one,
which differed by 3.7%. A per-sample trace of the benchmark itself, on
the same machine, showed the two clocks agreeing to within 2% on all
4,608 samples. XNU's thread-time accounting is correct at this scale.

What moved was the processor. The affected cells were not a set of
sizes but a set of *consecutive samples* — those four sizes are visited
in sequence in every round — and every sample in the window was scaled
by the same factor. Three different implementations do not share a
cache story, but they share a clock frequency. For about twelve
milliseconds the core ran roughly 12% faster than it did for the rest
of the run, both clocks measured it faithfully, and the benchmark
reported the truth: a minimum 12% under the median.

The lesson is the one this repository exists to teach, turned around.
The clock was not the uncontrolled variable. Frequency was, and no clock
can see it. A cycle counter can.

## The observation, in full

bench-hashes (github.com/johnservil/bench-hashes) times each
(implementation, input size) cell ~80 times at ~1 ms per sample,
contenders interleaved in a balanced order, sizes rotated per round.
Run at 15:28 on an M4 Max with `clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)`
as the sample clock; minimum–median–maximum in ns/B:

```
           sha2 crate           CommonCrypto         ring
 8 KiB     0.358–0.383–0.395    0.328–0.331–0.340    0.321–0.328–0.338
16 KiB     0.337–0.382–0.393    0.290–0.328–0.337    0.290–0.327–0.337
32 KiB     0.337–0.382–0.396    0.288–0.327–0.345    0.288–0.327–0.334
64 KiB     0.337–0.382–0.392    0.288–0.327–0.338    0.288–0.326–0.337
128 KiB    0.376–0.382–0.400    0.297–0.327–0.340    0.288–0.326–0.338
256 KiB    0.380–0.382–0.397    0.325–0.326–0.355    0.325–0.326–0.355
```

Minimum as a fraction of median: 88.2%, 88.4%, 88.7% at 16 KiB; 88.2%,
88.1%, 88.1% at 32 KiB; 88.2%, 88.1%, 88.3% at 64 KiB; then 98.4%,
90.8%, 88.3% at 128 KiB, and 99.5% everywhere else. One factor, twelve
samples, tailing off at the thirteenth.

The medians agree with the wall-clock run twenty minutes earlier to the
third digit. Whatever happened, happened to the extremes only.

## Why the shape rules out the clock

In each round the sizes are visited in rotation, three contenders per
size. Sixteen, 32, 64, and 128 KiB are therefore adjacent in time:
twelve samples, about twelve milliseconds. A phenomenon that touches
one sample in each of nine cells by the same factor, with the cells
adjacent in time and the factor fading at the boundary, is one event
of about twelve milliseconds' duration during which everything ran at
88% of its usual time.

An accounting error would not do that. A slice billed short is one
sample, off by however much was lost, not twelve samples off by one
ratio. An accounting *rate* error (the clock advancing at 88% of real
time for a window) was the hypothesis that fit the shape — and the
trace ruled it out: with `Instant` as the primary clock and the thread
clock read alongside, the two agreed on every sample. The thread clock
does not run at a different rate.

## The experiments

**`--pitfall`** (in this repository): 10,000 samples of ~1 ms fixed
integer work with one busy thread per core competing, both clocks read
around each sample. M4 Max: 8,131 samples uninterrupted, 1,868
descheduled (wall ≥ 1.5× its minimum, CPU time correctly omitting the
gap), 1 under-billed by 3.7%, 0 impossible. Both clocks' floors within
0.3% of their medians. Linux, 2-vCPU VM: 9,000 samples, 25% preempted,
0 under-billed.

**bench-hashes `--trace-clocks`**: the benchmark's own loop, `Instant`
as the sample clock, thread CPU, process CPU, and `mach_absolute_time`
read around every sample. M4 Max, same contenders and round count as
the 15:28 run: 4,608 samples, 0 with |cpu/wall − 1| > 2%.
`mach_absolute_time` ticks per wall nanosecond 0.024001 (nominal 3/125).
The one wide cell (ring at 128 B, max 1.35× median) has cpu/wall of
1.000 on that sample: a real preemption, honestly reported.

## What to do about frequency

A time measurement cannot distinguish "the code ran faster" from "the
core ran faster". Cycles can. On Apple silicon,
`thread_selfcounts(THSC_TIME_CPI_PER_PERF_LEVEL, …)` returns the calling
thread's cycles, instructions, and CPU time split by performance level
(P-cores and E-cores). Cycles divided by time is the frequency the
thread ran at during the sample; the split says which cluster ran it;
instructions over cycles is IPC, which changes when the code or its
cache behaviour changes and stays put when only the frequency does.

bench-hashes now records these per sample under `--trace-clocks`, and
`tools/analyze-clock-trace.py` reports the frequency distribution, the
core placement, the frequency at each cell's fastest and slowest
sample, and windows of consecutive samples running above or below the
median frequency. The 15:28 excursion, if it recurs, will show as a
~12 ms window of consecutive samples at a frequency 12% above the
median, on P-cores, with IPC unchanged.

Until it does, the recommendation for a benchmark of this kind is: read
the hardware counter (`CLOCK_UPTIME_RAW` on Darwin, `CLOCK_MONOTONIC` on
Linux; `std::time::Instant` gives both), report the median and the
min–max band, and treat a band that widens by a round fraction across
unrelated contenders at once as a frequency event, not a measurement
error. Where cycle counters are available, record them beside the time.

## On the original recommendation

For samples of a microsecond or two, the thread CPU clock's advantage
is real and this repository's table stands: a preemption is a
thousand-fold outlier at that scale, and the CPU clock removes it. For
samples of a millisecond, a preemption is a 5–20% outlier that a median
over eighty samples absorbs, and the two clocks agree on every
undisturbed sample. The hardware counter is then the simpler
instrument with no accounting layer to reason about, and it cannot
misreport in the direction a reader most wants to believe.
