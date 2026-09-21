#![feature(rustc_private)]

use std::time::Instant;
use rustc_hash::FxHashMap;

const DEFAULT_ITERS: u64 = 100_000;

const CALTIME_NANOS: u64 = 1_000_000;
const D: Duration = Duration::from_nanos(CALTIME_NANOS);

#[inline(never)]
pub fn dummy_func() -> i64 {
    // When I make this code a little faster/simpler then cputime on Macos starts telling me
    // that it took 0 nanoseconds. 🤔
    let mut a = Arc::new(0);
    for i in 0..30 {
        for j in 0..29 {
            *Arc::make_mut(&mut a) ^= black_box(i * j);
        }
    }

    *a
}

use std::sync::Arc;
use std::hint::black_box;
fn instant(_clock: Option<ClockType>, iters: u64) -> Vec<u64> {
    let mut durations = Vec::with_capacity(iters as usize);

    let mut i = 0;
    while i < iters {
        let inst = Instant::now();

        black_box(dummy_func());

        let d = inst.elapsed();
        assert!(d.as_nanos() > 0);
        durations.push(d.as_nanos() as u64);

        i += 1;
    }

    durations
}

#[cfg(windows)]
pub mod plat_windows {
    use windows_sys::Win32::System::Performance::QueryPerformanceCounter;
    use crate::{black_box, dummy_func, Instant, sleep, ClockType, DEFAULT_ITERS, D};
    
    pub fn qpc(_clock: Option<ClockType>) -> Vec<u64> {
        let iters = get_iters();
        let mut res = Vec::with_capacity(iters);
        let mut i = 0;

        while i < iters {
	    let mut start: i64 = 0;
	    let mut stop: i64 = 0;
	    let start_result = unsafe { QueryPerformanceCounter(&mut start) };

            black_box(dummy_func());

	    let stop_result = unsafe { QueryPerformanceCounter(&mut stop) };

	    assert!(start_result != 0);
	    assert!(stop_result != 0);
            assert!(stop > start);
            let ticks = stop as u64 - start as u64;

            res.push(ticks);

            i += 1;
        }

	res
    }

    /// Returns the number of qpc ticks per nanosecond, in (numer, denomer) format.
    /// Sleeps for about a millisecond in order to calibrate.
    pub fn qpc_calibrate(_clock: Option<ClockType>) -> (u64, u64) {
	let mut start: i64 = 0;
	let mut stop: i64 = 0;

	// let mut frequency: i64 = 0;
	// let freq_result = unsafe { QueryPerformanceFrequency(&mut frequency) };
	// assert!(freq_result != 0);

	let start_instant = Instant::now();
	let start_result = unsafe { QueryPerformanceCounter(&mut start) };

        sleep(D);

	let stop_result = unsafe { QueryPerformanceCounter(&mut stop) };
	let elap = start_instant.elapsed();

	assert!(elap.as_nanos() > 0);
	assert!(start_result != 0);
	assert!(stop_result != 0);
	assert!(start > 0);
	assert!(stop > 0);
        assert!(stop > start);

	let ticks = stop as u64 - start as u64;
	let nanos = elap.as_nanos() as u64;

	// println!("QueryPerformanceFrequency said the ticks per second is {}. Our calibration says the ticks per nanosecond is {}/{}", freq_result, ticks, nanos);

	(ticks, nanos)
    }

    pub fn increment_system_time() -> io::Result<()> {
        use winapi::um::sysinfoapi::{GetSystemTime, SetSystemTime};
        use winapi::shared::minwindef::SYSTEMTIME;

        // Read current time
        let mut system_time = SYSTEMTIME { wYear: 0, wMonth: 0, wDayOfWeek: 0, wDay: 0, wHour: 0, wMinute: 0, wSecond: 0, wMilliseconds: 0 };
        unsafe { GetSystemTime(&mut system_time); }

        // Add 1 second (don't bother to handle overflow)
        system_time.wSecond += 1;

        // Set new time
        unsafe {
            if SetSystemTime(&system_time) != 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }
}

#[cfg(target_vendor = "apple")]
pub mod plat_apple {
    use std::hint::black_box;
    use crate::{ClockType, dummy_func, D};
    extern crate libc;
    use libc::clockid_t;
    unsafe extern "C" {
        fn clock_gettime_nsec_np(clk_id: clockid_t) -> u64;
    }

