//! Fuzz `Amount::from_str` and `Display`.
//!
//! * Parsing never panics.
//! * Anything that parses formats to a string that parses back to the same value.
//! * A parsed value agrees with an independent i128 decimal parser.
#![no_main]

use libfuzzer_sys::fuzz_target;

use transaction_engine::Amount;
use transaction_engine_fuzz::check_amount_roundtrip;

/// Independent reference: `Some(units)` if `s` is a valid amount.
fn reference(s: &str) -> Option<i64> {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let mut parts = body.splitn(2, '.');
    let int = parts.next()?;
    let frac = parts.next().unwrap_or("");
    if (int.is_empty() && frac.is_empty())
        || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let frac = frac.trim_end_matches('0');
    if frac.len() > 4 {
        return None;
    }
    let mut units: i128 = 0;
    for b in int.bytes() {
        units = units * 10 + (b - b'0') as i128;
        if units > i64::MAX as i128 {
            return None;
        }
    }
    for i in 0..4 {
        let d = frac.as_bytes().get(i).map_or(0, |b| b - b'0');
        units = units * 10 + d as i128;
    }
    if units > i64::MAX as i128 {
        return None;
    }
    Some(if neg { -units } else { units } as i64)
}

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    let parsed = s.parse::<Amount>();
    // Rust's `trim` and ours must agree for the comparison to be meaningful;
    // both use Unicode whitespace.
    assert_eq!(
        parsed.ok().map(Amount::units),
        reference(s),
        "parser disagrees with reference on {s:?}"
    );
    if let Ok(a) = parsed {
        check_amount_roundtrip(a);
    }
});
