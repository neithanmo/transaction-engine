//! Domain types and transaction [`Engine`].
//!
//! Includes the fixed-point [`Amount`], the [`ClientId`] and [`TransactionId`]
//! newtypes, the [`Record`] a single row of input turns into, and the [`Engine`]
//! that applies Records to Client [`Account`]s. The vocabulary follows
//! `GLOSSARY.md`; the amount parsing rules follow the README's
//! *Assumptions → Amounts*; transaction storage follows ADR 0001.

pub mod amount;
pub mod engine;
pub mod record;

pub use amount::{Amount, AmountParseError};
pub use engine::{Account, Engine, Rejection};
pub use record::{
    ClientId, DisputeAction, DisputeKind, Record, Transaction, TransactionId, TransactionKind,
};
