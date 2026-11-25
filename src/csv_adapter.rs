//! The CSV adapter: the only module that knows about CSV.
//!
//! It turns each input row into a domain [`Record`] or a [`Malformed`] error,
//! and writes the Account report. The input rules follow the README's
//! *Input and errors* and *Assumptions → Amounts*.

use std::fmt;
use std::io::{self, Read, Write};

use csv::{ReaderBuilder, StringRecord, Trim};

use crate::amount::{Amount, AmountParseError};
use crate::engine::Account;
use crate::record::{
    ClientId, DisputeAction, DisputeKind, Record, Transaction, TransactionId, TransactionKind,
};

/// A fatal input problem: the stream cannot be read as Records at all.
#[derive(Debug)]
pub enum InputError {
    /// The header row lacks a required column.
    MissingColumn(&'static str),
    /// Reading the underlying stream failed.
    Io(io::Error),
    /// Any other CSV failure that is not about a single row.
    Csv(csv::Error),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::MissingColumn(column) => {
                write!(f, "input header is missing the `{column}` column")
            }
            InputError::Io(e) => write!(f, "cannot read input: {e}"),
            InputError::Csv(e) => write!(f, "cannot read input: {e}"),
        }
    }
}

impl std::error::Error for InputError {}

impl From<csv::Error> for InputError {
    fn from(error: csv::Error) -> Self {
        if error.is_io_error() {
            match error.into_kind() {
                csv::ErrorKind::Io(e) => InputError::Io(e),
                _ => unreachable!("is_io_error implies ErrorKind::Io"),
            }
        } else {
            InputError::Csv(error)
        }
    }
}

/// Why one input line could not be understood as a [`Record`]. The row is skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Malformed {
    /// The `type` is none of deposit, withdrawal, dispute, resolve, chargeback.
    UnknownType(String),
    /// A required field is absent or empty.
    MissingField(&'static str),
    /// The `client` field is not a `u16`.
    InvalidClient(String),
    /// The Transaction id field is not a `u32`.
    InvalidTransactionId(String),
    /// The `amount` field is not a valid Amount.
    InvalidAmount(AmountParseError),
    /// A Deposit or Withdrawal amount is zero or negative.
    NonPositiveAmount(Amount),
    /// The line is not valid UTF-8.
    InvalidUtf8,
}

impl fmt::Display for Malformed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Malformed::UnknownType(t) => write!(f, "unknown type `{t}`"),
            Malformed::MissingField(field) => write!(f, "missing `{field}` field"),
            Malformed::InvalidClient(v) => write!(f, "invalid client `{v}`"),
            Malformed::InvalidTransactionId(v) => write!(f, "invalid transaction id `{v}`"),
            Malformed::InvalidAmount(e) => write!(f, "invalid amount: {e}"),
            Malformed::NonPositiveAmount(a) => write!(f, "amount {a} is not positive"),
            Malformed::InvalidUtf8 => f.write_str("line is not valid UTF-8"),
        }
    }
}

// Input column names, matched exactly (after trimming) against the header.
const KIND: &str = "type";
const CLIENT: &str = "client";
const TRANSACTION_ID: &str = "tx";
const AMOUNT: &str = "amount";

/// Column positions, found by header name.
struct Columns {
    kind: usize,
    client: usize,
    transaction_id: usize,
    amount: usize,
}

