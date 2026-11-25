//! The domain [`Engine`] that applies [`Record`]s to Client [`Account`]s.
//!
//! The engine knows nothing about CSV or I/O. Vocabulary follows `GLOSSARY.md`
//! and transaction storage follows ADR 0001.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use crate::amount::Amount;
use crate::record::{ClientId, DisputeAction, DisputeKind, Record, TransactionId, TransactionKind};

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
    /// A Dispute action named no Deposit in its Client's Account: unknown
    /// Client, unknown id, a Withdrawal's id, or another Client's Deposit.
    NoSuchDeposit,
    /// A Dispute action does not fit the targeted Deposit's lifecycle state.
    WrongState,
}

impl fmt::Display for Rejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Rejection::DuplicateTransactionId => "duplicate transaction id",
            Rejection::InsufficientFunds => "insufficient available funds",
            Rejection::AccountLocked => "account is locked",
            Rejection::Overflow => "balance overflow",
            Rejection::NoSuchDeposit => "no such deposit for this client",
            Rejection::WrongState => "deposit is in the wrong state for this action",
        };
        f.write_str(msg)
    }
}

/// Lifecycle state of a stored Deposit (Settled, Disputed, Charged back).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepositState {
    Settled,
    Disputed,
    ChargedBack,
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

    /// Commit new Available and Held, rejecting them if Total would overflow.
    fn set_balances(&mut self, available: Amount, held: Amount) -> Result<(), Rejection> {
        available.checked_add(held).ok_or(Rejection::Overflow)?;
        self.available = available;
        self.held = held;
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
        self.set_balances(new_available, self.held)?;
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
        self.set_balances(new_available, self.held)
    }

    fn apply_dispute_action(
        &mut self,
        transaction_id: TransactionId,
        kind: DisputeKind,
    ) -> Result<(), Rejection> {
        let deposit = *self
            .deposits
            .get(&transaction_id)
            .ok_or(Rejection::NoSuchDeposit)?;
        let amount = deposit.amount;
        // Each legal transition: (next state, new Available, new Held, locks?).
        let (next_state, available, held, locks) = match (kind, deposit.state) {
            (DisputeKind::Dispute, DepositState::Settled) => (
                DepositState::Disputed,
                self.available.checked_sub(amount),
                self.held.checked_add(amount),
                false,
            ),
            (DisputeKind::Resolve, DepositState::Disputed) => (
                DepositState::Settled,
                self.available.checked_add(amount),
                self.held.checked_sub(amount),
                false,
            ),
            (DisputeKind::Chargeback, DepositState::Disputed) => (
                DepositState::ChargedBack,
                Some(self.available),
                self.held.checked_sub(amount),
                true,
            ),
            _ => return Err(Rejection::WrongState),
        };
        let (Some(available), Some(held)) = (available, held) else {
            return Err(Rejection::Overflow);
        };
        self.set_balances(available, held)?;
        self.deposits.insert(
            transaction_id,
            Deposit {
                state: next_state,
                ..deposit
            },
        );
        self.locked |= locks;
        Ok(())
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
            Record::DisputeAction(DisputeAction {
                client,
                transaction_id,
                kind,
            }) => self
                .accounts
                .get_mut(&client)
                .ok_or(Rejection::NoSuchDeposit)?
                .apply_dispute_action(transaction_id, kind),
        }
    }

    /// Read all Accounts in ascending [`ClientId`] order.
    pub fn accounts(&self) -> impl Iterator<Item = (ClientId, &Account)> {
        self.accounts
            .iter()
            .map(|(&client, account)| (client, account))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{DisputeAction, DisputeKind, Transaction, TransactionId, TransactionKind};

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

    fn dispute_action(client: u16, transaction_id: u32, kind: DisputeKind) -> Record {
        Record::DisputeAction(DisputeAction {
            client: ClientId::new(client),
            transaction_id: TransactionId::new(transaction_id),
            kind,
        })
    }

    fn dispute(client: u16, transaction_id: u32) -> Record {
        dispute_action(client, transaction_id, DisputeKind::Dispute)
    }

    fn resolve(client: u16, transaction_id: u32) -> Record {
        dispute_action(client, transaction_id, DisputeKind::Resolve)
    }

    fn chargeback(client: u16, transaction_id: u32) -> Record {
        dispute_action(client, transaction_id, DisputeKind::Chargeback)
    }

    /// (available, held, total, locked) for a Client's Account.
    fn balances(engine: &Engine, client: u16) -> (Amount, Amount, Amount, bool) {
        let acct = find_account(engine, client).expect("account exists");
        (acct.available(), acct.held(), acct.total(), acct.locked())
    }

    /// Lock a Client's Account the real way: a throwaway 1.0 Deposit under
    /// `transaction_id`, Disputed and then Charged back. Balances are unchanged.
    fn lock_via_chargeback(engine: &mut Engine, client: u16, transaction_id: u32) {
        assert_eq!(engine.apply(deposit(client, transaction_id, "1.0")), Ok(()));
        assert_eq!(engine.apply(dispute(client, transaction_id)), Ok(()));
        assert_eq!(engine.apply(chargeback(client, transaction_id)), Ok(()));
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
        lock_via_chargeback(&mut engine, 1, 90);

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
        lock_via_chargeback(&mut engine, 1, 90);

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
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
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

    #[test]
    fn dispute_moves_deposit_amount_from_available_to_held() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(deposit(1, 2, "4.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));

        assert_eq!(
            balances(&engine, 1),
            (amount("4.0"), amount("10.0"), amount("14.0"), false)
        );
    }

    #[test]
    fn resolve_releases_held_amount_back_to_available() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        assert_eq!(engine.apply(resolve(1, 1)), Ok(()));

        assert_eq!(
            balances(&engine, 1),
            (amount("10.0"), Amount::ZERO, amount("10.0"), false)
        );
    }

    #[test]
    fn chargeback_removes_held_amount_and_locks_account() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(deposit(1, 2, "3.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        assert_eq!(engine.apply(chargeback(1, 1)), Ok(()));

        assert_eq!(
            balances(&engine, 1),
            (amount("3.0"), Amount::ZERO, amount("3.0"), true)
        );
    }

    /// Applies `setup`, snapshots balances, then asserts `illegal` is Rejected
    /// as wrong state and leaves the snapshot untouched.
    fn assert_wrong_state(setup: &[Record], illegal: Record) {
        let mut engine = Engine::new();
        for record in setup {
            assert_eq!(engine.apply(*record), Ok(()), "setup record {record:?}");
        }
        let before = balances(&engine, 1);

        assert_eq!(engine.apply(illegal), Err(Rejection::WrongState));
        assert_eq!(balances(&engine, 1), before);
    }

    #[test]
    fn disputing_a_disputed_deposit_is_wrong_state() {
        assert_wrong_state(&[deposit(1, 1, "10.0"), dispute(1, 1)], dispute(1, 1));
    }

    #[test]
    fn resolving_a_settled_deposit_is_wrong_state() {
        assert_wrong_state(&[deposit(1, 1, "10.0")], resolve(1, 1));
    }

    #[test]
    fn charging_back_a_settled_deposit_is_wrong_state() {
        assert_wrong_state(&[deposit(1, 1, "10.0")], chargeback(1, 1));
    }

    #[test]
    fn every_dispute_action_on_a_charged_back_deposit_is_wrong_state() {
        let charged_back = [deposit(1, 1, "10.0"), dispute(1, 1), chargeback(1, 1)];
        assert_wrong_state(&charged_back, dispute(1, 1));
        assert_wrong_state(&charged_back, resolve(1, 1));
        assert_wrong_state(&charged_back, chargeback(1, 1));

        // Charged back is final: repeated attempts never reopen it.
        let mut engine = Engine::new();
        for record in charged_back {
            assert_eq!(engine.apply(record), Ok(()));
        }
        for record in [
            dispute(1, 1),
            resolve(1, 1),
            chargeback(1, 1),
            dispute(1, 1),
        ] {
            assert_eq!(engine.apply(record), Err(Rejection::WrongState));
        }
    }

    #[test]
    fn chargeback_after_resolve_is_wrong_state() {
        assert_wrong_state(
            &[deposit(1, 1, "10.0"), dispute(1, 1), resolve(1, 1)],
            chargeback(1, 1),
        );
    }

    #[test]
    fn rejected_dispute_action_leaves_deposit_state_unchanged() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(resolve(1, 1)), Err(Rejection::WrongState));
        // Still Settled: a Dispute is legal, and a second one is not.
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Err(Rejection::WrongState));
        // Still Disputed: a Chargeback is legal.
        assert_eq!(engine.apply(chargeback(1, 1)), Ok(()));
    }

    #[test]
    fn dispute_action_for_unknown_client_is_no_such_deposit_and_creates_no_account() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        for record in [dispute(9, 1), resolve(9, 1), chargeback(9, 1)] {
            assert_eq!(engine.apply(record), Err(Rejection::NoSuchDeposit));
        }

        let ids: Vec<u16> = engine.accounts().map(|(id, _)| id.get()).collect();
        assert_eq!(ids, vec![1]);
        assert_eq!(
            balances(&engine, 1),
            (amount("10.0"), Amount::ZERO, amount("10.0"), false)
        );
    }

    #[test]
    fn dispute_action_from_wrong_client_is_no_such_deposit() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(deposit(2, 2, "5.0")), Ok(()));
        for record in [dispute(2, 1), resolve(2, 1), chargeback(2, 1)] {
            assert_eq!(engine.apply(record), Err(Rejection::NoSuchDeposit));
        }

        assert_eq!(
            balances(&engine, 1),
            (amount("10.0"), Amount::ZERO, amount("10.0"), false)
        );
        assert_eq!(
            balances(&engine, 2),
            (amount("5.0"), Amount::ZERO, amount("5.0"), false)
        );
        // Client 1's Deposit is still Settled and disputable by its owner.
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
    }

    #[test]
    fn dispute_action_aimed_at_withdrawal_is_no_such_deposit() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(withdrawal(1, 2, "4.0")), Ok(()));
        for record in [dispute(1, 2), resolve(1, 2), chargeback(1, 2)] {
            assert_eq!(engine.apply(record), Err(Rejection::NoSuchDeposit));
        }

        assert_eq!(
            balances(&engine, 1),
            (amount("6.0"), Amount::ZERO, amount("6.0"), false)
        );
    }

    #[test]
    fn dispute_action_aimed_at_unknown_id_is_no_such_deposit() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 99)), Err(Rejection::NoSuchDeposit));

        assert_eq!(
            balances(&engine, 1),
            (amount("10.0"), Amount::ZERO, amount("10.0"), false)
        );
    }

    #[test]
    fn dispute_overflowing_held_is_rejected_leaving_balances_and_state_unchanged() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "900000000000000.0")), Ok(()));
        assert_eq!(engine.apply(withdrawal(1, 2, "900000000000000.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        assert_eq!(engine.apply(deposit(1, 3, "900000000000000.0")), Ok(()));
        let before = balances(&engine, 1);
        assert_eq!(
            before,
            (
                Amount::ZERO,
                amount("900000000000000.0"),
                amount("900000000000000.0"),
                false
            )
        );

        // Held would reach 1800T, past the ~922.337T i64 limit.
        assert_eq!(engine.apply(dispute(1, 3)), Err(Rejection::Overflow));
        assert_eq!(balances(&engine, 1), before);
        // Deposit 3 is still Settled, so Resolve is the wrong state for it.
        assert_eq!(engine.apply(resolve(1, 3)), Err(Rejection::WrongState));
    }

    #[test]
    fn dispute_resolve_dispute_chargeback_follows_the_lifecycle() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        assert_eq!(engine.apply(resolve(1, 1)), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        assert_eq!(
            balances(&engine, 1),
            (Amount::ZERO, amount("10.0"), amount("10.0"), false)
        );
        assert_eq!(engine.apply(chargeback(1, 1)), Ok(()));

        assert_eq!(
            balances(&engine, 1),
            (Amount::ZERO, Amount::ZERO, Amount::ZERO, true)
        );
    }

    #[test]
    fn chargeback_after_withdrawal_leaves_available_and_total_negative() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(withdrawal(1, 2, "8.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        // The Dispute pushes Available below zero; Held carries the full amount.
        assert_eq!(
            balances(&engine, 1),
            (amount("-8.0"), amount("10.0"), amount("2.0"), false)
        );
        assert_eq!(engine.apply(chargeback(1, 1)), Ok(()));

        assert_eq!(
            balances(&engine, 1),
            (amount("-8.0"), Amount::ZERO, amount("-8.0"), true)
        );
    }

    #[test]
    fn locked_account_deposit_can_be_disputed_and_charged_back_but_withdrawal_is_rejected() {
        let mut engine = Engine::new();

        assert_eq!(engine.apply(deposit(1, 1, "10.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 1)), Ok(()));
        assert_eq!(engine.apply(chargeback(1, 1)), Ok(()));
        assert!(find_account(&engine, 1).expect("account exists").locked());

        assert_eq!(engine.apply(deposit(1, 2, "20.0")), Ok(()));
        assert_eq!(engine.apply(dispute(1, 2)), Ok(()));
        assert_eq!(
            balances(&engine, 1),
            (Amount::ZERO, amount("20.0"), amount("20.0"), true)
        );
        assert_eq!(engine.apply(resolve(1, 2)), Ok(()));
        assert_eq!(engine.apply(dispute(1, 2)), Ok(()));
        assert_eq!(engine.apply(chargeback(1, 2)), Ok(()));
        assert_eq!(
            engine.apply(withdrawal(1, 3, "1.0")),
            Err(Rejection::AccountLocked)
        );

        assert_eq!(
            balances(&engine, 1),
            (Amount::ZERO, Amount::ZERO, Amount::ZERO, true)
        );
    }
}