    /// Returns the number of this clock's nanoseconds per Instant::now() nanoseconds, in (numer,
    /// denomer) format. Sleeps for about a millisecond in order to calibrate.
    pub fn gettime_nsec_np_clock_calibrate(clock: Option<ClockType>) -> (u64, u64) {
        let ct = clock.unwrap();

        let start_instant = Instant::now();
        let prev = unsafe { clock_gettime_nsec_np(ct) };
        sleep(D);
        let now = unsafe { clock_gettime_nsec_np(ct) };
        let elap = start_instant.elapsed().as_nanos() as u64;

        assert!(elap > 0);
        assert!(now > prev);

        let dur: u64 = now - prev;

        (dur, elap)
    }

    pub fn gettime_nsec_np_clock(clock: Option<ClockType>, iters: u64) -> Vec<u64> {
        let mut durations = Vec::with_capacity(iters as usize);
        let mut i = 0;
        let ct = clock.unwrap();
    
        while i < iters {
            let prev = unsafe { clock_gettime_nsec_np(ct) };

            black_box(dummy_func());

            let now = unsafe { clock_gettime_nsec_np(ct) };

            if now > prev {
                let dur: u64 = now - prev;

                durations.push(dur);
            }

            i += 1;
        }

        durations
    }

    use mach_sys::mach_time::mach_absolute_time;
    use std::time::Instant;
    use std::thread::sleep;

    /// Returns the number of this clock's ticks per Instant::now()'s nanoseconds, in (numer,
    /// denomer) format. Sleeps for about a millisecond in order to calibrate.
    pub fn mach_absolute_time_ticks_calibrate(_clock: Option<ClockType>) -> (u64, u64) {
        //let mut mtt1: MaybeUninit<mach_timebase_info> = MaybeUninit::uninit();
        //let retval = unsafe { mach_timebase_info(mtt1.as_mut_ptr()) };
        //assert_eq!(retval, KERN_SUCCESS);
        //let mtt2 = unsafe { mtt1.assume_init() };

        let start_instant = Instant::now();
        let t1 = unsafe { mach_absolute_time() };
        sleep(D);
        let t2 = unsafe { mach_absolute_time() };
        let elap = start_instant.elapsed().as_nanos() as u64;

        assert!(elap > 0);
        assert!(t2 > t1);
        let ticks = t2 - t1;

        //eprintln!("Mach kernel says that the ratio of mach ticks to nanoseconds is {}/{}. Our calibratiom says that the ratio is {ticks}/{elap}.", mtt2.denom, mtt2.numer);
        (ticks, elap)
    }

    pub fn mach_absolute_time_ticks(_clock: Option<ClockType>, iters: u64) -> Vec<u64> {
        //let mut mtt1: MaybeUninit<mach_timebase_info> = MaybeUninit::uninit();
        //let retval = unsafe { mach_timebase_info(mtt1.as_mut_ptr()) };
        //assert_eq!(retval, KERN_SUCCESS);
        //let mtt2 = unsafe { mtt1.assume_init() };

        //eprintln!("mach_timebase_info: {mtt2:?}");

        let mut durations = Vec::with_capacity(iters as usize);
        let mut i = 0;
    
        while i < iters {
            let t1 = unsafe { mach_absolute_time() };

            black_box(dummy_func());

            let t2 = unsafe { mach_absolute_time() };
            assert!(t2 > t1);

            let ticks = t2 - t1;
            assert!(ticks > 0);
            durations.push(ticks);

            i += 1;
        }

        durations
    }
}
    
#[cfg(target_vendor = "apple")]
type ClockType = u32;
#[cfg(target_vendor = "apple")]
type TvUsecType = i32;

#[cfg(any(target_os = "linux", target_os = "windows"))]
type ClockType = i32;
#[cfg(any(target_os = "linux", target_os = "windows"))]
type TvUsecType = i64;

use std::time::Duration;
use std::thread::sleep;

/// Returns the number of this clock's nanoseconds per Instant::now() nanoseconds, in (numer,
/// denomer) format. Sleeps for about a millisecond in order to calibrate. Note that it is using the
/// same clock for both of the measurements, so this is actually measuring nothing but the error in
/// our calibration tecnnique. :-} Most of the reason we are doing this at all is the insert a 1sec
/// delay before beginning the measurements of the clock, so that all of the measurements of clocks,
/// which are done on separate threads, will be running simultaneously.
fn instant_calibrate(_clock: Option<ClockType>) -> (u64, u64) {
    let start_instant1 = Instant::now();
    let start_instant2 = Instant::now();
    sleep(D);
    let elap2 = start_instant2.elapsed().as_nanos() as u64;
    let elap1 = start_instant1.elapsed().as_nanos() as u64;
    assert!(elap1 > 0);
    assert!(elap2 > 0);

    (elap2, elap1)
}

#[cfg(unix)]
pub mod plat_unixes {
    pub extern crate libc;
    use std::io::Error;
    use std::mem::MaybeUninit;
    use crate::{ClockType, D, Instant, sleep, black_box, dummy_func, TvUsecType};