impl Columns {
    fn from_header(header: &StringRecord) -> Result<Self, InputError> {
        let find = |name: &'static str| {
            header
                .iter()
                .position(|h| h == name)
                .ok_or(InputError::MissingColumn(name))
        };
        Ok(Columns {
            kind: find(KIND)?,
            client: find(CLIENT)?,
            transaction_id: find(TRANSACTION_ID)?,
            amount: find(AMOUNT)?,
        })
    }

    /// Understand one line's fields as a [`Record`]. The `type` matches
    /// case-insensitively, and a Dispute action's amount is ignored.
    fn parse(&self, fields: &StringRecord) -> Result<Record, Malformed> {
        let field = |index: usize, name: &'static str| {
            fields
                .get(index)
                .filter(|value| !value.is_empty())
                .ok_or(Malformed::MissingField(name))
        };

        let kind = field(self.kind, KIND)?;
        let client = field(self.client, CLIENT)?;
        let client = client
            .parse::<u16>()
            .map(ClientId::new)
            .map_err(|_| Malformed::InvalidClient(client.to_owned()))?;
        let transaction_id = field(self.transaction_id, TRANSACTION_ID)?;
        let transaction_id = transaction_id
            .parse::<u32>()
            .map(TransactionId::new)
            .map_err(|_| Malformed::InvalidTransactionId(transaction_id.to_owned()))?;

        let transaction = |kind| -> Result<Record, Malformed> {
            let amount: Amount = field(self.amount, AMOUNT)?
                .parse()
                .map_err(Malformed::InvalidAmount)?;
            if amount <= Amount::ZERO {
                return Err(Malformed::NonPositiveAmount(amount));
            }
            Ok(Record::Transaction(Transaction {
                client,
                transaction_id,
                kind,
                amount,
            }))
        };
        let dispute_action = |kind| {
            Ok(Record::DisputeAction(DisputeAction {
                client,
                transaction_id,
                kind,
            }))
        };

        match kind.to_ascii_lowercase().as_str() {
            "deposit" => transaction(TransactionKind::Deposit),
            "withdrawal" => transaction(TransactionKind::Withdrawal),
            "dispute" => dispute_action(DisputeKind::Dispute),
            "resolve" => dispute_action(DisputeKind::Resolve),
            "chargeback" => dispute_action(DisputeKind::Chargeback),
            _ => Err(Malformed::UnknownType(kind.to_owned())),
        }
    }
}

/// What one input line turned into: a [`Record`], or why it is Malformed.
#[derive(Debug)]
pub struct LineOutcome {
    /// The 1-based input line number, for warnings.
    pub line: u64,
    /// The understood Record, or the reason it was not understood.
    pub record: Result<Record, Malformed>,
}

/// Streams [`Record`]s out of CSV input, one line at a time.
pub struct RecordReader<R> {
    reader: csv::Reader<R>,
    columns: Columns,
    buffer: StringRecord,
}

impl<R: Read> RecordReader<R> {
    /// Read and validate the header. A missing column is fatal.
    pub fn new(input: R) -> Result<Self, InputError> {
        let mut reader = ReaderBuilder::new()
            .trim(Trim::All)
            .flexible(true)
            .from_reader(input);
        let columns = Columns::from_header(reader.headers()?)?;
        Ok(RecordReader {
            reader,
            columns,
            buffer: StringRecord::new(),
        })
    }

    /// The next line's outcome, `None` at end of input, or a fatal error.
    /// One buffer is reused, so memory does not grow with input size.
    pub fn next_record(&mut self) -> Result<Option<LineOutcome>, InputError> {
        match self.reader.read_record(&mut self.buffer) {
            Ok(false) => Ok(None),
            Ok(true) => Ok(Some(LineOutcome {
                line: self.buffer.position().map_or(0, csv::Position::line),
                record: self.columns.parse(&self.buffer),
            })),
            Err(error) => match error.kind() {
                csv::ErrorKind::Utf8 { pos, .. } => Ok(Some(LineOutcome {
                    line: pos.as_ref().map_or(0, csv::Position::line),
                    record: Err(Malformed::InvalidUtf8),
                })),
                _ => Err(error.into()),
            },
        }
    }
}

/// Write the Account report: a header, then one row per Account in the
/// order given, with every amount at four decimal places.
pub fn write_report<'a, W: Write>(
    output: W,
    accounts: impl Iterator<Item = (ClientId, &'a Account)>,
) -> io::Result<()> {
    let mut writer = csv::Writer::from_writer(output);
    writer.write_record(["client", "available", "held", "total", "locked"])?;
    for (client, account) in accounts {
        writer.write_record([
            client.get().to_string(),
            account.available().to_string(),
            account.held().to_string(),
            account.total().to_string(),
            account.locked().to_string(),
        ])?;
    }
    writer.flush()
}
