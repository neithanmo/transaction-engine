//! Volume test: a synthetic input stream whose final report is known in
//! closed form, so no CSV has to be checked in.
//!
//! Every Client belongs to a cohort (`client % 3`) with a fixed per-round
//! pattern and fixed per-Client amounts:
//!
//! - **Normal trading**: deposit `d`, withdraw `w`.
//! - **Dispute and Resolve**: deposit `d`, Dispute it, Resolve it, withdraw `w`.
//! - **Fraud**: in round 0, deposit `d`, withdraw `w`, Dispute and Chargeback
//!   the Deposit (Available goes to `-w`, the Account Locks). In later rounds,
//!   deposit `d` (accepted) and withdraw `w` (Rejected: Locked).
//!
//! With noise on, the stream also carries Records that must change nothing:
//! duplicate Transaction ids, Dispute actions from the wrong Client or aimed
//! at a Withdrawal or an unknown id, Resolves of Settled Deposits, Withdrawals
//! beyond Available, a few Malformed lines per round, and padded fields with
//! mixed-case `type` values. The report must match the closed form *and* be
//! byte-identical to the same stream without noise.
//!
//! The generator builds one (round, Client) block of lines at a time into a
//! small reused buffer, so the input is never held in memory. The large run
//! is opt-in: `cargo test --release --test large_stream -- --ignored`.

use std::io::{self, Read};

use transaction_engine::{Amount, run};

/// Transaction id slots reserved per (round, Client).
const SLOTS: u64 = 8;
const DEPOSIT_SLOT: u64 = 0;
const WITHDRAWAL_SLOT: u64 = 1;
const OVERDRAW_SLOT: u64 = 2;
const MALFORMED_SLOT: u64 = 3;
/// Never used by any Transaction, so Dispute actions aimed at it find nothing.
const UNKNOWN_SLOT: u64 = 7;

/// A Client id no cohort uses: Malformed lines name it to show they never
/// create an Account.
const STRANGER: u16 = u16::MAX;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cohort {
    Normal,
    DisputeResolve,
    Fraud,
}

fn cohort(client: u16) -> Cohort {
    match client % 3 {
        0 => Cohort::Normal,
        1 => Cohort::DisputeResolve,
        _ => Cohort::Fraud,
    }
}

/// Deposit amount in 0.0001 units: four significant decimals, varies by Client.
fn deposit_units(client: u16) -> i64 {
    12_345 + i64::from(client % 50) * 10_001
}

/// Withdrawal amount in 0.0001 units: always below the Deposit amount.
fn withdrawal_units(client: u16) -> i64 {
    1_234 + i64::from(client % 17) * 101
}

/// The closed-form final (available, held, locked) for a Client.
fn expected(client: u16, rounds: u32) -> (i64, i64, bool) {
    let (d, w, r) = (
        deposit_units(client),
        withdrawal_units(client),
        i64::from(rounds),
    );
    match cohort(client) {
        Cohort::Normal | Cohort::DisputeResolve => (r * (d - w), 0, false),
        Cohort::Fraud => (-w + (r - 1) * d, 0, true),
    }
}

fn amount_text(units: i64) -> String {
    format!("{}.{:04}", units / 10_000, units % 10_000)
}

/// Generates the synthetic CSV stream one (round, Client) block at a time.
struct SyntheticStream {
    clients: u16,
    rounds: u32,
    noise: bool,
    round: u32,
    /// The next Client to generate, 1-based; `clients + 1` ends the round.
    client: u16,
    started: bool,
    block: Vec<u8>,
    pos: usize,
}

impl SyntheticStream {
    fn new(clients: u16, rounds: u32, noise: bool) -> Self {
        assert!((2..STRANGER).contains(&clients), "clients out of range");
        assert!(rounds >= 1, "at least one round");
        let max_id = u64::from(rounds) * u64::from(clients) * SLOTS;
        assert!(
            max_id <= u64::from(u32::MAX),
            "Transaction ids overflow u32"
        );
        SyntheticStream {
            clients,
            rounds,
            noise,
            round: 0,
            client: 1,
            started: false,
            block: Vec::new(),
            pos: 0,
        }
    }

