//! Structured, stateful fuzzing of `Engine::apply`.
//!
//! libFuzzer bytes become a sequence of operations. Ids and clients come from
//! small pools so duplicates, cross-client disputes and lifecycle sequences
//! happen often. Amounts lean toward the i64 edge so Overflow paths are reached.
//! After every Record we check:
//!   * the Engine's result equals the reference model's result,
//!   * every Account's balances equal the model's,
//!   * a Rejected Record changed no balances, created no Account, and unlocked nothing,
//!   * total == available + held, held >= 0, ascending client order, Locked never clears.
#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;

use transaction_engine::{
    Amount, ClientId, DisputeAction, DisputeKind, Engine, Record, Rejection, Transaction,
    TransactionId, TransactionKind,
};
use transaction_engine_fuzz::{Model, check_amount_roundtrip, check_rows, engine_rows};

#[derive(Debug, Arbitrary)]
enum Client {
    Small(u8),
    Edge(bool),
    Raw(u16),
}

#[derive(Debug, Arbitrary)]
enum Tx {
    /// A never-used id (a counter).
    Fresh,
    /// One of the ids issued so far, counted back from the newest.
    Recent(u8),
    /// An id from a tiny pool, so collisions are common.
    Pool(u8),
    /// u32 boundaries.
    Edge(bool),
    Raw(u32),
}

#[derive(Debug, Arbitrary)]
enum Amt {
    Units(u16),
    Whole(u16),
    /// Near i64::MAX.
    NearMax(u16),
    /// i64::MAX divided by a small number, so a few of them add up to an overflow.
    Fraction(u8),
    Raw(i64),
}

#[derive(Debug, Arbitrary)]
enum Op {
    Deposit(Client, Tx, Amt),
    Withdrawal(Client, Tx, Amt),
    Dispute(Client, Tx),
    Resolve(Client, Tx),
    Chargeback(Client, Tx),
}

#[derive(Default)]
struct Gen {
    next: u32,
    issued: Vec<u32>,
}

impl Gen {
    fn client(&self, c: &Client) -> ClientId {
        ClientId::new(match *c {
            Client::Small(b) => (b % 6) as u16,
            Client::Edge(max) => {
                if max {
                    u16::MAX
                } else {
                    0
                }
            }
            Client::Raw(v) => v,
        })
    }

    fn tx(&mut self, t: &Tx) -> TransactionId {
        let id = match *t {
            Tx::Fresh => {
                self.next = self.next.wrapping_add(1);
                self.next
            }
            Tx::Recent(back) => match self.issued.len() {
                0 => 0,
                n => self.issued[n - 1 - (back as usize % n)],
            },
            Tx::Pool(b) => (b % 8) as u32,
            Tx::Edge(max) => {
                if max {
                    u32::MAX
                } else {
                    0
                }
            }
            Tx::Raw(v) => v,
        };
        self.issued.push(id);
        TransactionId::new(id)
    }
}

/// Any amount, including zero and negative: `Transaction::new` must reject
/// exactly the non-positive ones, which the target then skips.
fn amount(a: &Amt) -> Amount {
    let units = match *a {
        Amt::Units(u) => u as i64,
        Amt::Whole(w) => w as i64 * 10_000,
        Amt::NearMax(d) => i64::MAX - d as i64,
        Amt::Fraction(d) => i64::MAX / (d as i64 % 8 + 1),
        Amt::Raw(r) => r,
    };
    Amount::from_units(units)
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Ok(ops) = u.arbitrary_iter::<Op>() else {
        return;
    };

    let mut engine = Engine::new();
    let mut model = Model::default();
    let mut g = Gen::default();
    let mut before = engine_rows(&engine);

    for op in ops.take(4096) {
        let Ok(op) = op else { break };
        let record = match &op {
            Op::Deposit(c, t, a) | Op::Withdrawal(c, t, a) => {
                let kind = if matches!(op, Op::Deposit(..)) {
                    TransactionKind::Deposit
                } else {
                    TransactionKind::Withdrawal
                };
                let amount = amount(a);
                match Transaction::new(g.client(c), g.tx(t), kind, amount) {
                    Ok(transaction) => Record::Transaction(transaction),
                    Err(rejected) => {
                        assert!(amount <= Amount::ZERO, "positive {amount} refused");
                        assert_eq!(rejected.0, amount);
                        continue;
                    }
                }
            }
            Op::Dispute(c, t) | Op::Resolve(c, t) | Op::Chargeback(c, t) => {
                Record::DisputeAction(DisputeAction {
                    client: g.client(c),
                    transaction_id: g.tx(t),
                    kind: match op {
                        Op::Dispute(..) => DisputeKind::Dispute,
                        Op::Resolve(..) => DisputeKind::Resolve,
                        _ => DisputeKind::Chargeback,
                    },
                })
            }
        };

        let got = engine.apply(record);
        let want = model.apply(&record);
        assert_eq!(got, want, "result mismatch on {record:?}");

        let after = engine_rows(&engine);
        check_rows(&after);
        assert_eq!(after, model.rows(), "balances diverge after {record:?}");

        if got.is_err() {
            // A Rejected Record changes no balances. The one exception is a
            // non-duplicate Transaction for a new Client, which creates a zero Account.
            let mut expected = before.clone();
            if let Record::Transaction(t) = record {
                let c = t.client().get();
                if got != Err(Rejection::DuplicateTransactionId) {
                    if let Err(pos) = expected.binary_search_by_key(&c, |r| r.0) {
                        expected.insert(pos, (c, 0, 0, 0, false));
                    }
                }
            }
            assert_eq!(
                after, expected,
                "rejected {record:?} ({got:?}) changed state"
            );
        }

        // Locked never clears.
        for b in before.iter().filter(|b| b.4) {
            let a = after
                .iter()
                .find(|a| a.0 == b.0)
                .expect("accounts are never removed");
            assert!(a.4, "client {} was unlocked by {record:?}", b.0);
        }
        assert!(after.len() >= before.len(), "an Account disappeared");

        before = after;
    }

    model.check_internal();
    for (_, a) in engine.accounts() {
        check_amount_roundtrip(a.available());
        check_amount_roundtrip(a.held());
        check_amount_roundtrip(a.total());
    }
});
