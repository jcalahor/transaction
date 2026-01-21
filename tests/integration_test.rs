use polars::prelude::*;
use std::fs;
use std::process::Command;

/// Helper function to run the transaction processor and compare output with expected CSV
fn assert_csv_output_matches(input_csv: &str, expected_csv: &str) {
    // Run the program with input CSV
    let output = Command::new("cargo")
        .args(&["run", "--", input_csv])
        .output()
        .expect("Failed to execute command");

    // Write stdout to temporary file
    let temp_output = format!("{}.output", input_csv);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Extract only CSV output (lines starting with numbers or the header)
    let csv_lines: Vec<&str> = stdout
        .lines()
        .skip_while(|line| !line.contains("client, available, held, total, locked"))
        .collect();

    // Normalize whitespace by removing spaces after commas for consistent parsing
    let normalized_lines: Vec<String> = csv_lines
        .iter()
        .map(|line| line.replace(", ", ","))
        .collect();

    fs::write(&temp_output, normalized_lines.join("\n")).expect("Failed to write output file");

    // Read both CSVs using polars
    let actual_df = CsvReader::from_path(&temp_output)
        .expect("Failed to read actual output")
        .has_header(true)
        .with_try_parse_dates(false)
        .truncate_ragged_lines(true)
        .finish()
        .expect("Failed to parse actual CSV");

    let expected_df = CsvReader::from_path(expected_csv)
        .expect("Failed to read expected output")
        .has_header(true)
        .with_try_parse_dates(false)
        .truncate_ragged_lines(true)
        .finish()
        .expect("Failed to parse expected CSV");

    // Compare shapes
    assert_eq!(
        actual_df.shape(),
        expected_df.shape(),
        "DataFrames have different shapes. Actual: {:?}, Expected: {:?}",
        actual_df.shape(),
        expected_df.shape()
    );

    // Compare column names
    assert_eq!(
        actual_df.get_column_names(),
        expected_df.get_column_names(),
        "Column names don't match"
    );

    // Sort both dataframes by client for consistent comparison
    let actual_sorted = actual_df
        .sort(["client"], false, false)
        .expect("Failed to sort actual dataframe");

    let expected_sorted = expected_df
        .sort(["client"], false, false)
        .expect("Failed to sort expected dataframe");

    // Compare each column
    for col_name in actual_sorted.get_column_names() {
        let actual_col = actual_sorted
            .column(col_name)
            .expect(&format!("Column {} not found in actual", col_name));
        let expected_col = expected_sorted
            .column(col_name)
            .expect(&format!("Column {} not found in expected", col_name));

        assert!(
            actual_col.equals(expected_col),
            "Column '{}' values don't match.\nActual:\n{:?}\nExpected:\n{:?}",
            col_name,
            actual_col,
            expected_col
        );
    }

    // Cleanup temporary file
    fs::remove_file(&temp_output).ok();
}

#[test]
fn test_csv_processing_integration() {
    assert_csv_output_matches(
        "tests/input/test_data.csv",
        "tests/expected/test_data_expected.csv",
    );
}

#[test]
fn test_single_client_multiple_transactions() {
    assert_csv_output_matches(
        "tests/input/single_client.csv",
        "tests/expected/single_client_expected.csv",
    );
}

#[test]
fn test_empty_csv() {
    assert_csv_output_matches("tests/input/empty.csv", "tests/expected/empty_expected.csv");
}

#[test]
fn test_dispute_and_resolve() {
    assert_csv_output_matches(
        "tests/input/dispute_resolve.csv",
        "tests/expected/dispute_resolve_expected.csv",
    );
}

#[test]
fn test_dispute_and_chargeback() {
    assert_csv_output_matches(
        "tests/input/dispute_chargeback.csv",
        "tests/expected/dispute_chargeback_expected.csv",
    );
}
