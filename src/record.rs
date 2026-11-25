//! Domain records: what one row of input means once it is understood.
//!
//! A [`Record`] is either a [`Transaction`] (a Deposit or Withdrawal, which
//! carries an [`Amount`]) or a [`DisputeAction`] (a Dispute, Resolve or
//! Chargeback, which carries no amount and targets a Deposit by its
//! [`TransactionId`]). The split is structural: a `Transaction` cannot be
//! built without a positive amount, and a `DisputeAction` has no amount field to set.

use std::fmt;

use crate::amount::Amount;

/// The owner of exactly one Account, identified only by a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(u16);

impl ClientId {
    /// Wrap a raw client number.
    pub const fn new(value: u16) -> Self {
        ClientId(value)
    }

    /// The raw client number.
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl From<u16> for ClientId {
    fn from(value: u16) -> Self {
        ClientId(value)
    }
}

/// The globally unique identity of a Transaction. Dispute actions reference
/// it but never own one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransactionId(u32);

impl TransactionId {
    /// Wrap a raw transaction number.
    pub const fn new(value: u32) -> Self {
        TransactionId(value)
    }

    /// The raw transaction number.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl From<u32> for TransactionId {
    fn from(value: u32) -> Self {
        TransactionId(value)
    }
}

/// A Deposit or Withdrawal: owns a [`TransactionId`] and carries a positive [`Amount`].
///
/// The fields are private so [`Transaction::new`] is the only way to build one:
/// every `Transaction` in existence has an amount greater than zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transaction {
    client: ClientId,
    transaction_id: TransactionId,
    kind: TransactionKind,
    amount: Amount,
}

/// A Deposit or Withdrawal was given a zero or negative amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NonPositiveAmount(pub Amount);

impl fmt::Display for NonPositiveAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "amount {} is not positive", self.0)
    }
}

impl std::error::Error for NonPositiveAmount {}

impl Transaction {
    /// Build a Transaction, rejecting a zero or negative `amount`.
    pub fn new(
        client: ClientId,
        transaction_id: TransactionId,
        kind: TransactionKind,
        amount: Amount,
    ) -> Result<Self, NonPositiveAmount> {
        if amount <= Amount::ZERO {
            return Err(NonPositiveAmount(amount));
        }
        Ok(Transaction {
            client,
            transaction_id,
            kind,
            amount,
        })
    }

    /// The Client whose Account the Transaction touches.
    pub fn client(&self) -> ClientId {
        self.client
    }

    /// The Transaction's globally unique id.
    pub fn transaction_id(&self) -> TransactionId {
        self.transaction_id
    }

    /// Whether it credits (Deposit) or debits (Withdrawal) the Account.
    pub fn kind(&self) -> TransactionKind {
        self.kind
    }

    /// The amount moved, always greater than zero.
    pub fn amount(&self) -> Amount {
        self.amount
    }
}

/// Which way a [`Transaction`] moves funds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    /// Credits a Client's Account.
    Deposit,
    /// Debits a Client's Account.
    Withdrawal,
}

/// A Dispute, Resolve or Chargeback: carries no amount and targets an
/// existing Deposit by that Deposit's [`TransactionId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisputeAction {
    /// The Client making the claim; it only ever reaches that Client's Account.
    pub client: ClientId,
    /// The Transaction id of the targeted Deposit.
    pub transaction_id: TransactionId,
    /// Which stage of the dispute lifecycle this action is.
    pub kind: DisputeKind,
}

/// Which stage of the dispute lifecycle a [`DisputeAction`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisputeKind {
    /// Asserts a Deposit was erroneous, moving its amount to Held.
    Dispute,
    /// Closes a Dispute in the Account's favour, releasing the Held amount.
    Resolve,
    /// Closes a Dispute against the Account, removing the Held amount and Locking it.
    Chargeback,
}

/// One understood row of input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record {
    /// A Deposit or Withdrawal.
    Transaction(Transaction),
    /// A Dispute, Resolve or Chargeback.
    DisputeAction(DisputeAction),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(units: i64) -> Result<Transaction, NonPositiveAmount> {
        Transaction::new(
            ClientId::new(1),
            TransactionId::new(1),
            TransactionKind::Deposit,
            Amount::from_units(units),
        )
    }

    #[test]
    fn positive_amount_builds_a_transaction() {
        let transaction = build(1).expect("smallest positive amount is accepted");
        assert_eq!(transaction.amount(), Amount::from_units(1));
        assert_eq!(transaction.client(), ClientId::new(1));
        assert_eq!(transaction.transaction_id(), TransactionId::new(1));
        assert_eq!(transaction.kind(), TransactionKind::Deposit);
    }

    #[test]
    fn zero_and_negative_amounts_are_rejected() {
        for units in [0, -1, i64::MIN] {
            assert_eq!(
                build(units),
                Err(NonPositiveAmount(Amount::from_units(units)))
            );
        }
    }
}
