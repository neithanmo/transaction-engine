//! The pipeline function: CSV in, Account report out.
//!
//! Error classes follow the README's *Input and errors*: a Malformed line is
//! skipped with a warning on stderr (the first [`MAX_WARNINGS`], then one line
//! counting the rest), a Rejected Record is skipped silently, and a fatal
//! error is returned before anything is written.

use std::fmt;
use std::io::{self, Read, Write};

use crate::csv_adapter::{self, InputError, LineOutcome, Malformed, RecordReader};
use crate::engine::Engine;

/// How many Malformed lines get their own warning. Past this, they are only
/// counted, so an input full of bad lines cannot flood stderr.
pub const MAX_WARNINGS: u64 = 100;

/// Warns about the first [`MAX_WARNINGS`] Malformed lines and counts the rest.
#[derive(Default)]
struct Warnings {
    malformed: u64,
}

impl Warnings {
    fn malformed(&mut self, line: u64, reason: &Malformed) {
        self.malformed += 1;
        if self.malformed <= MAX_WARNINGS {
            eprintln!("warning: line {line}: malformed record: {reason}");
        }
    }

    /// Print how many warnings were omitted, if any.
    fn finish(self) {
        let omitted = self.malformed.saturating_sub(MAX_WARNINGS);
        if omitted > 0 {
            eprintln!("warning: {omitted} additional malformed records omitted");
        }
    }
}

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
    let mut warnings = Warnings::default();
    let read = loop {
        match records.next_record() {
            Ok(Some(LineOutcome { line, record })) => match record {
                // A Rejected Record is an expected domain outcome: skip it.
                Ok(record) => {
                    let _ = engine.apply(record);
                }
                Err(reason) => warnings.malformed(line, &reason),
            },
            Ok(None) => break Ok(()),
            Err(fatal) => break Err(fatal),
        }
    };
    // The count of omitted warnings is printed even when the run is fatal.
    warnings.finish();
    read?;
    csv_adapter::write_report(output, engine.accounts()).map_err(FatalError::Output)
}