    /// Returns the number of this clock's nanoseconds per Instant::now() nanoseconds, in (numer,
    /// denomer) format. Sleeps for about a millisecond in order to calibrate.
    pub fn libc_gettime_clock_calibrate(clock: Option<ClockType>) -> (u64, u64) {
	let ct = clock.unwrap();

	let mut tp1: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
	let mut tp2: MaybeUninit<libc::timespec> = MaybeUninit::uninit();

	let start_instant = Instant::now();
	let retval1 = unsafe { libc::clock_gettime(ct, tp1.as_mut_ptr()) };
	sleep(D);
	let retval2 = unsafe { libc::clock_gettime(ct, tp2.as_mut_ptr()) };
	let elap = start_instant.elapsed();
	assert!(elap.as_nanos() > 0);

	assert_eq!(retval1, 0);
	let instsec = unsafe { (*tp1.as_ptr()).tv_sec };
	let instnsec = unsafe { (*tp1.as_ptr()).tv_nsec };
	assert_eq!(retval2, 0);
	let newinstsec = unsafe { (*tp2.as_ptr()).tv_sec };
	let newinstnsec = unsafe { (*tp2.as_ptr()).tv_nsec };

	assert!(newinstsec * 1_000_000_000 + newinstnsec > instsec * 1_000_000_000 + instnsec, "newinstsec: {newinstsec}, newinstnsec: {newinstnsec}, instsec: {instsec}, instnsec: {instnsec}");
	let durnanosi64 = (newinstsec - instsec) * 1_000_000_000 + newinstnsec - instnsec;
	assert!(durnanosi64 > 0);
	let durnanos: u64 = durnanosi64.try_into().unwrap();
	(durnanos, elap.as_nanos() as u64)
    }

    pub fn libc_gettime_clock(clock: Option<ClockType>, iters: u64) -> Vec<u64> {
	let mut durations = Vec::with_capacity(iters as usize);
	let mut i = 0;
	let ct = clock.unwrap();
	
	while i < iters {
            let mut tp1: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
            let mut tp2: MaybeUninit<libc::timespec> = MaybeUninit::uninit();

            let retval1 = unsafe { libc::clock_gettime(ct, tp1.as_mut_ptr()) };

            black_box(dummy_func());

            let retval2 = unsafe { libc::clock_gettime(ct, tp2.as_mut_ptr()) };

            assert_eq!(retval1, 0);
            let instsec = unsafe { (*tp1.as_ptr()).tv_sec };
            let instnsec = unsafe { (*tp1.as_ptr()).tv_nsec };
            assert_eq!(retval2, 0);
            let newinstsec = unsafe { (*tp2.as_ptr()).tv_sec };
            let newinstnsec = unsafe { (*tp2.as_ptr()).tv_nsec };

            if newinstsec * 1_000_000_000 + newinstnsec > instsec * 1_000_000_000 + instnsec {
                let durnanosi64 = (newinstsec - instsec) * 1_000_000_000 + newinstnsec - instnsec;
                assert!(durnanosi64 > 0);
                let durnanos: u64 = durnanosi64.try_into().unwrap();
                assert!(durnanos > 0);

                durations.push(durnanos);
            }

            i += 1;
	}

	durations
    }

    use libc::{gettimeofday, settimeofday, timeval};

    /// Returns nanoseconds since epoch
    pub fn get_system_time() -> u64 {
        let mut tv = timeval { tv_sec: 0, tv_usec: 0 };
        unsafe {
            if gettimeofday(&mut tv as *mut timeval, std::ptr::null_mut()) != 0 {
                panic!();
            }
        }

        (tv.tv_sec * 1_000_000_000 + tv.tv_usec as i64 * 1000) as u64
    }

