//! The validator. Streams rows, checks cells, emits a [`ValidationReport`].

use std::fs::File;
use std::io::{self, BufReader, Cursor, Read};
use std::path::Path;

use serde_json::Value;

use crate::contract::{Contract, ContractField, FieldType};
use crate::error::CsvDataQualityError;
use crate::report::{ValidationReport, Violation, ViolationKind};

/// The validator. Owns the contract.
pub struct Validator {
    contract: Contract,
    /// Max sample violations the report keeps in memory.
    max_samples: usize,
    max_record_bytes: usize,
}

const MAX_SAMPLES: usize = 10_000;
const DEFAULT_RECORD_BYTES: usize = 8 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;

impl Validator {
    /// Build a validator from a contract. Default sample cap: 100.
    pub fn new(contract: Contract) -> Self {
        Self {
            contract,
            max_samples: 100,
            max_record_bytes: DEFAULT_RECORD_BYTES,
        }
    }

    /// Override the sample cap. `0` retains no samples; values above 10,000 are capped.
    #[must_use]
    pub fn max_samples(mut self, n: usize) -> Self {
        self.max_samples = n.min(MAX_SAMPLES);
        self
    }

    /// Set the maximum bytes in a CSV record, including embedded newlines.
    /// The default is 8 MiB; values above 64 MiB are capped.
    #[must_use]
    pub fn max_record_bytes(mut self, n: usize) -> Self {
        self.max_record_bytes = n.min(MAX_RECORD_BYTES);
        self
    }

    /// Read a CSV file off disk and validate it.
    pub async fn validate_file<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Result<ValidationReport, CsvDataQualityError> {
        let path = path.as_ref().to_path_buf();
        let contract = self.contract.clone();
        let max_samples = self.max_samples;
        let max_record_bytes = self.max_record_bytes;
        tokio::task::spawn_blocking(move || {
            let file = File::open(path)?;
            Self {
                contract,
                max_samples,
                max_record_bytes,
            }
            .validate_reader(BufReader::new(file))
        })
        .await?
    }

    /// Validate an in-memory CSV buffer.
    pub fn validate_bytes(&self, bytes: &[u8]) -> Result<ValidationReport, CsvDataQualityError> {
        self.validate_reader(Cursor::new(bytes))
    }

    /// Validate a CSV reader incrementally. Memory also depends on the largest CSV record.
    pub fn validate_reader<R: Read>(
        &self,
        input: R,
    ) -> Result<ValidationReport, CsvDataQualityError> {
        self.contract.validate()?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_reader(RecordLimitedReader::new(input, self.max_record_bytes));

        // Header check.
        let headers = reader
            .headers()
            .map_err(|err| CsvDataQualityError::Parse {
                line: 1,
                source: err,
            })?
            .iter()
            .collect::<Vec<_>>();
        let expected = self.contract.field_names();
        if headers.len() != expected.len() || headers.iter().zip(&expected).any(|(h, e)| h != e) {
            let first_mismatch = headers.iter().zip(&expected).position(|(h, e)| h != e);
            return Err(CsvDataQualityError::HeaderMismatch(format!(
                "expected {} columns, got {}; first differing column: {}",
                expected.len(),
                headers.len(),
                first_mismatch.map_or_else(|| "count".to_string(), |i| (i + 1).to_string())
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
        if violations.len() < self.max_samples {
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
                    return Some(self.bad_type(row, field, "integer"));
                }
            }
            FieldType::Number => {
                if !trimmed.parse::<f64>().is_ok_and(f64::is_finite) {
                    return Some(self.bad_type(row, field, "number"));
                }
            }
            FieldType::Boolean => {
                if !is_boolean(trimmed) {
                    return Some(self.bad_type(row, field, "boolean"));
                }
            }
            FieldType::Timestamp => {
                if !is_timestamp_ish(trimmed) {
                    return Some(self.bad_type(row, field, "timestamp"));
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
            if !values
                .iter()
                .any(|value| enum_matches(field.field_type, raw, trimmed, value))
            {
                return Some(Violation {
                    row,
                    column: field.name.clone(),
                    kind: ViolationKind::EnumMismatch,
                    message: format!("column {:?} value is not in declared enum", field.name),
                });
            }
        }

        None
    }

    fn bad_type(&self, row: u64, field: &ContractField, expected: &str) -> Violation {
        Violation {
            row,
            column: field.name.clone(),
            kind: ViolationKind::BadType,
            message: format!("column {:?} value is not a valid {expected}", field.name),
        }
    }
}

/// Reject a large logical CSV record before the CSV parser buffers it.
struct RecordLimitedReader<R> {
    inner: R,
    limit: usize,
    record_bytes: usize,
    quoted: bool,
    at_field_start: bool,
    just_closed_quote: bool,
}

impl<R> RecordLimitedReader<R> {
    fn new(inner: R, limit: usize) -> Self {
        Self {
            inner,
            limit,
            record_bytes: 0,
            quoted: false,
            at_field_start: true,
            just_closed_quote: false,
        }
    }
}

impl<R: Read> Read for RecordLimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buf)?;
        for &byte in &buf[..count] {
            self.record_bytes += 1;
            if self.record_bytes > self.limit {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "CSV record exceeds configured byte limit",
                ));
            }
            if self.quoted {
                if byte == b'"' {
                    self.quoted = false;
                    self.just_closed_quote = true;
                }
                continue;
            }
            match byte {
                b'"' if self.at_field_start || self.just_closed_quote => {
                    self.quoted = true;
                    self.at_field_start = false;
                    self.just_closed_quote = false;
                }
                b',' => {
                    self.at_field_start = true;
                    self.just_closed_quote = false;
                }
                b'\r' | b'\n' => {
                    self.record_bytes = 0;
                    self.at_field_start = true;
                    self.just_closed_quote = false;
                }
                _ => {
                    self.at_field_start = false;
                    self.just_closed_quote = false;
                }
            }
        }
        Ok(count)
    }
}

