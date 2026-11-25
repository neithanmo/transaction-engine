//! Domain primitives for the transaction engine.
//!
//! These are the types every other module builds on: the fixed-point
//! [`Amount`], the [`ClientId`] and [`TransactionId`] newtypes, and the
//! [`Record`] a single row of input turns into. The vocabulary follows
//! `GLOSSARY.md`; the amount parsing rules follow the README's
//! *Assumptions → Amounts*.

pub mod amount;
pub mod record;

pub use amount::{Amount, AmountParseError};
pub use record::{
    ClientId, DisputeAction, DisputeKind, Record, Transaction, TransactionId, TransactionKind,
};
