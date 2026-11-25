//! The domain [`Engine`] that applies [`Record`]s to Client [`Account`]s.
//!
//! The engine knows nothing about CSV or I/O. Vocabulary follows `GLOSSARY.md`
//! and transaction storage follows ADR 0001.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use crate::amount::Amount;
use crate::record::{ClientId, Record, TransactionId, TransactionKind};

/// Why a [`Record`] was Rejected by a business rule.
///
/// Rejections are expected domain outcomes (distinct from Malformed input
/// errors) that leave every balance and every Deposit's state unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// Another well-formed Transaction already used this Transaction id.
    DuplicateTransactionId,
    /// A Withdrawal exceeded the Account's Available balance.
    InsufficientFunds,
    /// A Withdrawal targeted a Locked Account.
    AccountLocked,
    /// Applying the Record would overflow Available, Held or Total.
    Overflow,
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Rejection::DuplicateTransactionId => "duplicate transaction id",
            Rejection::InsufficientFunds => "insufficient available funds",
            Rejection::AccountLocked => "account is locked",
            Rejection::Overflow => "balance overflow",
        };
        f.write_str(msg)
    }
}

/// Lifecycle state of a stored Deposit (Settled, Disputed, Charged back).
/// `Disputed` and `ChargedBack` arrive with Dispute actions in #6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepositState {
    Settled,
}

/// An applied Deposit retained inside its owner's [`Account`] so later
/// Dispute actions can reach it without a separate client check (ADR 0001).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Deposit {
    amount: Amount,
    state: DepositState,
}

/// A Client's single asset account, holding Available, Held, Locked and
/// applied Deposits. Total is always computed as Available plus Held.
#[derive(Debug)]
pub struct Account {
    available: Amount,
    held: Amount,
    locked: bool,
    deposits: HashMap<TransactionId, Deposit>,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            available: Amount::ZERO,
            held: Amount::ZERO,
            locked: false,
            deposits: HashMap::new(),
        }
    }
}

impl Account {
    /// Funds the Client may withdraw or trade.
    pub fn available(&self) -> Amount {
        self.available
    }

    /// Funds set aside while a Deposit is Disputed.
    pub fn held(&self) -> Amount {
        self.held
    }

    /// Available plus Held, always computed rather than stored.
    pub fn total(&self) -> Amount {
        self.available
            .checked_add(self.held)
            .expect("account invariant: available + held never overflows")
    }

    /// Whether the Account is Locked.
    pub fn locked(&self) -> bool {
        self.locked
    }

    fn set_available(&mut self, new_available: Amount) -> Result<(), Rejection> {
        let _ = new_available
            .checked_add(self.held)
            .ok_or(Rejection::Overflow)?;
        self.available = new_available;
        Ok(())
    }

    fn apply_deposit(
        &mut self,
        transaction_id: TransactionId,
        amount: Amount,
    ) -> Result<(), Rejection> {
        let new_available = self
            .available
            .checked_add(amount)
            .ok_or(Rejection::Overflow)?;
        self.set_available(new_available)?;
        self.deposits.insert(
            transaction_id,
            Deposit {
                amount,
                state: DepositState::Settled,
            },
        );
        Ok(())
    }

    fn apply_withdrawal(&mut self, amount: Amount) -> Result<(), Rejection> {
        if self.locked {
            return Err(Rejection::AccountLocked);
        }
        if self.available < amount {
            return Err(Rejection::InsufficientFunds);
        }
        let new_available = self
            .available
            .checked_sub(amount)
            .ok_or(Rejection::Overflow)?;
        self.set_available(new_available)
    }

    #[cfg(test)]
    fn hold(&mut self, amount: Amount) {
        self.available = self
            .available
            .checked_sub(amount)
            .expect("test setup: hold underflow");
        self.held = self
            .held
            .checked_add(amount)
            .expect("test setup: hold overflow");
    }
}

/// Synchronous, single-threaded payments engine.
#[derive(Debug, Default)]
pub struct Engine {
    seen_transaction_ids: HashSet<TransactionId>,
    accounts: BTreeMap<ClientId, Account>,
}

impl Engine {
    /// Create an empty engine with no Accounts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one domain [`Record`] to the engine.
    pub fn apply(&mut self, record: Record) -> Result<(), Rejection> {
        match record {
            Record::Transaction(transaction) => {
                debug_assert!(
                    transaction.amount > Amount::ZERO,
                    "well-formed transactions carry a positive amount"
                );
                if !self.seen_transaction_ids.insert(transaction.transaction_id) {
                    return Err(Rejection::DuplicateTransactionId);
                }
                let account = self.accounts.entry(transaction.client).or_default();
                match transaction.kind {
                    TransactionKind::Deposit => {
                        account.apply_deposit(transaction.transaction_id, transaction.amount)
                    }
                    TransactionKind::Withdrawal => account.apply_withdrawal(transaction.amount),
                }
            }
            Record::DisputeAction(_) => todo!("dispute actions are implemented in #6"),
        }
    }

