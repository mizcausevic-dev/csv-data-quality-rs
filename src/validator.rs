//! The validator. Streams rows, checks cells, emits a [`ValidationReport`].

use std::io::Cursor;
use std::path::Path;

use serde_json::Value;
use tokio::fs;

use crate::contract::{Contract, ContractField, FieldType};
use crate::error::CsvDataQualityError;
use crate::report::{ValidationReport, Violation, ViolationKind};

/// The validator. Owns the contract.
pub struct Validator {
    contract: Contract,
    /// Max sample violations the report keeps in memory. `0` = unlimited.
    max_samples: usize,
}

impl Validator {
    /// Build a validator from a contract. Default sample cap: 100.
    pub fn new(contract: Contract) -> Self {
        Self {
            contract,
            max_samples: 100,
        }
    }

    /// Override the sample cap. `0` = keep every violation.
    #[must_use]
    pub fn max_samples(mut self, n: usize) -> Self {
        self.max_samples = n;
        self
    }

    /// Read a CSV file off disk and validate it.
    pub async fn validate_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<ValidationReport, CsvDataQualityError> {
        let bytes = fs::read(path).await?;
        self.validate_bytes(&bytes)
    }

    /// Validate an in-memory CSV buffer.
    pub fn validate_bytes(&self, bytes: &[u8]) -> Result<ValidationReport, CsvDataQualityError> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_reader(Cursor::new(bytes));

        // Header check.
        let headers = reader
            .headers()
            .map_err(|err| CsvDataQualityError::Parse {
                line: 1,
                source: err,
            })?
            .iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let expected = self.contract.field_names();
        if headers.len() != expected.len() || headers.iter().zip(&expected).any(|(h, e)| h != e) {
            return Err(CsvDataQualityError::HeaderMismatch(format!(
                "expected {expected:?}, got {headers:?}"
            )));
        }

        let mut violations: Vec<Violation> = Vec::new();
        let mut violation_count: u64 = 0;
        let mut rows: u64 = 0;
        let expected_cols = self.contract.fields.len();

        let mut record = csv::StringRecord::new();
        loop {
            let parsed =
                reader
                    .read_record(&mut record)
                    .map_err(|err| CsvDataQualityError::Parse {
                        line: reader.position().line(),
                        source: err,
                    })?;
            if !parsed {
                break;
            }
            rows += 1;

            if record.len() != expected_cols {
                self.push(
                    &mut violations,
                    &mut violation_count,
                    Violation {
                        row: rows,
                        column: "<row>".to_string(),
                        kind: ViolationKind::ColumnCountMismatch,
                        message: format!(
                            "row has {} cells, expected {}",
                            record.len(),
                            expected_cols
                        ),
                    },
                );
                continue;
            }

            for (i, field) in self.contract.fields.iter().enumerate() {
                let raw = record.get(i).unwrap_or("");
                if let Some(violation) = self.check_cell(rows, field, raw) {
                    self.push(&mut violations, &mut violation_count, violation);
                }
            }
        }

        Ok(ValidationReport {
            dataset_id: self.contract.dataset_id.clone(),
            contract_version: self.contract.version.clone(),
            rows_scanned: rows,
            violation_count,
            samples: violations,
            valid: violation_count == 0,
        })
    }

    fn push(&self, violations: &mut Vec<Violation>, count: &mut u64, v: Violation) {
        *count += 1;
        if self.max_samples == 0 || violations.len() < self.max_samples {
            violations.push(v);
        }
    }

    fn check_cell(&self, row: u64, field: &ContractField, raw: &str) -> Option<Violation> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            if field.required {
                return Some(Violation {
                    row,
                    column: field.name.clone(),
                    kind: ViolationKind::Required,
                    message: format!("required column {:?} is empty", field.name),
                });
            }
            return None;
        }

        // Type check.
        match field.field_type {
            FieldType::String => {}
            FieldType::Integer => {
                if trimmed.parse::<i64>().is_err() {
                    return Some(self.bad_type(row, field, trimmed, "integer"));
                }
            }
            FieldType::Number => {
                if trimmed.parse::<f64>().is_err() {
                    return Some(self.bad_type(row, field, trimmed, "number"));
                }
            }
            FieldType::Boolean => {
                if !is_boolean(trimmed) {
                    return Some(self.bad_type(row, field, trimmed, "boolean"));
                }
            }
            FieldType::Timestamp => {
                if !is_timestamp_ish(trimmed) {
                    return Some(self.bad_type(row, field, trimmed, "timestamp"));
                }
            }
            FieldType::Json => {
                if serde_json::from_str::<Value>(trimmed).is_err() {
                    return Some(Violation {
                        row,
                        column: field.name.clone(),
                        kind: ViolationKind::InvalidJson,
                        message: format!("column {:?} value is not valid JSON", field.name),
                    });
                }
            }
        }

        // Enum check.
        if let Some(values) = &field.r#enum {
            let cell_as_json = parse_cell_as_json(field.field_type, trimmed);
            if !values.iter().any(|v| v == &cell_as_json) {
                return Some(Violation {
                    row,
                    column: field.name.clone(),
                    kind: ViolationKind::EnumMismatch,
                    message: format!(
                        "column {:?} value {trimmed:?} is not in declared enum",
                        field.name
                    ),
                });
            }
        }

        None
    }

    fn bad_type(&self, row: u64, field: &ContractField, raw: &str, expected: &str) -> Violation {
        Violation {
            row,
            column: field.name.clone(),
            kind: ViolationKind::BadType,
            message: format!(
                "column {:?} value {:?} is not a valid {expected}",
                field.name, raw
            ),
        }
    }
}

fn is_boolean(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "1" | "0" | "yes" | "no"
    )
}

fn is_timestamp_ish(s: &str) -> bool {
    // Pragmatic — we don't pull chrono. Accept ISO-8601 with date + optional
    // time component; reject anything obviously off.
    // Pattern: YYYY-MM-DD with optional T... suffix.
    if s.len() < 10 {
        return false;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let year = &s[0..4];
    let month = &s[5..7];
    let day = &s[8..10];
    year.bytes().all(|b| b.is_ascii_digit())
        && month.bytes().all(|b| b.is_ascii_digit())
        && day.bytes().all(|b| b.is_ascii_digit())
}

fn parse_cell_as_json(field_type: FieldType, raw: &str) -> Value {
    match field_type {
        FieldType::String => Value::String(raw.to_string()),
        FieldType::Integer => raw.parse::<i64>().map_or(Value::Null, Value::from),
        FieldType::Number => raw.parse::<f64>().map_or(Value::Null, Value::from),
        FieldType::Boolean => Value::from(matches!(
            raw.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        )),
        FieldType::Timestamp => Value::String(raw.to_string()),
        FieldType::Json => serde_json::from_str(raw).unwrap_or(Value::Null),
    }
}