    pub fn set_system_time(nanos_since_epoch: u64) {
        let tv = timeval { tv_sec: nanos_since_epoch as i64 / 1_000_000_000, tv_usec: (nanos_since_epoch / 1000) as TvUsecType };
        
        unsafe {
            if settimeofday(&tv as *const timeval, std::ptr::null()) != 0 {
                eprintln!("You have to give this process super-user/admin privs for it to be able to set the system clock.");
                eprintln!("{}", Error::last_os_error());
                panic!();
            }
        }
    }

    /// Returns nanoseconds
    pub fn get_monotonic_raw() -> u64 {
        let mut tp: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
        let retval = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, tp.as_mut_ptr()) };
        assert_eq!(retval, 0);
        unsafe { (*tp.as_ptr()).tv_sec as u64 * 1_000_000_000 + (*tp.as_ptr()).tv_nsec as u64 }
    }

    pub fn increment_system_time() {
        set_system_time(get_system_time() + 1_000_000_000);
    }
}

#[cfg(target_arch = "x86_64")]
pub mod plat_x86_64 {
    use crate::{ClockType, dummy_func, D};
    use core::arch::x86_64;
    use std::hint::black_box;
    use std::thread::sleep;
    use std::time::Instant;

    pub fn rdtscp(_clock: Option<ClockType>, iters: u64) -> Vec<u64> {
        let mut aux = 0;

        let mut res = Vec::with_capacity(iters as usize);
        let mut i = 0;
        
        while i < iters {
            let now1 = unsafe { x86_64::__rdtscp(&mut aux) };

            black_box(dummy_func());

            let now2 = unsafe { x86_64::__rdtscp(&mut aux) };

            debug_assert!(now2 > now1);
            let ticks = now2 as u64 - now1 as u64;

            res.push(ticks);

            i += 1;
        }

        res
    }

    /// Returns the number of tsc ticks per nanosecond, in (numer, denomer) format.
    /// Sleeps for about a millisecond in order to calibrate.
    pub fn rdtscp_calibrate(_clock: Option<ClockType>) -> (u64, u64) {
        let mut aux = 0;
        let start_instant = Instant::now();
        let start_tsc = unsafe { x86_64::__rdtscp(&mut aux) };
        sleep(D);
        let end_tsc = unsafe { x86_64::__rdtscp(&mut aux) };
        let elap = start_instant.elapsed();
        assert!(end_tsc > start_tsc);
        assert!(elap.as_nanos() > 0);

        ((end_tsc - start_tsc).into(), elap.as_nanos() as u64)
    }
}

#[cfg(unix)]
fn jump_clock_forward_1_sec() {
    plat_unixes::increment_system_time()
}
#[cfg(windows)]
fn jump_clock_forward_1_sec() {
    plat_windows::increment_system_time()
}

#[cfg(unix)]
fn get_system_time() -> u64 {
    plat_unixes::get_system_time()
}

