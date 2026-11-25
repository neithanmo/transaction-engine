//! The pipeline function: CSV in, Account report out.
//!
//! Error classes follow the README's *Input and errors*: a Malformed line is
//! skipped with a warning on stderr, a Rejected Record is skipped silently,
//! and a fatal error is returned before anything is written.

use std::fmt;
use std::io::{self, Read, Write};

use crate::csv_adapter::{self, InputError, LineOutcome, RecordReader};
use crate::engine::Engine;

/// A fatal error: the run is abandoned and no report is written.
#[derive(Debug)]
pub enum FatalError {
    /// The input could not be read as Records.
    Input(InputError),
    /// Writing the report failed.
    Output(io::Error),
}

impl fmt::Display for FatalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FatalError::Input(e) => e.fmt(f),
            FatalError::Output(e) => write!(f, "cannot write report: {e}"),
        }
    }
}

impl std::error::Error for FatalError {}

impl From<InputError> for FatalError {
    fn from(error: InputError) -> Self {
        FatalError::Input(error)
    }
}

/// Stream every Record from `input` into a fresh [`Engine`], then write the
/// Account report to `output`. Records are read one at a time; the input is
/// never collected. On a fatal input error nothing is written to `output`;
/// the report is written only after the whole input has been read.
pub fn run<R: Read, W: Write>(input: R, output: W) -> Result<(), FatalError> {
    let mut records = RecordReader::new(input)?;
    let mut engine = Engine::new();
    while let Some(LineOutcome { line, record }) = records.next_record()? {
        match record {
            // A Rejected Record is an expected domain outcome: skip it.
            Ok(record) => {
                let _ = engine.apply(record);
            }
            Err(malformed) => eprintln!("warning: line {line}: malformed record: {malformed}"),
        }
    }
    csv_adapter::write_report(output, engine.accounts()).map_err(FatalError::Output)
}
