//! Domain records: what one row of input means once it is understood.
//!
//! A [`Record`] is either a [`Transaction`] (a Deposit or Withdrawal, which
//! carries an [`Amount`]) or a [`DisputeAction`] (a Dispute, Resolve or
//! Chargeback, which carries no amount and targets a Deposit by its
//! [`TransactionId`]). The split is structural: a `Transaction` cannot be
//! built without an amount, and a `DisputeAction` has no amount field to set.

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

/// A Deposit or Withdrawal: owns a [`TransactionId`] and carries an [`Amount`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transaction {
    /// The Client whose Account the Transaction touches.
    pub client: ClientId,
    /// The Transaction's globally unique id.
    pub transaction_id: TransactionId,
    /// Whether it credits (Deposit) or debits (Withdrawal) the Account.
    pub kind: TransactionKind,
    /// The amount moved. Positivity is a business rule checked by the caller.
    pub amount: Amount,
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
