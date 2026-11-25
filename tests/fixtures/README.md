# Acceptance fixtures

End-to-end fixtures for the pipeline function (issue #1 → Testing Decisions → **Seam 1**).
Each scenario is a pair sharing a name: `<scenario>.input.csv` is fed to the engine and
its output must equal `<scenario>.expected.csv`.

These are **data only**: every expected output was derived by hand from the rules, not
from any implementation. The sources of truth are `GLOSSARY.md`, ADR 0001
(`docs/adr/0001-only-deposits-are-disputable.md`) and the README's **Assumptions** and
**Input and errors** sections. The in-process harness that runs the pairs is built in
issue #7.

## Output conventions

Every `*.expected.csv` follows the report format from the README and issue #1:

- header `client,available,held,total,locked`;
- one row per Account, **sorted by client id**;
- every amount printed **without trailing zeros, keeping at least one decimal place** (`1.5`, `1.0001`, `2.0`, `0.0`);
- `locked` as `true` / `false`;
- `total` always equals `available + held`.

## Scenarios

| Fixture | What it proves | Rule |
| --- | --- | --- |
| `spec-example` | Two Clients; Client 2's over-Available Withdrawal is Rejected and leaves balances untouched. | README *Deposits and Withdrawals* (US 22–24) |
| `deposit-withdraw-dispute-chargeback` | Deposit 100, withdraw 80, Dispute the Deposit (Available → −80, Held 100), Chargeback (Total → −80, Locked). | README *Balances can go negative, but only through disputes*; ADR 0001 |
| `dispute-resolve-dispute-chargeback` | A Resolve returns a Deposit to Settled so it can be Disputed again; the second Dispute is Charged back. | README *Deposit lifecycle* (US 29, 30) |
| `chargeback-after-resolve-rejected` | After a Resolve the Deposit is Settled, so a Chargeback (which needs Disputed) is Rejected; nothing changes and the Account is not Locked. | README *Deposit lifecycle* (US 34) |
| `locked-account-behaviour` | A Chargeback Locks the Account; a later Withdrawal is Rejected while a new Deposit is accepted and is itself disputable and chargeable. | README *Locked Accounts* (US 42–45) |
| `dispute-from-wrong-client` | A Dispute naming another Client cannot reach that Client's Deposit; no balances change. | README *A Dispute action must come from the Deposit's owner* (US 37); ADR 0001 |
| `dispute-for-unknown-client` | A Dispute action for a Client with no Account (and an unknown Transaction id) is Rejected and creates no Account. | README *A Dispute action must come from the Deposit's owner* (US 35, 38) |
| `dispute-targets-withdrawal` | A Dispute targeting a Withdrawal's id is Rejected (Withdrawals are not retained as disputable). | ADR 0001 (US 36) |
| `duplicate-transaction-ids` | A reused id is Rejected whether the first use was an applied Deposit or a Rejected Withdrawal, and a duplicate for a brand-new Client creates no Account. | README *Transaction ids are checked for uniqueness* (US 26, 27) |
| `rejected-withdrawal-creates-account` | A Withdrawal from an unknown Client is Rejected for insufficient funds but still creates a zero Account. | README *When an Account is created* (US 25) |
| `input-formatting` | Whitespace around fields, reordered columns, and Dispute rows with no trailing amount column are all accepted. | README *Input and errors* (US 9–12) |
| `malformed-rows` | Unknown type, an unparsable amount, a Record missing fields, and invalid UTF-8 are skipped as Malformed and never create an Account, while valid Records around them still apply. The input file contains a raw invalid byte. | README *Input and errors* → Malformed (US 13, 14) |
| `amount-edge-cases` | Four decimals and trailing zeros beyond four are accepted exactly; five significant decimals, zero, negative and missing amounts are Malformed; a Deposit that would overflow is Rejected. | README *Amounts* (US 15–20) |
| `decimal-shorthand-and-trailing-zeros` | `.5`, `5.` and more than four trailing zeros (`1.2345000`) parse exactly; sums to `6.0`. | README *Amounts* |
| `dispute-ignores-populated-or-garbage-amount` | A populated or unparsable 4th column on a Dispute or Resolve is ignored; the full Deposit amount is Held. | README *Input and errors* |
| `multi-deposit-partial-chargeback-and-debt-recovery` | Two simultaneous Disputes; one Resolved, one Charged back; a later Deposit pays the debt down on the Locked Account while a Withdrawal is still Rejected. | README *Balances can go negative*, *Locked Accounts* |
| `active-held-funds-at-eof` | A Dispute still open at end of input is reported as Held. | README *Deposit lifecycle* |
| `header-only-zero-accounts` | A header-only file prints only the output header. | README *Input and errors* |
| `u16-u32-zero-and-max-boundaries` | Client and tx ids at `0` and at `u16`/`u32` max are accepted. | README *Input and errors* |
| `resolve-rejected-outside-disputed-state` | Resolve of a Settled Deposit, a second Resolve, and a Resolve or Dispute after Chargeback are all Rejected; so is a Dispute of an already-Disputed Deposit. | README *Deposit lifecycle* |
| `resolve-wrong-target-rejected` | Resolve targeting a Withdrawal, an unknown tx, another Client's Deposit, or from an unknown Client is Rejected and creates no Account. | README *A Dispute action must come from the Deposit's owner*; ADR 0001 |
| `resolve-on-locked-account-and-from-negative` | Resolve restores a negative Available; a Resolve on a Locked Account still releases Held funds. | README *Locked Accounts* |

### Notes on hand-derived values

- `locked-account-behaviour`: after the Lock the Account holds 100; the Withdrawal of 50
  is Rejected (so Available stays 100), and the new Deposit of 25 is accepted, Disputed
  and Charged back, which nets to zero — leaving `100.0`.
- `duplicate-transaction-ids`: Client 1 keeps `100 + 10`; Client 2's Account exists at
  zero because its over-Available Withdrawal used up id `2` before the duplicate Deposit;
  Client 3 never appears because its Deposit reused id `3`.
- `amount-edge-cases`: Client 1 is `1.2345 + 2.5`; the two 900-trillion Deposits for
  Client 2 overflow on the second, so only the first (`900000000000000.0`) stands.
- `resolve-rejected-outside-disputed-state`: every Rejected Resolve would, if applied, move
  money out of an empty Held (or re-credit a charged-back 10), so the final `40.0` / `0.0`
  only holds if all of them are skipped.
- `resolve-wrong-target-rejected`: Client 2 is `20 − 5 = 15`, with the Deposit still Disputed
  (`-5.0` Available, `20.0` Held); Client 3 never appears.
- `resolve-on-locked-account-and-from-negative`: `100 − 80 = 20`, Dispute → `−80/100`, Resolve → `20/0`;
  Deposits 30 and 10 → `60`; both Disputed → `20/40`; Chargeback of 10 → `20/30`, Locked;
  Resolve of 30 → `50/0`; the final Withdrawal is Rejected.