    /// Read all Accounts in ascending [`ClientId`] order.
    pub fn accounts(&self) -> impl Iterator<Item = (ClientId, &Account)> {
        self.accounts
            .iter()
            .map(|(&client, account)| (client, account))
    }

    /// Test-only setup until Chargeback exists in #6.
    #[cfg(test)]
    fn lock_account(&mut self, client: ClientId) {
        self.accounts
            .get_mut(&client)
            .expect("test setup: account must exist before locking")
            .locked = true;
    }

    /// Test-only setup for non-zero Held until Dispute exists in #6.
    #[cfg(test)]
    fn hold_funds(&mut self, client: ClientId, amount: Amount) {
        self.accounts
            .get_mut(&client)
            .expect("test setup: account must exist before holding funds")
            .hold(amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Transaction, TransactionId, TransactionKind};

    fn amount(s: &str) -> Amount {
        s.parse().expect("valid test amount literal")
    }

    fn deposit(client: u16, transaction_id: u32, amt: &str) -> Record {
        Record::Transaction(Transaction {
            client: ClientId::new(client),
            transaction_id: TransactionId::new(transaction_id),
            kind: TransactionKind::Deposit,
            amount: amount(amt),
        })
    }

    fn withdrawal(client: u16, transaction_id: u32, amt: &str) -> Record {
        Record::Transaction(Transaction {
            client: ClientId::new(client),
            transaction_id: TransactionId::new(transaction_id),
            kind: TransactionKind::Withdrawal,
            amount: amount(amt),
        })
    }

    fn find_account(engine: &Engine, client: u16) -> Option<&Account> {
        let target = ClientId::new(client);
        engine
            .accounts()
            .find_map(|(id, acct)| (id == target).then_some(acct))
    }

    #[test]
    fn deposit_credits_available_and_total_in_client_order() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(2, 1, "25.50")), Ok(()));
        assert_eq!(engine.apply(deposit(1, 2, "10.00")), Ok(()));
        assert_eq!(engine.apply(deposit(1, 3, "5.25")), Ok(()));

        let rows: Vec<_> = engine
            .accounts()
            .map(|(id, acct)| {
                (
                    id.get(),
                    acct.available(),
                    acct.held(),
                    acct.total(),
                    acct.locked(),
                )
            })
            .collect();

        assert_eq!(
            rows,
            vec![
                (1, amount("15.25"), Amount::ZERO, amount("15.25"), false),
                (2, amount("25.50"), Amount::ZERO, amount("25.50"), false),
            ]
        );
    }

    #[test]
    fn withdrawal_debits_available_and_total() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.00")), Ok(()));
        assert_eq!(engine.apply(withdrawal(1, 2, "3.75")), Ok(()));
        assert_eq!(engine.apply(withdrawal(1, 3, "6.25")), Ok(()));

        let acct = find_account(&engine, 1).expect("account exists");
        assert_eq!(acct.available(), Amount::ZERO);
        assert_eq!(acct.held(), Amount::ZERO);
        assert_eq!(acct.total(), Amount::ZERO);
        assert!(!acct.locked());
    }

    #[test]
    fn withdrawal_over_available_is_rejected_leaving_balances_unchanged() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.00")), Ok(()));
        assert_eq!(
            engine.apply(withdrawal(1, 2, "10.0001")),
            Err(Rejection::InsufficientFunds)
        );

        let acct = find_account(&engine, 1).expect("account exists");
        assert_eq!(acct.available(), amount("10.00"));
        assert_eq!(acct.total(), amount("10.00"));
    }

    #[test]
    fn rejected_withdrawal_for_unknown_client_still_creates_zero_account() {
        let mut engine = Engine::new();

        assert_eq!(
            engine.apply(withdrawal(7, 1, "50.00")),
            Err(Rejection::InsufficientFunds)
        );

        let acct = find_account(&engine, 7).expect("account created");
        assert_eq!(acct.available(), Amount::ZERO);
        assert_eq!(acct.held(), Amount::ZERO);
        assert_eq!(acct.total(), Amount::ZERO);
        assert!(!acct.locked());
    }

    #[test]
    fn duplicate_id_deposit_after_deposit_is_rejected() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "100.0")), Ok(()));
        assert_eq!(
            engine.apply(deposit(1, 1, "50.0")),
            Err(Rejection::DuplicateTransactionId)
        );

        let acct = find_account(&engine, 1).expect("account exists");
        assert_eq!(acct.available(), amount("100.0"));
        assert_eq!(acct.total(), amount("100.0"));
    }

    #[test]
    fn duplicate_id_after_rejected_withdrawal_is_rejected() {
        let mut engine = Engine::new();

        assert_eq!(
            engine.apply(withdrawal(2, 2, "999.0")),
            Err(Rejection::InsufficientFunds)
        );
        assert_eq!(
            engine.apply(deposit(2, 2, "40.0")),
            Err(Rejection::DuplicateTransactionId)
        );

        let acct = find_account(&engine, 2).expect("account exists");
        assert_eq!(acct.available(), Amount::ZERO);
        assert_eq!(acct.total(), Amount::ZERO);
    }

    #[test]
    fn duplicate_id_for_new_client_is_rejected_without_creating_account() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 3, "10.0")), Ok(()));
        assert_eq!(
            engine.apply(deposit(3, 3, "70.0")),
            Err(Rejection::DuplicateTransactionId)
        );

        assert!(find_account(&engine, 3).is_none());
        let ids: Vec<u16> = engine.accounts().map(|(id, _)| id.get()).collect();
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn locked_account_rejects_withdrawals_and_uses_up_their_id() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "100.0")), Ok(()));
        engine.lock_account(ClientId::new(1));

        assert_eq!(
            engine.apply(withdrawal(1, 2, "50.0")),
            Err(Rejection::AccountLocked)
        );
        // The rejected withdrawal still used up transaction id 2.
        assert_eq!(
            engine.apply(deposit(1, 2, "10.0")),
            Err(Rejection::DuplicateTransactionId)
        );
        // Locked takes precedence over insufficient funds.
        assert_eq!(
            engine.apply(withdrawal(1, 3, "500.0")),
            Err(Rejection::AccountLocked)
        );

        let acct = find_account(&engine, 1).expect("account exists");
        assert_eq!(acct.available(), amount("100.0"));
        assert_eq!(acct.total(), amount("100.0"));
        assert!(acct.locked());
    }

    #[test]
    fn locked_account_still_accepts_deposits() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "100.0")), Ok(()));
        engine.lock_account(ClientId::new(1));

        assert_eq!(engine.apply(deposit(1, 2, "25.0")), Ok(()));

        let acct = find_account(&engine, 1).expect("account exists");
        assert_eq!(acct.available(), amount("125.0"));
        assert_eq!(acct.total(), amount("125.0"));
        assert!(acct.locked());
    }

    #[test]
    fn overflow_on_available_is_rejected_without_changing_balances() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "900000000000000.0")), Ok(()));
        assert_eq!(
            engine.apply(deposit(1, 2, "900000000000000.0")),
            Err(Rejection::Overflow)
        );
        // The overflowing Deposit still used up transaction id 2.
        assert_eq!(
            engine.apply(deposit(1, 2, "1.0")),
            Err(Rejection::DuplicateTransactionId)
        );

        let acct = find_account(&engine, 1).expect("account exists");
        assert_eq!(acct.available(), amount("900000000000000.0"));
        assert_eq!(acct.held(), Amount::ZERO);
        assert_eq!(acct.total(), amount("900000000000000.0"));
    }

    #[test]
    fn overflow_on_computed_total_is_rejected_without_changing_balances() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "500000000000000.0")), Ok(()));
        engine.hold_funds(ClientId::new(1), amount("500000000000000.0"));
        assert_eq!(engine.apply(deposit(1, 2, "400000000000000.0")), Ok(()));

        // Adding 100T keeps Available at 500T (well within i64), but pushes
        // Total (Available + Held) to 1000T, past the ~922.337T i64 limit.
        assert_eq!(
            engine.apply(deposit(1, 3, "100000000000000.0")),
            Err(Rejection::Overflow)
        );
        // Transaction id 3 was still used up even though the Deposit overflowed.
        assert_eq!(
            engine.apply(deposit(1, 3, "1.0")),
            Err(Rejection::DuplicateTransactionId)
        );

        let acct = find_account(&engine, 1).expect("account exists");
        assert_eq!(acct.available(), amount("400000000000000.0"));
        assert_eq!(acct.held(), amount("500000000000000.0"));
        assert_eq!(acct.total(), amount("900000000000000.0"));
    }
}
