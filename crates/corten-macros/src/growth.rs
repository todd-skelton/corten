//! A test's check that a case runs in about linear time, by its growth rather than the clock.

use std::time::Duration;

/// `case` at size `n` runs in about linear time. Under a second is (the quadratic version of
/// each case that uses it takes far longer); slower, on a busy machine, it is checked by its
/// growth instead: at `n` about four times as long as at `n / 4`, not sixteen, the fastest of two
/// runs at each size, so the load slows both alike rather than failing a bound on the clock. Load
/// that slows only the smaller runs can let a slow case pass, never fail a fast one.
pub(crate) fn linear(name: &str, n: usize, case: impl Fn(usize)) {
    let time = |n: usize| {
        let started = std::time::Instant::now();
        case(n);
        started.elapsed()
    };
    let first = time(n);
    if first < Duration::from_secs(1) {
        return;
    }
    let (quarter, whole) = (time(n / 4).min(time(n / 4)), first.min(time(n)));
    assert!(grows_linearly(quarter, whole), "{name}: {whole:?} at {n}, {quarter:?} at {}", n / 4);
}

/// The verdict on two times, at a quarter of the size and at the whole: under ten times as long
/// (about four is linear, sixteen quadratic), with 20 ms for a size so small that timer noise is
/// most of it.
fn grows_linearly(quarter: Duration, whole: Duration) -> bool {
    whole < quarter * 10 + Duration::from_millis(20)
}

/// The verdict, on times given rather than measured, so a busy machine cannot change it.
#[test]
fn the_verdict_fails_a_quadratic_case_and_passes_a_linear_one() {
    let whole = Duration::from_millis(1_100);
    assert!(!grows_linearly(whole / 16, whole), "a quadratic case: 16 times as long at four times the size");
    assert!(grows_linearly(whole / 4, whole), "a linear case: four times as long");
    assert!(grows_linearly(Duration::ZERO, Duration::from_millis(19)), "under the timer's noise");
}
