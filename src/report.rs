//! Structured violation output.

use serde::{Deserialize, Serialize};

/// Categorised reason a cell failed validation.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// Required cell was empty.
    Required,
    /// Cell value did not parse as the declared type.
    BadType,
    /// Cell value was not one of the contract's enum entries.
    EnumMismatch,
    /// Row had a different number of columns than the header.
    ColumnCountMismatch,
    /// Cell declared as `json` was not valid JSON.
    InvalidJson,
}

/// One per-cell violation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Violation {
    /// 1-based row number (header excluded).
    pub row: u64,
    /// Column name (or `"<row>"` for whole-row issues like ColumnCountMismatch).
    pub column: String,
    /// Categorised reason.
    pub kind: ViolationKind,
    /// Human-readable message.
    pub message: String,
}

/// Aggregate report.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ValidationReport {
    /// Dataset id from the contract.
    pub dataset_id: String,
    /// Contract version.
    pub contract_version: String,
    /// Number of data rows scanned (excluding header).
    pub rows_scanned: u64,
    /// Total number of violations.
    pub violation_count: u64,
    /// Up to `max_samples` violations, oldest first.
    pub samples: Vec<Violation>,
    /// Whether the file passes (no violations).
    pub valid: bool,
}
