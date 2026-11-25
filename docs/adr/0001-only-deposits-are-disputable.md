# Only Deposits are disputable

The input spec calls every Record a "transaction" and says a Dispute targets "a transaction", but its balance rules (Dispute: Available −amount, Held +amount; Chargeback: Held −amount, Total −amount) only reverse money that came *in*. Applied to a Withdrawal, they would debit the Client a second time instead of crediting them, and the spec gives no inverse rules. We therefore decided that only Deposits are disputable. A Dispute action that targets any other Transaction id is Rejected.

## How Transactions are retained

Two structures, each with one job:

- **A global set of every Transaction id** from a well-formed Transaction, whether applied or Rejected. A well-formed Transaction uses up its id whatever its outcome (a declined withdrawal still has a reference number), so a later Record reusing any seen id, Deposit or Withdrawal, is Rejected as a duplicate before its Account is created.
- **Per Account, a map from Transaction id to each applied Deposit**, holding its amount and lifecycle state (Settled, Disputed, Charged back). Withdrawals and Rejected Deposits are never in it, so everything in it is disputable.

## Considered Options

- **Retain only Deposits and trust the spec for Withdrawal id uniqueness.** Rejected: a Deposit reusing a Withdrawal's id would be applied silently.
- **One global map from Transaction id to an entry enum (`Deposit {..}` or `NotDisputable`).** Rejected: every entry pays for the largest variant, the client-ownership check has to be written by hand, and a "not disputable" variant exists only to mark what the map should not contain.

## Consequences

- A Dispute action looks up a Deposit only inside its own Client's Account, so a mismatch between the Dispute action's Client and the Deposit's owner cannot happen by construction. An unknown id, a Withdrawal's id and another Client's Deposit all resolve to the same Rejection.
- The id is inserted into the global set *before* the Transaction is applied to its Account, and never removed. No step needs undoing, so the two structures can later sit behind independent locks (for example a sharded set beside per-Client Accounts) without a lock spanning both.
- Memory grows with every Transaction: about one `u32` per Withdrawal and one map entry per applied Deposit, all kept for the whole run.
- Supporting Withdrawal disputes later would mean defining inverse balance rules and storing Withdrawals in the per-Account map.
