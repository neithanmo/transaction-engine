//! Command-line entry point: `transaction-engine <input.csv>`.
//!
//! The domain and the CSV pipeline live in the `transaction_engine` library;
//! this binary only checks its arguments, opens the file and reports a fatal
//! error as a stderr message with a non-zero exit.

use std::env;
use std::fs::File;
use std::io;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [path] = args.as_slice() else {
        eprintln!("usage: transaction-engine <input.csv>");
        return ExitCode::FAILURE;
    };

    let input = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("error: cannot open {}: {e}", path.to_string_lossy());
            return ExitCode::FAILURE;
        }
    };

    match transaction_engine::run(input, io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
