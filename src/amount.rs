//! The fixed-point monetary [`Amount`].

use std::fmt;
use std::str::FromStr;

/// A monetary amount as a fixed-point integer counting units of 0.0001,
/// stored in an `i64` (a range of about ±922 trillion whole units).
///
/// Parsing never goes through floating point, so no rounding ever happens.
/// An `Amount` can be negative, because a Dispute can drive a balance below
/// zero; parsing therefore accepts a leading `-` and the "a Deposit or
/// Withdrawal amount must be positive" rule is left to the caller.
///
/// Parsing accepts magnitudes up to `i64::MAX` units (about ±922 trillion);
/// the single most-negative value, `i64::MIN`, is representable and formats
/// correctly but is deliberately not parseable, which no valid input reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(i64);

impl Amount {
    /// Zero.
    pub const ZERO: Amount = Amount(0);

    /// The number of 0.0001 units in one whole unit.
    const SCALE: i64 = 10_000;

    /// Build an `Amount` directly from a count of 0.0001 units.
    pub const fn from_units(units: i64) -> Self {
        Amount(units)
    }

    /// The raw count of 0.0001 units.
    pub const fn units(self) -> i64 {
        self.0
    }

    /// Checked addition; `None` on overflow in either direction.
    pub fn checked_add(self, rhs: Amount) -> Option<Amount> {
        self.0.checked_add(rhs.0).map(Amount)
    }

    /// Checked subtraction; `None` on overflow in either direction.
    pub fn checked_sub(self, rhs: Amount) -> Option<Amount> {
        self.0.checked_sub(rhs.0).map(Amount)
    }
}

/// Why a string could not be read as an [`Amount`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmountParseError {
    /// The text was empty (after trimming).
    Empty,
    /// The text held a character that is not part of a decimal number.
    InvalidChar,
    /// More than four significant decimal places, which would lose precision.
    TooManyDecimals,
    /// The value does not fit in the `i64` unit range.
    OutOfRange,
}

impl fmt::Display for AmountParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            AmountParseError::Empty => "empty amount",
            AmountParseError::InvalidChar => "amount is not a decimal number",
            AmountParseError::TooManyDecimals => "amount has more than four decimal places",
            AmountParseError::OutOfRange => "amount is out of range",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for AmountParseError {}

