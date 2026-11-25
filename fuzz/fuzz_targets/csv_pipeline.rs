//! End-to-end fuzzing of `run()` on raw CSV bytes, seeded with the fixtures.
//!
//! Checks:
//!   * `run` never panics, whatever the bytes,
//!   * a fatal error writes nothing to the output,
//!   * on success the report has the documented header, five fields per row,
//!     ascending client ids, total == available + held, and held >= 0,
//!   * the report equals what the reference model computes from the same
//!     Records (read through `RecordReader`, as the pipeline does).
#![no_main]

use libfuzzer_sys::fuzz_target;

use transaction_engine::csv_adapter::RecordReader;
use transaction_engine::{Amount, run};
use transaction_engine_fuzz::{Model, Row, check_rows};

/// Replay the input through `RecordReader` and the model. `None` if fatal.
fn model_report(data: &[u8]) -> Option<Vec<Row>> {
    let mut reader = RecordReader::new(data).ok()?;
    let mut model = Model::default();
    while let Some(outcome) = reader.next_record().ok()? {
        if let Ok(record) = outcome.record {
            let _ = model.apply(&record);
        }
    }
    model.check_internal();
    Some(model.rows())
}

fn parse_report(out: &[u8]) -> Vec<Row> {
    let mut reader = csv::ReaderBuilder::new().has_headers(true).from_reader(out);
    let header = reader.headers().expect("report header readable").clone();
    assert_eq!(
        header.iter().collect::<Vec<_>>(),
        ["client", "available", "held", "total", "locked"],
        "report header"
    );
    let amount = |s: &str| -> i64 {
        s.parse::<Amount>()
            .unwrap_or_else(|e| panic!("report amount {s:?} unparseable: {e:?}"))
            .units()
    };
    reader
        .records()
        .map(|r| {
            let r = r.expect("report row readable");
            assert_eq!(r.len(), 5, "report row width: {r:?}");
            let locked = match &r[4] {
                "true" => true,
                "false" => false,
                other => panic!("report locked field {other:?}"),
            };
            (
                r[0].parse().expect("report client is u16"),
                amount(&r[1]),
                amount(&r[2]),
                amount(&r[3]),
                locked,
            )
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let mut out = Vec::new();
    let result = run(data, &mut out);
    let expected = model_report(data);

    match result {
        Err(_) => {
            assert!(out.is_empty(), "fatal error but output was written");
            assert!(
                expected.is_none(),
                "run failed but RecordReader read everything"
            );
        }
        Ok(()) => {
            let rows = parse_report(&out);
            check_rows(&rows);
            let expected = expected.expect("run succeeded but RecordReader hit a fatal error");
            assert_eq!(rows, expected, "report differs from the reference model");
        }
    }
});
