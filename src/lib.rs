//! # csv-data-quality
//!
//! **Streaming CSV validator against a [`data-contract-registry`](https://github.com/mizcausevic-dev/data-contract-registry)
//! contract.** Reads a CSV row by row, checks each cell against the contract's
//! field type / required / enum constraints, and emits a structured
//! [`ValidationReport`].
//!
//! ## What it answers
//!
//! When the registry says "the dataset must look like this", the producer has
//! to be able to *prove* their output matches. This crate is the proof.
//!
//! ```no_run
//! use csv_data_quality::{Validator, Contract, FieldType};
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let contract = Contract::from_json(r#"{
//!   "dataset_id": "users.daily_active",
//!   "version": "1.0.0",
//!   "fields": [
//!     {"name": "user_id",     "type": "string"},
//!     {"name": "active_date", "type": "timestamp"},
//!     {"name": "plan",        "type": "string", "enum": ["free", "pro"]},
//!     {"name": "ltv",         "type": "number", "required": false}
//!   ]
//! }"#)?;
//!
//! let validator = Validator::new(contract);
//! let report = validator.validate_file("daily_active_2026_05_15.csv").await?;
//! println!("{} violation(s)", report.violation_count);
//! # Ok(()) }
//! ```
//!
//! ## Pieces
//!
//! - [`Contract`] — slim Rust struct that accepts the JSON shape
//!   `data-contract-registry` emits. No coupling to the Python service —
//!   the JSON travels independently.
//! - [`FieldType`] — six primitives (string / integer / number / boolean /
//!   timestamp / json), matching the registry's vocabulary.
//! - [`Validator`] — owns the contract, validates a stream of rows.
//! - [`Violation`] — structured per-cell error: row, column, kind, message.
//! - [`ValidationReport`] — count + first-N violations + sample.
//!
//! ## Composes with
//!
//! - **[data-contract-registry](https://github.com/mizcausevic-dev/data-contract-registry)**
//!   — the source of contracts. Fourth cross-ecosystem hook in the portfolio.
//! - **[audit-stream-py](https://github.com/mizcausevic-dev/audit-stream-py)**
//!   — emit a `contract_compatibility_failed` event when validation lights up.
//! - **[reliability-toolkit-rs](https://github.com/mizcausevic-dev/reliability-toolkit-rs)**
//!   — wrap registry-fetch calls in a circuit breaker.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::unused_self)]

pub mod contract;
pub mod error;
pub mod report;
pub mod validator;

pub use contract::{Contract, ContractField, FieldType};
pub use error::CsvDataQualityError;
pub use report::{ValidationReport, Violation, ViolationKind};
pub use validator::Validator;
