//! Crate-wide error type.

use thiserror::Error;

/// Anything that can go wrong inside the crate.
#[derive(Debug, Error)]
pub enum CsvDataQualityError {
    /// Failed to parse the JSON contract.
    #[error("invalid contract JSON: {0}")]
    Contract(#[from] serde_json::Error),

    /// I/O failure reading the CSV.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// The CSV header doesn't match the contract's declared fields.
    #[error("CSV header mismatch: {0}")]
    HeaderMismatch(String),

    /// CSV crate-level parse failure (malformed row, quote balance, etc.).
    #[error("csv parse error at line {line}: {source}")]
    Parse {
        /// 1-based line number, header-inclusive.
        line: u64,
        /// Underlying csv crate error.
        #[source]
        source: csv::Error,
    },
}
