# transaction-engine

A toy payments engine. It reads a chronological CSV of Records, applies Deposits, Withdrawals and Dispute actions to Client Accounts, and writes the final Account balances as CSV to stdout.

```sh
cargo run -- transactions.csv > accounts.csv
```

Domain vocabulary is defined in [`GLOSSARY.md`](./GLOSSARY.md). Design decisions are recorded in [`docs/adr/`](./docs/adr/).

## Assumptions

Where the spec is silent, these choices follow how a bank would behave.

### Only Deposits can be disputed

The spec's Dispute and Chargeback rules only make sense for money that came in: applied to a Withdrawal, they would debit the Client twice. A Dispute action that targets a Withdrawal is Rejected. See [ADR 0001](./docs/adr/0001-only-deposits-are-disputable.md).

### Deposit lifecycle

A Deposit is **Settled**, **Disputed** or **Charged back**. A Dispute moves Settled → Disputed. A Resolve moves Disputed → Settled, so the Deposit can be disputed again. A Chargeback moves Disputed → Charged back, which is final. Any Dispute action that does not fit the Deposit's current state is Rejected.

### A Dispute action must come from the Deposit's owner

A Dispute, Resolve or Chargeback only reaches Deposits in its own Client's Account. Otherwise any Client could freeze another Client's Account. Dispute actions never create an Account.

### Transaction ids are checked for uniqueness

The spec promises globally unique Transaction ids. The engine still checks. Every well-formed Deposit or Withdrawal uses up its Transaction id, even if it is then Rejected (for example for insufficient funds). A later Transaction that reuses a seen id is Rejected as a whole and does not create an Account, because none of its fields can be trusted.

### When an Account is created

Any well-formed Deposit or Withdrawal creates its Client's Account, even if the Transaction itself is then Rejected. The exception is a duplicate Transaction id. Malformed Records and Dispute actions never create an Account.

### Balances can go negative, but only through disputes

A Withdrawal needs Available ≥ amount. A Dispute does not: if a Client deposits 100, withdraws 80, and the Deposit is then disputed, Available becomes −80 and Held becomes 100. A Chargeback then leaves Total at −80, which is a debt the Client owes. Refusing the Dispute instead would let exactly the fraud this engine exists to catch go through. Held never goes negative.

### Locked Accounts

A Chargeback Locks the Account permanently. A Locked Account:

- **rejects Withdrawals**, so no further funds can leave an Account that has suffered a Chargeback;
- **still accepts Deposits**. Incoming funds carry no risk to the engine, and they can pay down a negative balance that a Chargeback left behind. Refusing them would strand money the Client is sending in;
- **still accepts Disputes, Resolves and Chargebacks**. These come from the payment partner, not the Client, so freezing the Account cannot stop them. Refusing a Resolve would leave funds Held forever, and refusing a Chargeback would let a reversed Deposit stand.

This follows how major exchanges and payment processors handle chargeback restrictions in practice: outflows are suspended immediately, while inbound deposits remain open so the account holder can settle the outstanding balance and payment-rail notifications continue to be processed.

An Account is never unlocked.

### Amounts

Amounts are fixed-point integers in units of 0.0001, stored in an `i64` (up to about 922 trillion), and parsed without going through floating point, so no rounding ever happens. See [ADR 0002](./docs/adr/0002-amounts-are-fixed-point-i64.md).

These Records are Malformed, because their amount never becomes a valid Amount:

- an amount with more than four significant decimal places (trailing zeros such as `1.00000` are accepted);
- a zero or negative amount, or a Deposit or Withdrawal with no amount.

A Record that would overflow Available, Held or Total is Rejected.

An amount on a Dispute action is ignored.

## Input and errors

Whitespace around fields and headers is trimmed, columns are matched by header name, and Dispute actions may omit the trailing amount column. Header names and `type` values are case-sensitive. Headers must include lowercase `type`, `client`, `tx` and `amount`; type values must be lowercase `deposit`, `withdrawal`, `dispute`, `resolve` or `chargeback`.