fn stats<F, G>(func: F, calibrate: G, clock: Option<ClockType>, fnname: &str, clockname: &str, scale: bool)
where
    F: Fn(Option<ClockType>, u64) -> Vec<u64>,
    G: Fn(Option<ClockType>) -> (u64, u64),
{
    let iters = get_iters();

    let (numer, denomer) = calibrate(clock);
    let durations = func(clock, iters);

    let mut map: FxHashMap<u64, u64> = FxHashMap::default();

    if scale {
        for dur in durations {
            let nanos = dur * denomer / numer;
            *map.entry(nanos).or_insert(0) += 1;
        }
    } else {
        for dur in durations {
            *map.entry(dur).or_insert(0) += 1;
        }
    }
    
    let mut pairs: Vec<(&u64, &u64)> = map.iter().collect();

    pairs.sort_by(|a, b| a.0.cmp(b.0));

    let mut perc50: u64 = 0;
    let mut perc95: u64 = 0;
    let min: u64 = *pairs[0].0;
    let max: u64 = *pairs[(*pairs).len()-1].0;

    let mut numsamples: u64 = 0;
    for (_nanos, num) in &pairs {
        numsamples += *num;
    }
    
    //println!("{:>10} {:>10} {:>10} {:>10}", "nanos", "#", "# < nanos", "# > nanos");
    //println!("{:>10} {:>10} {:>10} {:>10}", "-----", "-", "---------", "---------");
    let mut sumnanos = 0;
    let mut sumnums = 0;
    for (nanos, num) in &pairs {
        if (sumnums + *num >= numsamples * 95 / 100) && (sumnums < numsamples * 95 / 100) {
            //println!("95 percentile");
            perc95 = **nanos;
        }
        if (sumnums + *num >= numsamples * 50 / 100) && (sumnums < numsamples * 50 / 100) {
            //println!("50 percentile");
            perc50 = **nanos;
        }
        sumnanos += *nanos * *num;
        sumnums += *num;
    }
    assert!(sumnanos < i64::MAX as u64);
    let mean: i64 = (sumnanos / numsamples).try_into().unwrap();

    if perc50 == 0 {
        perc50 = *pairs[pairs.len()-1].0;
    }
    if perc95 == 0 {
        perc95 = *pairs[pairs.len()-1].0;
    }

    let mut sumsquares: f64 = 0f64;
    for (nanos, num) in pairs {
        let f64ns: f64 = *nanos as f64;
        let diff: f64 = f64ns - mean as f64;
        let sqdiff: f64 = diff.powf(2f64);
        let n: f64 = (*num) as f64;
        sumsquares += sqdiff * n;
//        eprintln!("xyz, sumsquares: {}, sqdiff: {}, n: {}", sumsquares as u128, sqdiff as u128, n as u128);
    }
    let stddev = (sumsquares / (numsamples - 1) as f64).sqrt();

    if scale {
        println!("{fnname:>38} {clockname:>21} {:>13} {:>7} {:>7} {:>11} {:>10} {:>17} {:>13} {:>10}", numsamples.separate_with_commas(), min.separate_with_commas(), perc50.separate_with_commas(), mean.separate_with_commas(), perc95.separate_with_commas(), max.separate_with_commas(), (stddev as u128).separate_with_commas(), "---");
    } else {
        let drift = numer as f64 / denomer as f64;
        println!("{fnname:>38} {clockname:>21} {:>13} {:>7} {:>7} {:>11} {:>10} {:>17} {:>13} {:>10.6}", numsamples.separate_with_commas(), min.separate_with_commas(), perc50.separate_with_commas(), mean.separate_with_commas(), perc95.separate_with_commas(), max.separate_with_commas(), (stddev as u128).separate_with_commas(), drift);
    }

}

use thousands::Separable;

use std::thread;

macro_rules! add_wrapped_fn {
    ($vec:expr, $func:path, $calibrate:path, $clock:expr, $scale:expr) => {
        $vec.push(move || {
            // Full stringified clock (e.g., "Some(libc::CLOCK_THREAD_CPUTIME_ID)")
            let clock_str = stringify!($clock);

            let mut pruned_clockname = clock_str;
            if let Some(s) = pruned_clockname.strip_prefix("Some(") { pruned_clockname = s; }
            if let Some(s) = pruned_clockname.strip_prefix("libc::") { pruned_clockname = s; }
            if let Some(s) = pruned_clockname.strip_prefix("CLOCK_") { pruned_clockname = s; }
            if let Some(s) = pruned_clockname.strip_suffix(")") { pruned_clockname = s; }
            if let Some(s) = pruned_clockname.strip_suffix("_ID") { pruned_clockname = s; }
            
            // Call stats with the pruned clock name
            stats($func, $calibrate, $clock, stringify!($func), pruned_clockname, $scale);
        });
    };
}

fn get_iters() -> u64 {
    let args: Vec<String> = env::args().collect();

    // Iterate over the arguments to find the "--iters" argument
    for arg in &args {
        if let Some(iters_str) = arg.strip_prefix("--iters=") {
            if let Ok(argiters) = iters_str.parse::<u64>() {
                return argiters;
            } else {
                panic!();
            }
        }
    }

    DEFAULT_ITERS
}

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

#[cfg(unix)]
fn get_monotonic_raw() -> u64 {
    plat_unixes::get_monotonic_raw()
}

#[cfg(unix)]
fn set_system_time(nanos: u64) {
    plat_unixes::set_system_time(nanos)
}

fn jump_clock_ahead_thread(should_exit: Arc<AtomicBool>) {
    let start_syst = get_system_time();
    let start_mont = get_monotonic_raw();

    sleep(D);

    while !should_exit.load(Ordering::Relaxed) {
        jump_clock_forward_1_sec();
    }

    // Set system time back to where it "ought" to be.
    let stop_mont = get_monotonic_raw();
    let new_syst = start_syst + stop_mont - start_mont;
    set_system_time(new_syst);
}

