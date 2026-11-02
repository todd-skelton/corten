//! A test's check that a case runs in about linear time, by its growth rather than the clock.

/// `case` at size `n` runs in about linear time. Under a second is (the quadratic version of
/// each case that uses it takes far longer); slower, on a busy machine, it is checked by its growth instead: at
/// `n` about four times as long as at `n / 4`, not sixteen, the fastest of two runs at each
/// size, so the load slows both alike rather than failing a bound on the clock.
pub(crate) fn linear(name: &str, n: usize, case: impl Fn(usize)) {
    let time = |n: usize| {
        let started = std::time::Instant::now();
        case(n);
        started.elapsed()
    };
    let first = time(n);
    if first < std::time::Duration::from_secs(1) {
        return;
    }
    let (quarter, whole) = (time(n / 4).min(time(n / 4)), first.min(time(n)));
    // Ten times, with 20 ms for a size so small that timer noise is most of it.
    let bound = quarter * 10 + std::time::Duration::from_millis(20);
    assert!(whole < bound, "{name}: {whole:?} at {n}, {quarter:?} at {}", n / 4);
}

/// The growth check fails a case that grows with the square of its size once it takes a
/// second: 16 times the time at a quarter of the size, past the bound of 10.
#[test]
#[should_panic(expected = "quadratic")]
fn linear_fails_a_quadratic_case() {
    linear("quadratic", 1_000, |n| std::thread::sleep(std::time::Duration::from_micros((n * n) as u64 * 11 / 10)));
}

/// And passes one that grows with its size, however slow the machine makes it.
#[test]
fn linear_passes_a_linear_case() {
    linear("linear", 1_000, |n| std::thread::sleep(std::time::Duration::from_micros(n as u64 * 1_100)));
}
