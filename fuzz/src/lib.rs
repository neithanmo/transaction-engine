//! Shared fuzzing support: an independent reference model of the Engine and
//! helpers to compare it against the real one.
//!
//! The model is deliberately written differently from `src/engine.rs`: it does
//! all arithmetic in `i128` and checks the `i64` range afterwards, instead of
//! using checked `i64` operations. A disagreement on any Record's outcome or on
//! any balance is a bug in one of them.

use std::collections::{BTreeMap, HashMap, HashSet};

use transaction_engine::{
    Account, Amount, DisputeAction, DisputeKind, Engine, Record, Rejection, Transaction,
    TransactionKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Settled,
    Disputed,
    ChargedBack,
}

#[derive(Debug, Clone, Default)]
struct ModelAccount {
    available: i128,
    held: i128,
    locked: bool,
    /// Deposits by transaction id: (amount, state).
    deposits: HashMap<u32, (i128, State)>,
}

/// Reference model of the Engine. See the module docs.
#[derive(Debug, Default)]
pub struct Model {
    seen: HashSet<u32>,
    accounts: BTreeMap<u16, ModelAccount>,
}

/// One Account row as the report shows it: (client, available, held, total, locked).
pub type Row = (u16, i64, i64, i64, bool);

fn fits(v: i128) -> bool {
    i64::MIN as i128 <= v && v <= i64::MAX as i128
}

/// Every balance an Account stores or reports must fit in an `i64`.
fn balances_fit(available: i128, held: i128) -> bool {
    fits(available) && fits(held) && fits(available + held)
}

impl Model {
    pub fn apply(&mut self, record: &Record) -> Result<(), Rejection> {
        match *record {
            Record::Transaction(Transaction {
                client,
                transaction_id,
                kind,
                amount,
            }) => {
                let id = transaction_id.get();
                if self.seen.contains(&id) {
                    return Err(Rejection::DuplicateTransactionId);
                }
                self.seen.insert(id);
                let amount = amount.units() as i128;
                let acct = self.accounts.entry(client.get()).or_default();
                match kind {
                    TransactionKind::Deposit => {
                        let available = acct.available + amount;
                        if !balances_fit(available, acct.held) {
                            return Err(Rejection::Overflow);
                        }
                        acct.available = available;
                        acct.deposits.insert(id, (amount, State::Settled));
                        Ok(())
                    }
                    TransactionKind::Withdrawal => {
                        if acct.locked {
                            return Err(Rejection::AccountLocked);
                        }
                        if acct.available < amount {
                            return Err(Rejection::InsufficientFunds);
                        }
                        let available = acct.available - amount;
                        if !balances_fit(available, acct.held) {
                            return Err(Rejection::Overflow);
                        }
                        acct.available = available;
                        Ok(())
                    }
                }
            }
            Record::DisputeAction(DisputeAction {
                client,
                transaction_id,
                kind,
            }) => {
                let acct = self
                    .accounts
                    .get_mut(&client.get())
                    .ok_or(Rejection::NoSuchDeposit)?;
                let &(amount, state) = acct
                    .deposits
                    .get(&transaction_id.get())
                    .ok_or(Rejection::NoSuchDeposit)?;
                let (next, available, held, locks) = match (kind, state) {
                    (DisputeKind::Dispute, State::Settled) => (
                        State::Disputed,
                        acct.available - amount,
                        acct.held + amount,
                        false,
                    ),
                    (DisputeKind::Resolve, State::Disputed) => (
                        State::Settled,
                        acct.available + amount,
                        acct.held - amount,
                        false,
                    ),
                    (DisputeKind::Chargeback, State::Disputed) => (
                        State::ChargedBack,
                        acct.available,
                        acct.held - amount,
                        true,
                    ),
                    _ => return Err(Rejection::WrongState),
                };
                if !balances_fit(available, held) {
                    return Err(Rejection::Overflow);
                }
                acct.available = available;
                acct.held = held;
                acct.locked |= locks;
                acct.deposits.insert(transaction_id.get(), (amount, next));
                Ok(())
            }
        }
    }

    /// Model rows in ascending client order.
    pub fn rows(&self) -> Vec<Row> {
        self.accounts
            .iter()
            .map(|(&c, a)| {
                (
                    c,
                    a.available as i64,
                    a.held as i64,
                    (a.available + a.held) as i64,
                    a.locked,
                )
            })
            .collect()
    }

    /// Held must equal the sum of that Account's Disputed Deposits.
    pub fn check_internal(&self) {
        for (client, a) in &self.accounts {
            let disputed: i128 = a
                .deposits
                .values()
                .filter(|(_, s)| *s == State::Disputed)
                .map(|(amt, _)| amt)
                .sum();
            assert_eq!(a.held, disputed, "model: client {client} held != sum of disputed");
        }
    }
}

pub fn row(client: u16, a: &Account) -> Row {
    (
        client,
        a.available().units(),
        a.held().units(),
        a.total().units(),
        a.locked(),
    )
}

/// Engine rows in the order `Engine::accounts` yields them.
pub fn engine_rows(engine: &Engine) -> Vec<Row> {
    engine.accounts().map(|(c, a)| row(c.get(), a)).collect()
}

/// Invariants of an Engine snapshot that need no model.
pub fn check_rows(rows: &[Row]) {
    for w in rows.windows(2) {
        assert!(w[0].0 < w[1].0, "accounts not strictly ascending: {w:?}");
    }
    for &(client, available, held, total, _) in rows {
        assert_eq!(
            available as i128 + held as i128,
            total as i128,
            "client {client}: total != available + held"
        );
        assert!(held >= 0, "client {client}: held is negative ({held})");
    }
}

/// `Amount`'s Display output must parse back to the same value
/// (except `i64::MIN`, which is documented as not parseable).
pub fn check_amount_roundtrip(a: Amount) {
    if a.units() == i64::MIN {
        return;
    }
    let shown = a.to_string();
    let back: Amount = shown
        .parse()
        .unwrap_or_else(|e| panic!("formatted amount {shown:?} does not parse: {e:?}"));
    assert_eq!(back, a, "amount {shown:?} did not round-trip");
}