| Outcome | Examples | Effect |
| --- | --- | --- |
| **Fatal** | wrong arguments, unreadable or empty file, missing or incorrectly cased header columns, I/O error mid-stream | exit non-zero, nothing on stdout |
| **Malformed** | unknown or incorrectly cased type value, unparsable number, missing field, invalid UTF-8 | Record skipped, warning on stderr (the first 100, then one line counting the rest) |
| **Rejected** | insufficient funds, duplicate id, no such Deposit, wrong lifecycle state, Withdrawal from a Locked Account, overflow | Record skipped |

An empty file is Fatal because it has no header. A file containing only a valid header succeeds with zero Accounts and writes only the output header.

A fatal error prints nothing, because balances from a half-read file would look final but be wrong. CSV has no framing, so a stream truncated mid-line cannot be told apart from a short row: it shows up as a Malformed Record, or, if cut inside the amount (`1.` for `1.5`), it is silently read as a smaller value.

Output rows are sorted by client id. Amounts are printed without trailing zeros but with at least one decimal place: `1.5`, `1.0001`, `2.0`, `0.0`.

## Scaling

The engine is fed one Record at a time and never holds the input in memory. Its memory grows with the number of Transactions: one id per Transaction for the uniqueness check, plus each applied Deposit, kept for the whole run because the spec sets no time limit on disputes. For inputs near the full `u32` id space, the id set could become a fixed 512 MiB bitset.

The engine is single-threaded. For many concurrent streams, Accounts can be sharded by Client, since every Record only ever touches its own Client's Account. The global Transaction id set is the one shared structure, and a sharded concurrent set keeps contention per bucket. One caveat: with threads, which of two Records with the same id is accepted depends on timing, so results could differ between runs unless ids are guaranteed unique upstream.

## Correctness and safety

### Type-system guarantees

- **Distinct IDs:** `ClientId(u16)` and `TransactionId(u32)` are separate newtypes, so client and transaction identifiers cannot be swapped accidentally.
- **Structural record split:** `Record` separates `Transaction` (always carries an `Amount`) from `DisputeAction` (has no `Amount` field), eliminating impossible combinations at compile time.
- **Checked-only arithmetic:** `Amount(i64)` deliberately does not implement `+` or `-`; all balance math goes through `checked_add` and `checked_sub`.
- **Single source of truth for Total:** `Account` stores only `available` and `held` and computes `total = available + held`. Every balance mutation checks `available + held` for overflow before
committing, so `total` can never drift or overflow.
- **Exhaustive lifecycle:** Deposit transitions (`Settled`, `Disputed`, `ChargedBack`) are enforced by a single exhaustive `match`.

### Safety

No `unsafe` code is used anywhere in the crate. Production paths contain no `unwrap()`; the only `expect()` is in `Account::total()`, which is protected by the commit-time overflow check in
`Account::set_balances`.

### Testing

Run the full test suite with `cargo test`:

- **Unit tests** (`src/amount.rs`, `src/engine.rs`): fixed-point parsing/formatting edge cases, checked overflow, and every deposit/withdrawal/dispute/resolve/chargeback state transition.
- **Acceptance fixtures** (`tests/fixtures/`): 13 hand-derived CSV input/expected pairs covering the spec example, negative balances after chargeback, locked-account behavior, cross-client/withdrawal
disputes, duplicate IDs, formatting, malformed rows, and amount edge cases.
- **Pipeline & CLI tests** (`tests/pipeline.rs`, `tests/cli.rs`): exit codes, stdout/stderr separation, missing headers, and mid-stream I/O failures writing nothing to stdout.
- **Volume & noise-invariance test** (`tests/large_stream.rs`): streams interleaved cohorts with closed-form expected balances and verifies injected noise has zero effect. Run the large 10,000-client /
200-round case with:

```sh
cargo test --release --test large_stream -- --ignored
```
