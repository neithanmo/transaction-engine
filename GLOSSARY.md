# Transaction Engine

A toy payments engine that replays a chronological stream of client Records and reports each client's Account balances.

## Language

### Input

**Record**:
One row of input, before it is known to be a Transaction or a Dispute action.
_Avoid_: Operation, entry, event, row

**Transaction**:
A Deposit or Withdrawal. It owns a Transaction id and carries an amount.
_Avoid_: Tx (as a domain term), operation

**Transaction id**:
The globally unique identity of a Transaction, whichever kind it is. Dispute actions reference it but never own one.
_Avoid_: Deposit id, withdrawal id, tx

**Deposit**:
A Transaction that credits a client's Account.

**Withdrawal**:
A Transaction that debits a client's Account.

**Dispute action**:
A Dispute, Resolve or Chargeback. It carries no amount and owns no Transaction id; it targets an existing Deposit by that Deposit's Transaction id.
_Avoid_: Dispute transaction, claim

**Dispute**:
A Dispute action asserting that a Deposit was erroneous, moving its amount from Available to Held.

**Resolve**:
A Dispute action closing a Dispute in the Account's favour, releasing the Held amount back to Available.
_Avoid_: Resolution (as a state)

**Chargeback**:
A Dispute action closing a Dispute against the Account, removing the Held amount and Locking the Account.
_Avoid_: Reversal

**Rejected**:
Describes a Record that leaves every balance and every Deposit's state unchanged.
_Avoid_: Failed, invalid, dropped

### Accounts

**Client**:
The owner of exactly one Account, identified only by a number.
_Avoid_: Customer, user

**Account**:
A Client's single asset account, holding the Available and Held balances and the Locked flag.
_Avoid_: Wallet, balance

**Available**:
Funds the Client may withdraw or trade. May be negative after a Dispute.

**Held**:
Funds set aside while a Deposit is Disputed. Never negative.

**Total**:
Available plus Held.

**Locked**:
An Account that has suffered a Chargeback. A Locked Account accepts Deposits and Dispute actions but rejects Withdrawals, and is never unlocked.
_Avoid_: Frozen, restricted, closed

### Deposit lifecycle

**Settled**:
A Deposit that is not under Dispute and can be Disputed. Every Deposit starts here, and a Resolve returns it here.

**Disputed**:
A Deposit whose amount is currently Held.

**Charged back**:
A Deposit that suffered a Chargeback. It is final: every later Dispute action targeting it is Rejected.