use std::env;
fn main() {
    let args: Vec<String> = env::args().collect();

    // `--pitfall[=MICROSECONDS]`: demonstrate the scheduler-accounting pitfall
    // on ~1 ms samples (or the given length) and exit.
    if let Some(arg) = args.iter().find(|a| a.starts_with("--pitfall")) {
        let micros: u64 = arg.strip_prefix("--pitfall=").map(|v| v.parse().expect("--pitfall=MICROSECONDS")).unwrap_or(1_000);
        let iters = get_iters().min(20_000) as usize;
        pitfall::run(micros * 1_000, iters);
        return;
    }

    let mut fns: Vec<fn()> = Vec::new();
    let mut clockmeasurementhandles = Vec::new();

    add_wrapped_fn!(fns, instant, instant_calibrate, None, false);

#[cfg(unix)]
    {
    use crate::plat_unixes::{libc, libc_gettime_clock, libc_gettime_clock_calibrate};
    add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_REALTIME), false);
    add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_MONOTONIC), false);
    add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_MONOTONIC_RAW), false);
    add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_PROCESS_CPUTIME_ID), false);
    add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_THREAD_CPUTIME_ID), false);
    }
#[cfg(target_vendor = "apple")]
    {
        // use crate::plat_unixes::{libc_gettime_clock, libc_gettime_clock_calibrate, libc};
        use crate::plat_unixes::{libc, libc_gettime_clock, libc_gettime_clock_calibrate};
        add_wrapped_fn!(fns, plat_apple::mach_absolute_time_ticks, plat_apple::mach_absolute_time_ticks_calibrate, None, true);
        add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_UPTIME_RAW), false);
        add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_UPTIME_RAW_APPROX), false);
        add_wrapped_fn!(fns, libc_gettime_clock, libc_gettime_clock_calibrate, Some(libc::CLOCK_MONOTONIC_RAW_APPROX), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_REALTIME), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_MONOTONIC), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_MONOTONIC_RAW), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_MONOTONIC_RAW_APPROX), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_UPTIME_RAW), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_UPTIME_RAW_APPROX), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_PROCESS_CPUTIME_ID), false);
        add_wrapped_fn!(fns, plat_apple::gettime_nsec_np_clock, plat_apple::gettime_nsec_np_clock_calibrate, Some(libc::CLOCK_THREAD_CPUTIME_ID), false);
    }
#[cfg(target_arch = "x86_64")]
    add_wrapped_fn!(fns, plat_x86_64::rdtscp, plat_x86_64::rdtscp_calibrate, None, true);
#[cfg(windows)]
    add_wrapped_fn!(fns, plat_windows::qpc, plat_windows::qpc_calibrate, None, true);


//    println!("iters: {}", iters.separate_with_commas());
    println!("{:>38} {:>21} {:>13} {:>7} {:>7} {:>11} {:>10} {:>17} {:>13} {:>10}", "fnname", "clock", "nsamples", "min", "perc50", "mean", "perc95", "max", "stddev", "drift");
    println!("{:>38} {:>21} {:>13} {:>7} {:>7} {:>11} {:>10} {:>17} {:>13} {:>10}", "------", "-----", "--------", "---", "------", "----", "------", "---", "------", "-----");

    let numthreadsperfunc = if args.contains(&"--overthread".to_string()) {
        let count = thread::available_parallelism().unwrap().get();
        assert!(count >= 1_usize);
        count * 2
    } else {
        1
    };

    for func in fns {
        for _i in 0..numthreadsperfunc {
            let handle = thread::spawn(func);
            clockmeasurementhandles.push(handle);
        }
    }

    if args.contains(&"--clockjumpahead".to_string()) {
        let should_exit = Arc::new(AtomicBool::new(false));

        let should_exit_clone = Arc::clone(&should_exit);
        let jumperhandle = thread::spawn(|| {
            jump_clock_ahead_thread(should_exit_clone);
        });

        for handle in clockmeasurementhandles {
            handle.join().unwrap();
        }

        should_exit.store(true, Ordering::Relaxed);

        jumperhandle.join().unwrap();
    } else {
        for handle in clockmeasurementhandles {
            handle.join().unwrap();
        }
    }

}