    fn id(&self, round: u32, client: u16, slot: u64) -> u32 {
        let index = u64::from(round) * u64::from(self.clients) + u64::from(client - 1);
        u32::try_from(index * SLOTS + slot + 1).expect("checked in new")
    }

    /// Append one line. With noise on, fields are padded and the `type` case
    /// varies; a Dispute action then also omits the trailing amount column.
    fn line(&mut self, kind: &str, client: u16, id: u32, amount: Option<&str>) {
        let line = if self.noise {
            let kind = match id % 3 {
                0 => kind.to_ascii_uppercase(),
                1 => kind
                    .chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if i % 2 == 0 {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        }
                    })
                    .collect(),
                _ => kind.to_owned(),
            };
            match amount {
                Some(a) => format!("  {kind} , {client},{id} ,  {a}\n"),
                None => format!(" {kind},{client} , {id}\n"),
            }
        } else {
            format!("{kind},{client},{id},{}\n", amount.unwrap_or(""))
        };
        self.block.extend_from_slice(line.as_bytes());
    }

    /// Generate the next block; `false` once every round is done.
    fn refill(&mut self) -> bool {
        self.block.clear();
        self.pos = 0;
        if !self.started {
            self.started = true;
            let header = if self.noise {
                " type , client , tx , amount \n"
            } else {
                "type,client,tx,amount\n"
            };
            self.block.extend_from_slice(header.as_bytes());
            return true;
        }
        if self.client > self.clients {
            self.client = 1;
            self.round += 1;
        }
        if self.round >= self.rounds {
            return false;
        }
        let (round, client) = (self.round, self.client);
        self.client += 1;
        self.cohort_block(round, client);
        if self.noise {
            self.noise_block(round, client);
        }
        true
    }

    fn cohort_block(&mut self, round: u32, client: u16) {
        let deposit = self.id(round, client, DEPOSIT_SLOT);
        let withdrawal = self.id(round, client, WITHDRAWAL_SLOT);
        let d = amount_text(deposit_units(client));
        let w = amount_text(withdrawal_units(client));
        match cohort(client) {
            Cohort::Normal => {
                self.line("deposit", client, deposit, Some(&d));
                self.line("withdrawal", client, withdrawal, Some(&w));
            }
            Cohort::DisputeResolve => {
                self.line("deposit", client, deposit, Some(&d));
                self.line("dispute", client, deposit, None);
                self.line("resolve", client, deposit, None);
                self.line("withdrawal", client, withdrawal, Some(&w));
            }
            Cohort::Fraud if round == 0 => {
                self.line("deposit", client, deposit, Some(&d));
                self.line("withdrawal", client, withdrawal, Some(&w));
                self.line("dispute", client, deposit, None);
                self.line("chargeback", client, deposit, None);
            }
            Cohort::Fraud => {
                self.line("deposit", client, deposit, Some(&d));
                self.line("withdrawal", client, withdrawal, Some(&w));
            }
        }
    }

    /// Records that must leave every balance and Deposit state unchanged.
    fn noise_block(&mut self, round: u32, client: u16) {
        let deposit = self.id(round, client, DEPOSIT_SLOT);
        let withdrawal = self.id(round, client, WITHDRAWAL_SLOT);

        // Duplicate Transaction ids: last round's Deposit and Withdrawal again.
        if round > 0 {
            let old_deposit = self.id(round - 1, client, DEPOSIT_SLOT);
            let old_withdrawal = self.id(round - 1, client, WITHDRAWAL_SLOT);
            self.line("deposit", client, old_deposit, Some("500.0"));
            self.line("withdrawal", client, old_withdrawal, Some("0.0001"));
        }

        // Dispute actions from the wrong Client, aimed at the previous
        // Client's latest Deposit.
        let (other_round, other) = if client > 1 {
            (Some(round), client - 1)
        } else {
            (round.checked_sub(1), self.clients)
        };
        if let Some(other_round) = other_round {
            let theirs = self.id(other_round, other, DEPOSIT_SLOT);
            self.line("dispute", client, theirs, None);
            self.line("chargeback", client, theirs, None);
        }

        // Dispute actions aimed at a Withdrawal and at an unknown id.
        self.line("dispute", client, withdrawal, None);
        let unknown = self.id(round, client, UNKNOWN_SLOT);
        self.line("chargeback", client, unknown, None);

        // A Resolve of this round's Deposit, which is never Disputed by now.
        self.line("resolve", client, deposit, None);

        // A Withdrawal far beyond any balance (or from a Locked Account).
        let overdraw = self.id(round, client, OVERDRAW_SLOT);
        self.line("withdrawal", client, overdraw, Some("1000000000.0"));

        // A few Malformed lines once per round. Each warns on stderr, so they
        // stay rare (see the README's Input and errors).
        if client == 1 {
            let id = self.id(round, client, MALFORMED_SLOT);
            let lines = [
                format!("sprinkle,{STRANGER},{id},1.0\n"),
                format!("deposit,{STRANGER},{id},abc\n"),
                format!("deposit,{STRANGER},{id},1.23456\n"),
                format!("deposit,{STRANGER},{id},-3.0\n"),
                format!("withdrawal,{STRANGER}\n"),
            ];
            for line in lines {
                self.block.extend_from_slice(line.as_bytes());
            }
        }
    }
}

