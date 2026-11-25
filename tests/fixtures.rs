//! Acceptance fixtures: every `tests/fixtures/<scenario>.input.csv` runs through the
//! pipeline function in-process and must produce `<scenario>.expected.csv`.

use std::fs;
use std::path::Path;

#[test]
fn every_fixture_produces_its_expected_report() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut scenarios: Vec<String> = fs::read_dir(&dir)
        .expect("fixtures directory is readable")
        .filter_map(|entry| {
            let name = entry.expect("fixture entry").file_name();
            let name = name.to_str()?;
            name.strip_suffix(".input.csv").map(str::to_owned)
        })
        .collect();
    scenarios.sort();
    assert!(
        !scenarios.is_empty(),
        "no fixtures found in {}",
        dir.display()
    );

    let mut failures = Vec::new();
    for scenario in &scenarios {
        let input = fs::File::open(dir.join(format!("{scenario}.input.csv")))
            .unwrap_or_else(|e| panic!("{scenario}: cannot open input: {e}"));
        let expected = fs::read_to_string(dir.join(format!("{scenario}.expected.csv")))
            .unwrap_or_else(|e| panic!("{scenario}: cannot read expected output: {e}"));

        let mut output = Vec::new();
        match transaction_engine::run(input, &mut output) {
            Err(e) => failures.push(format!("{scenario}: fatal error: {e}")),
            Ok(()) => {
                let actual = String::from_utf8(output).expect("report is UTF-8");
                if actual != expected {
                    failures.push(format!(
                        "{scenario}:\n--- expected\n{expected}--- actual\n{actual}"
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} fixtures failed:\n\n{}",
        failures.len(),
        scenarios.len(),
        failures.join("\n\n")
    );
}