fn is_boolean(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "false" | "1" | "0" | "yes" | "no"
    )
}

fn is_timestamp_ish(s: &str) -> bool {
    // Date or ISO-8601 date-time. Byte access avoids panics on non-ASCII input.
    let bytes = s.as_bytes();
    if bytes.len() < 10 || !bytes.is_ascii() || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let (Some(year), Some(month), Some(day)) = (
        digits(&bytes[0..4]),
        digits(&bytes[5..7]),
        digits(&bytes[8..10]),
    ) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > max_day {
        return false;
    }
    if bytes.len() == 10 {
        return true;
    }
    if bytes.len() < 19 || bytes[10] != b'T' || bytes[13] != b':' || bytes[16] != b':' {
        return false;
    }
    let (Some(hour), Some(minute), Some(second)) = (
        digits(&bytes[11..13]),
        digits(&bytes[14..16]),
        digits(&bytes[17..19]),
    ) else {
        return false;
    };
    if hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    let mut end = 19;
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        let start = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == start {
            return false;
        }
    }
    match bytes.get(end) {
        None => true,
        Some(b'Z') => end + 1 == bytes.len(),
        Some(b'+' | b'-') => {
            bytes.len() == end + 6
                && bytes[end + 3] == b':'
                && digits(&bytes[end + 1..end + 3]).is_some_and(|h| h <= 23)
                && digits(&bytes[end + 4..end + 6]).is_some_and(|m| m <= 59)
        }
        _ => false,
    }
}

fn digits(bytes: &[u8]) -> Option<u32> {
    bytes.iter().try_fold(0u32, |n, b| {
        if b.is_ascii_digit() {
            Some(n * 10 + u32::from(*b - b'0'))
        } else {
            None
        }
    })
}

fn enum_matches(field_type: FieldType, raw: &str, trimmed: &str, value: &Value) -> bool {
    match field_type {
        FieldType::String => value.as_str() == Some(raw),
        FieldType::Integer => trimmed
            .parse::<i64>()
            .ok()
            .zip(value.as_i64())
            .is_some_and(|(cell, allowed)| cell == allowed),
        FieldType::Number => number_enum_matches(trimmed, value),
        FieldType::Boolean => {
            value.as_bool()
                == Some(matches!(
                    trimmed.to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes"
                ))
        }
        FieldType::Timestamp => value.as_str() == Some(trimmed),
        FieldType::Json => serde_json::from_str::<Value>(trimmed)
            .ok()
            .is_some_and(|cell| cell == *value),
    }
}

const EXACT_F64_INTEGER: u64 = 9_007_199_254_740_992;

// Enum membership uses exact numeric equality; an epsilon would admit other values.
// Casts below are used only for integers inside f64's exact integer range.
#[allow(clippy::float_cmp, clippy::cast_precision_loss)]
fn number_enum_matches(raw: &str, value: &Value) -> bool {
    let Ok(cell) = raw.parse::<f64>() else {
        return false;
    };
    if let Some(allowed) = value.as_i64() {
        if let Ok(exact) = raw.parse::<i64>() {
            return exact == allowed;
        }
        return allowed.unsigned_abs() < EXACT_F64_INTEGER
            && cell.abs() < EXACT_F64_INTEGER as f64
            && cell == allowed as f64;
    }
    if let Some(allowed) = value.as_u64() {
        if let Ok(exact) = raw.parse::<u64>() {
            return exact == allowed;
        }
        return allowed < EXACT_F64_INTEGER
            && cell.abs() < EXACT_F64_INTEGER as f64
            && cell == allowed as f64;
    }
    value.as_f64().is_some_and(|allowed| {
        allowed.abs() < EXACT_F64_INTEGER as f64
            && cell.abs() < EXACT_F64_INTEGER as f64
            && cell == allowed
    })
}