// ── Scheduler-accounting pitfall ────────────────────────────────────────────
//
// CPU-time clocks (CLOCK_THREAD_CPUTIME_ID, CLOCK_PROCESS_CPUTIME_ID) are
// scheduler accounting: the kernel bills time to a thread at context switches
// and timer ticks, and interpolates within the running slice. A hardware
// counter (CLOCK_UPTIME_RAW on Darwin, CLOCK_MONOTONIC on Linux) is a
// register read. The two behave differently when the thread is interrupted
// mid-measurement.
//
// This mode runs one fixed piece of work many times and reads BOTH clocks
// around each run. For every sample it records wall nanoseconds, CPU
// nanoseconds, and the ratio. A run of pure computation on one thread uses
// CPU time at exactly the wall rate while it runs, so the honest outcomes are:
//
//   cpu ≈ wall            the thread ran uninterrupted;
//   cpu < wall, by a lot  the thread was descheduled; wall counted the gap,
//                         CPU time did not (the case that motivates CPU clocks).
//
// The pitfall is the third outcome: cpu < wall by a modest, repeated amount
// on samples that show no sign of a deschedule (their wall time is at the
// distribution's minimum). Those are slices the accounting billed short. In
// a benchmark, such a sample reports the work running faster than the
// hardware allows; a wall-clock sample can only run long, which a median
// absorbs and a min–max band reports.
//
// The report groups samples by how far cpu falls below wall and shows the
// distribution of the CPU-clock reading itself: its minimum against its
// median tells whether the clock invented speed.

pub mod pitfall {
    use super::{black_box, Separable};
    use std::time::Instant;

    /// Deterministic work of about a millisecond: a hash-like integer loop
    /// with a data dependency, so it cannot be hoisted or vectorised away
    /// and its per-call cost is stable to well under a percent.
    #[inline(never)]
    fn work(rounds: u64) -> u64 {
        let mut a: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut b: u64 = 0x1234_5678_9ABC_DEF0;
        for i in 0..rounds {
            a = a.rotate_left(13) ^ b.wrapping_add(i);
            b = b.rotate_right(7).wrapping_mul(0x100_0000_01B3) ^ a;
        }
        black_box(a ^ b)
    }