impl Read for SyntheticStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.pos == self.block.len() {
            if !self.refill() {
                return Ok(0);
            }
        }
        let n = buf.len().min(self.block.len() - self.pos);
        buf[..n].copy_from_slice(&self.block[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

fn report(clients: u16, rounds: u32, noise: bool) -> String {
    let mut output = Vec::new();
    run(SyntheticStream::new(clients, rounds, noise), &mut output).expect("no fatal error");
    String::from_utf8(output).expect("report is UTF-8")
}

/// Check every report row against the closed form, and that no other
/// Account (such as the Malformed lines' stranger) exists.
fn assert_matches_closed_form(report: &str, clients: u16, rounds: u32) {
    let mut lines = report.lines();
    assert_eq!(lines.next(), Some("client,available,held,total,locked"));

    let mut seen = 0u16;
    for line in lines {
        seen += 1;
        let fields: Vec<&str> = line.split(',').collect();
        let [client, available, held, total, locked] = fields[..] else {
            panic!("bad report line {line:?}");
        };
        let client: u16 = client.parse().expect("client id");
        assert_eq!(client, seen, "rows are one per Client in client order");

        let units = |s: &str| s.parse::<Amount>().expect("amount").units();
        let (want_available, want_held, want_locked) = expected(client, rounds);
        assert_eq!(
            (
                units(available),
                units(held),
                units(total),
                locked == "true"
            ),
            (
                want_available,
                want_held,
                want_available + want_held,
                want_locked
            ),
            "client {client} (row {line:?})"
        );
    }
    assert_eq!(seen, clients, "exactly one Account per generated Client");
}

fn check(clients: u16, rounds: u32) {
    let quiet = report(clients, rounds, false);
    assert_matches_closed_form(&quiet, clients, rounds);

    let noisy = report(clients, rounds, true);
    assert_eq!(noisy, quiet, "noise must have no net effect on the report");
}

#[test]
fn small_synthetic_stream_matches_closed_form_and_ignores_noise() {
    check(30, 5);
}

#[test]
#[ignore = "volume run: cargo test --release --test large_stream -- --ignored"]
fn large_synthetic_stream_matches_closed_form_and_ignores_noise() {
    check(10_000, 200);
}
