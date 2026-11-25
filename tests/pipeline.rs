//! Fatal errors from the pipeline function write nothing.

use std::io::{self, Read};

use transaction_engine::{FatalError, run};

/// Yields `data`, then fails every later read with an I/O error.
struct FailsAfter {
    data: io::Cursor<Vec<u8>>,
}

impl Read for FailsAfter {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.data.read(buf)? {
            0 => Err(io::Error::other("disk on fire")),
            n => Ok(n),
        }
    }
}

#[test]
fn io_error_mid_stream_is_fatal_and_writes_nothing() {
    let input = FailsAfter {
        data: io::Cursor::new(b"type,client,tx,amount\ndeposit,1,1,10.0\ndeposit,1,2,".to_vec()),
    };
    let mut output = Vec::new();

    let result = run(input, &mut output);

    assert!(
        matches!(result, Err(FatalError::Input(_))),
        "expected a fatal input error, got {result:?}"
    );
    assert!(output.is_empty(), "nothing may be written on a fatal error");
}

#[test]
fn missing_header_column_is_fatal_and_writes_nothing() {
    let input: &[u8] = b"type,client,amount\ndeposit,1,10.0\n";
    let mut output = Vec::new();

    let result = run(input, &mut output);

    assert!(
        matches!(&result, Err(e) if e.to_string().contains("`tx`")),
        "expected a fatal error naming the `tx` column, got {result:?}"
    );
    assert!(output.is_empty(), "nothing may be written on a fatal error");
}
