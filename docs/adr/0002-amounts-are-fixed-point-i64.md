# Amounts are fixed-point `i64`

The input spec gives amounts with up to four decimal places and expects the same precision in the output. We store every Amount as a whole number of 0.0001 units in an `i64`, parsed straight from the decimal text, so no rounding ever happens and arithmetic is exact. The `Amount` newtype is the only place that knows the representation.

## Considered Options

- **`f64`.** Rejected: binary floating point cannot represent most four-decimal values exactly, so balances drift and `0.1 + 0.2` style errors reach the report.
- **A decimal crate (for example `rust_decimal`).** Rejected: a fixed scale of four places needs only integer arithmetic and a small parser, so the crate would be a dependency carrying generality we never use, and its own rounding rules would have to be audited.
- **`i128` units.** Rejected for now: `i64` already covers about ±922 trillion whole units, far beyond any plausible balance, and keeps each stored Deposit half the size. Widening is a one-line change inside the newtype.

## Consequences

- Any balance change that would leave the `i64` range is Rejected as overflow instead of wrapping or panicking. That includes Total, which is computed as Available plus Held, so every change checks the resulting Total too.
- An amount with a fifth significant decimal place fails to parse (`TooManyDecimals`) instead of being rounded, so its Record is Malformed; trailing zeros are accepted.
- `Amount` can be negative, because a Dispute can push Available below zero (see the README → Assumptions).
