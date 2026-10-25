//! Byte-level pieces the HTML and JSON writers share: testing text for a byte to escape eight
//! bytes at a time, and integer digits two at a time. Safe Rust, no dependencies.

const ONES: u64 = u64::from_ne_bytes([1; 8]);
const HIGH: u64 = ONES << 7;

/// Nonzero exactly when some byte of `x` is below `n` (n at most 128).
#[inline(always)]
fn below(x: u64, n: u8) -> u64 {
    x.wrapping_sub(ONES * n as u64) & !x & HIGH
}

/// Nonzero exactly when some byte of `x` is `b`.
#[inline(always)]
fn has(x: u64, b: u8) -> u64 {
    below(x ^ (ONES * b as u64), 1)
}

/// Some byte of `x` may be one HTML escapes: `&`, `'`, `<`, `>` or `"`. `| 5` turns `"` (0x22)
/// and `&` (0x26) into `'` (0x27), and `| 2` turns `<` (0x3c) into `>` (0x3e), so two tests find
/// all five. The first finds `#` (0x23) too, which is written as it is: a `#` only costs the
/// byte-exact path.
#[inline(always)]
pub(crate) fn html(x: u64) -> bool {
    (has(x | (ONES * 5), b'\'') | has(x | (ONES * 2), b'>')) != 0
}

/// Some byte of `x` is one JSON escapes: a control character, `"` or `\`.
#[cfg(feature = "json")]
#[inline(always)]
pub(crate) fn json(x: u64) -> bool {
    (below(x, 0x20) | has(x, b'"') | has(x, b'\\')) != 0
}

#[inline(always)]
fn word(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("eight bytes"))
}

#[inline(always)]
fn half(bytes: &[u8]) -> u64 {
    u32::from_le_bytes(bytes.try_into().expect("four bytes")) as u64
}

/// Whether `hit` finds a byte in `bytes`. Eight bytes per test; the last (or only) bytes are read
/// overlapping, so a short string is one test, not a loop over its bytes. A string shorter than
/// four is padded with spaces, which neither set has.
#[inline(always)]
pub(crate) fn any(bytes: &[u8], hit: impl Fn(u64) -> bool) -> bool {
    let n = bytes.len();
    if n >= 8 {
        let mut i = 0;
        while i + 8 < n {
            if hit(word(&bytes[i..i + 8])) {
                return true;
            }
            i += 8;
        }
        hit(word(&bytes[n - 8..]))
    } else if n >= 4 {
        hit(half(&bytes[..4]) | half(&bytes[n - 4..]) << 32)
    } else if n > 0 {
        hit(bytes[0] as u64 | (bytes[n / 2] as u64) << 8 | (bytes[n - 1] as u64) << 16 | (ONES * b' ' as u64) << 24)
    } else {
        false
    }
}

const PAIRS: &[u8; 200] = b"0001020304050607080910111213141516171819\
    2021222324252627282930313233343536373839\
    4041424344454647484950515253545556575859\
    6061626364656667686970717273747576777879\
    8081828384858687888990919293949596979899";

/// The decimal digits of `n`, written at the end of `buf`, two per division.
#[inline]
pub(crate) fn digits(mut n: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    while n >= 100 {
        let pair = (n % 100) as usize * 2;
        n /= 100;
        i -= 2;
        buf[i..i + 2].copy_from_slice(&PAIRS[pair..pair + 2]);
    }
    if n >= 10 {
        let pair = n as usize * 2;
        i -= 2;
        buf[i..i + 2].copy_from_slice(&PAIRS[pair..pair + 2]);
    } else {
        i -= 1;
        buf[i] = b'0' + n as u8;
    }
    &buf[i..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_escaped_byte_at_every_position_and_length() {
        for len in 0..40 {
            for at in 0..len {
                for (special, hit) in [(b'&', html as fn(u64) -> bool), (b'\'', html), (b'<', html), (b'>', html), (b'"', html)] {
                    let mut text = vec![b'a'; len];
                    text[at] = special;
                    assert!(any(&text, hit), "{len} {at} {}", special as char);
                }
            }
            assert!(!any(&vec![b'a'; len], html));
            // Bytes next to the special ones, and UTF-8, are not hits.
            assert!(!any(&"%(;=?é日".repeat(len).into_bytes()[..len.min(7 * len)], html));
        }
    }

    #[test]
    fn the_html_test_finds_the_five_and_only_hash_besides() {
        for b in 0..=255u8 {
            let alone = u64::from_le_bytes([b, b'a', b'a', b'a', b'a', b'a', b'a', b'a']);
            assert_eq!(html(alone), b"&'<>\"#".contains(&b), "{b:#x}");
        }
    }

    #[cfg(feature = "json")]
    #[test]
    fn finds_json_escapes() {
        for len in 1..40 {
            for at in 0..len {
                for special in (0u8..0x20).chain(*b"\"\\") {
                    let mut text = vec![b' '; len];
                    text[at] = special;
                    assert!(any(&text, json), "{len} {at} {special}");
                }
            }
            assert!(!any("!#[]^~\u{7f}é".repeat(len).as_bytes(), json));
        }
    }

    #[test]
    fn digits_match_display() {
        let mut buf = [0; 20];
        for n in (0..2000).chain([9_999, 10_000, 99_999, 123_456_789, u32::MAX as u64, u64::MAX - 1, u64::MAX]) {
            assert_eq!(digits(n, &mut buf), n.to_string().as_bytes());
        }
    }
}