impl FromStr for Amount {
    type Err = AmountParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err(AmountParseError::Empty);
        }

        let (negative, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };

        let (int_part, frac_part) = match body.split_once('.') {
            Some((int, frac)) => (int, frac),
            None => (body, ""),
        };

        // Both parts must be plain ASCII digits (either may be empty, as in
        // "5." or ".5", but not both).
        let is_digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if !is_digits(int_part) || !is_digits(frac_part) {
            return Err(AmountParseError::InvalidChar);
        }
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(AmountParseError::InvalidChar);
        }

        // Trailing zeros carry no precision, so only the significant decimals
        // count against the four-place limit.
        if frac_part.trim_end_matches('0').len() > 4 {
            return Err(AmountParseError::TooManyDecimals);
        }

        // Whole units, scaled into 0.0001 units, with every step checked so a
        // value beyond the i64 range is rejected rather than wrapped.
        let int_units = if int_part.is_empty() {
            0i64
        } else {
            int_part
                .parse::<i64>()
                .map_err(|_| AmountParseError::OutOfRange)?
                .checked_mul(Self::SCALE)
                .ok_or(AmountParseError::OutOfRange)?
        };

        // The first four fractional digits, right-padded. Anything past the
        // fourth place has already been checked to be a trailing zero.
        let mut frac_digits = [b'0'; 4];
        for (slot, byte) in frac_digits.iter_mut().zip(frac_part.bytes()) {
            *slot = byte;
        }
        let frac_units = (frac_digits[0] - b'0') as i64 * 1_000
            + (frac_digits[1] - b'0') as i64 * 100
            + (frac_digits[2] - b'0') as i64 * 10
            + (frac_digits[3] - b'0') as i64;

        let magnitude = int_units
            .checked_add(frac_units)
            .ok_or(AmountParseError::OutOfRange)?;
        let units = if negative { -magnitude } else { magnitude };
        Ok(Amount(units))
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Work on the magnitude so the sign and the four-digit fraction are
        // formatted independently; `unsigned_abs` copes with `i64::MIN`.
        let magnitude = self.0.unsigned_abs();
        let scale = Self::SCALE as u64;
        if self.0 < 0 {
            f.write_str("-")?;
        }
        write!(f, "{}.{:04}", magnitude / scale, magnitude % scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Amount, AmountParseError> {
        s.parse::<Amount>()
    }

    // --- parsing: accepted forms ---

    #[test]
    fn parses_zero() {
        assert_eq!(parse("0").unwrap().units(), 0);
    }

    #[test]
    fn parses_whole_number() {
        assert_eq!(parse("5").unwrap().units(), 50_000);
    }

    #[test]
    fn parses_one_decimal() {
        assert_eq!(parse("1.5").unwrap().units(), 15_000);
    }

    #[test]
    fn parses_four_decimals_exactly() {
        assert_eq!(parse("1.2345").unwrap().units(), 12_345);
    }

    #[test]
    fn parses_smallest_unit() {
        assert_eq!(parse("0.0001").unwrap().units(), 1);
    }

    #[test]
    fn accepts_trailing_zeros_beyond_four() {
        // 1.00000 has five decimal places but none of them is significant.
        assert_eq!(parse("1.00000").unwrap().units(), 10_000);
    }

    #[test]
    fn accepts_trailing_zero_making_five_places_but_four_significant() {
        assert_eq!(parse("1.23450").unwrap().units(), 12_345);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(parse("  1.0  ").unwrap().units(), 10_000);
    }

    #[test]
    fn parses_negative() {
        assert_eq!(parse("-0.5").unwrap().units(), -5_000);
    }

    #[test]
    fn parses_largest_representable_value() {
        // i64::MAX units == 922337203685477.5807
        assert_eq!(parse("922337203685477.5807").unwrap().units(), i64::MAX);
    }

    // --- parsing: rejected forms ---

    #[test]
    fn rejects_empty() {
        assert_eq!(parse(""), Err(AmountParseError::Empty));
        assert_eq!(parse("   "), Err(AmountParseError::Empty));
    }

    #[test]
    fn rejects_five_significant_decimals() {
        assert_eq!(parse("1.23456"), Err(AmountParseError::TooManyDecimals));
    }

    #[test]
    fn rejects_fifth_decimal_place_even_when_small() {
        assert_eq!(parse("0.00001"), Err(AmountParseError::TooManyDecimals));
    }

    #[test]
    fn rejects_non_numeric_text() {
        assert_eq!(parse("abc"), Err(AmountParseError::InvalidChar));
        assert_eq!(parse("1,0"), Err(AmountParseError::InvalidChar));
        assert_eq!(parse("1.2.3"), Err(AmountParseError::InvalidChar));
        assert_eq!(parse("+5"), Err(AmountParseError::InvalidChar));
    }

    #[test]
    fn rejects_value_beyond_i64_range() {
        assert_eq!(parse("99999999999999999999"), Err(AmountParseError::OutOfRange));
        // one unit past i64::MAX
        assert_eq!(parse("922337203685477.5808"), Err(AmountParseError::OutOfRange));
    }

    // --- formatting: always four decimal places ---

    fn shown(units: i64) -> String {
        Amount::from_units(units).to_string()
    }

    #[test]
    fn formats_whole_number_with_four_decimals() {
        assert_eq!(shown(50_000), "5.0000");
    }

    #[test]
    fn formats_with_trailing_decimal_digits() {
        assert_eq!(shown(15_000), "1.5000");
        assert_eq!(shown(12_345), "1.2345");
    }

    #[test]
    fn formats_zero() {
        assert_eq!(shown(0), "0.0000");
    }

    #[test]
    fn formats_negative() {
        assert_eq!(shown(-800_000), "-80.0000");
    }

    #[test]
    fn formats_negative_between_minus_one_and_zero_with_leading_zero() {
        assert_eq!(shown(-5_000), "-0.5000");
        assert_eq!(shown(-1), "-0.0001");
    }

    #[test]
    fn formats_extremes() {
        assert_eq!(shown(i64::MAX), "922337203685477.5807");
        assert_eq!(shown(i64::MIN), "-922337203685477.5808");
    }

    // --- checked arithmetic ---

    #[test]
    fn adds_and_subtracts_within_range() {
        let a = Amount::from_units(10_000);
        let b = Amount::from_units(2_500);
        assert_eq!(a.checked_add(b), Some(Amount::from_units(12_500)));
        assert_eq!(a.checked_sub(b), Some(Amount::from_units(7_500)));
    }

    #[test]
    fn addition_reports_overflow_in_both_directions() {
        assert_eq!(Amount::from_units(i64::MAX).checked_add(Amount::from_units(1)), None);
        assert_eq!(Amount::from_units(i64::MIN).checked_add(Amount::from_units(-1)), None);
    }

    #[test]
    fn subtraction_reports_overflow_in_both_directions() {
        assert_eq!(Amount::from_units(i64::MIN).checked_sub(Amount::from_units(1)), None);
        assert_eq!(Amount::from_units(i64::MAX).checked_sub(Amount::from_units(-1)), None);
    }
}
