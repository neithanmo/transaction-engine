//! Domain types, the transaction [`Engine`] and the CSV [`run`] pipeline.
//!
//! Includes the fixed-point [`Amount`], the [`ClientId`] and [`TransactionId`]
//! newtypes, the [`Record`] a single row of input turns into, and the [`Engine`]
//! that applies Records to Client [`Account`]s. The vocabulary follows
//! `GLOSSARY.md`; the amount parsing rules follow the README's
//! *Assumptions → Amounts*; transaction storage follows ADR 0001. [`run`] streams CSV input through
//! the [`Engine`] and writes the Account report; [`csv_adapter`] is the only
//! module that knows about CSV.

pub mod amount;
pub mod csv_adapter;
pub mod engine;
pub mod pipeline;
pub mod record;

pub use amount::{Amount, AmountParseError};
pub use engine::{Account, Engine, Rejection};
pub use pipeline::{FatalError, run};
pub use record::{
    ClientId, DisputeAction, DisputeKind, Record, Transaction, TransactionId, TransactionKind,
};