    #[cfg(unix)]
    fn thread_cpu_ns() -> u64 {
        use super::plat_unixes::libc;
        let mut tp = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        let rc = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut tp) };
        assert_eq!(rc, 0);
        tp.tv_sec as u64 * 1_000_000_000 + tp.tv_nsec as u64
    }

    #[cfg(not(unix))]
    fn thread_cpu_ns() -> u64 {
        panic!("the pitfall mode reads CLOCK_THREAD_CPUTIME_ID and runs on Unix only");
    }

    struct Sample {
        wall_ns: u64,
        cpu_ns: u64,
    }

    fn percentile(sorted: &[u64], p: usize) -> u64 {
        sorted[(sorted.len() - 1) * p / 100]
    }

    pub fn run(target_ns: u64, iters: usize) {
        // Calibrate the work to about target_ns of wall time.
        let mut rounds: u64 = 1_000;
        loop {
            let t = Instant::now();
            black_box(work(rounds));
            let ns = t.elapsed().as_nanos() as u64;
            if ns >= target_ns / 8 {
                rounds = rounds * target_ns / ns.max(1);
                break;
            }
            rounds *= 4;
        }
        for _ in 0..16 {
            black_box(work(rounds));
        }

        // Competing threads: one busy loop per available core, so the
        // measuring thread is preempted and migrated the way it would be on
        // a machine doing other work. Without them the pitfall rarely fires.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let competitors: Vec<_> = (0..std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
            .map(|_| {
                let stop = stop.clone();
                std::thread::spawn(move || {
                    let mut x = 0u64;
                    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                        x = black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407));
                    }
                    x
                })
            })
            .collect();

        let mut samples: Vec<Sample> = Vec::with_capacity(iters);
        for _ in 0..iters {
            let cpu0 = thread_cpu_ns();
            let wall0 = Instant::now();
            black_box(work(rounds));
            let wall_ns = wall0.elapsed().as_nanos() as u64;
            let cpu_ns = thread_cpu_ns() - cpu0;
            samples.push(Sample { wall_ns, cpu_ns });
        }

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        for handle in competitors {
            let _ = handle.join();
        }

        let mut wall: Vec<u64> = samples.iter().map(|s| s.wall_ns).collect();
        let mut cpu: Vec<u64> = samples.iter().map(|s| s.cpu_ns).collect();
        wall.sort_unstable();
        cpu.sort_unstable();

        let wall_min = wall[0];
        let wall_med = percentile(&wall, 50);
        let cpu_min = cpu[0];
        let cpu_med = percentile(&cpu, 50);

        println!();
        println!("Scheduler-accounting pitfall: {} samples of ~{} µs of fixed work, wall (Instant) and CPU (CLOCK_THREAD_CPUTIME_ID) read around each,",
            iters.separate_with_commas(), target_ns / 1_000);
        println!("with one competing busy thread per core so the measuring thread is preempted and migrated as on a busy machine.");
        println!();
        println!("{:>22} {:>12} {:>12} {:>12} {:>12} {:>12}", "clock", "min", "perc50", "perc95", "max", "min/perc50");
        println!("{:>22} {:>12} {:>12} {:>12} {:>12} {:>12}", "-----", "---", "------", "------", "---", "----------");
        for (name, v) in [("wall (hardware ctr)", &wall), ("thread CPU time", &cpu)] {
            let min = v[0];
            let med = percentile(v, 50);
            println!("{:>22} {:>12} {:>12} {:>12} {:>12} {:>11}%",
                name,
                min.separate_with_commas(),
                med.separate_with_commas(),
                percentile(v, 95).separate_with_commas(),
                v[v.len() - 1].separate_with_commas(),
                min * 100 / med);
        }
        println!();

        // Classify each sample by cpu relative to its own wall reading.
        let mut uninterrupted = 0usize;      // cpu within 1% of wall
        let mut descheduled = 0usize;        // wall ≥ 1.5× wall_min: a real gap
        let mut under_billed = 0usize;       // wall near its min, yet cpu < 0.97 wall
        let mut cpu_over_wall = 0usize;      // impossible: cpu > wall by > 1%
        let mut under_billed_ratio_permille: Vec<u64> = Vec::new();
        for s in &samples {
            let near_min_wall = s.wall_ns <= wall_min + wall_min / 20; // within 5% of the fastest run
            if s.cpu_ns > s.wall_ns + s.wall_ns / 100 {
                cpu_over_wall += 1;
            } else if s.wall_ns >= wall_min + wall_min / 2 {
                descheduled += 1;
            } else if near_min_wall && s.cpu_ns * 100 < s.wall_ns * 97 {
                under_billed += 1;
                under_billed_ratio_permille.push(s.cpu_ns * 1000 / s.wall_ns);
            } else {
                uninterrupted += 1;
            }
        }
        under_billed_ratio_permille.sort_unstable();

        println!("Per-sample classification (cpu vs the same sample's wall reading):");
        println!("  {:>7}  uninterrupted: cpu ≈ wall", uninterrupted.separate_with_commas());
        println!("  {:>7}  descheduled: wall ≥ 1.5× its minimum; CPU time correctly omits the gap", descheduled.separate_with_commas());
        println!("  {:>7}  UNDER-BILLED: wall within 5% of its minimum (no deschedule), yet cpu < 97% of wall", under_billed.separate_with_commas());
        if !under_billed_ratio_permille.is_empty() {
            let n = under_billed_ratio_permille.len();
            println!("           cpu/wall on those: min {}‰, median {}‰, max {}‰",
                under_billed_ratio_permille[0], under_billed_ratio_permille[n / 2], under_billed_ratio_permille[n - 1]);
        }
        println!("  {:>7}  cpu > wall by more than 1% (impossible for a single thread; indicates clock skew or bad interpolation)", cpu_over_wall.separate_with_commas());
        println!();

        let wall_floor_permille = wall_min * 1000 / wall_med;
        let cpu_floor_permille = cpu_min * 1000 / cpu_med;
        println!("Verdict for benchmarking:");
        println!("  wall clock minimum is {}‰ of its median; thread CPU minimum is {}‰ of its median.",
            wall_floor_permille, cpu_floor_permille);
        if cpu_floor_permille + 20 < wall_floor_permille {
            println!("  The CPU clock reports the fixed work finishing faster than the hardware counter ever saw it finish.");
            println!("  Those samples are under-billed slices, not faster runs. A benchmark's minimum and min–max band");
            println!("  built on this clock will show speed the machine did not deliver. Prefer the hardware counter.");
        } else {
            println!("  On this run the CPU clock's floor matches the hardware counter's; the pitfall did not appear here.");
            println!("  It appears under load and on machines where the scheduler moves threads between cores; rerun");
            println!("  with --overthread or alongside other work to provoke it.");
        }
    }
}
