//! Slim Rust view of a `data-contract-registry` contract.
//!
//! We don't depend on the registry crate — the JSON travels independently.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

use crate::error::CsvDataQualityError;

/// Six primitives matching the registry vocabulary.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// UTF-8 string.
    String,
    /// 64-bit integer.
    Integer,
    /// Floating-point number.
    Number,
    /// Boolean (`true` / `false` / `1` / `0` / `yes` / `no` accepted as input).
    Boolean,
    /// ISO-8601 timestamp.
    Timestamp,
    /// Anything else — the cell is checked for valid JSON only.
    Json,
}

/// One column declaration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ContractField {
    /// Column name in the CSV header.
    pub name: String,
    /// Type the cell must match.
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// When `false`, an empty cell is allowed.
    #[serde(default = "default_required")]
    pub required: bool,
    /// If set, the cell value (parsed) must be one of these.
    #[serde(default)]
    pub r#enum: Option<Vec<Value>>,
    /// Free-form note for the operator.
    #[serde(default)]
    pub description: Option<String>,
}

fn default_required() -> bool {
    true
}

/// Whole contract.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Contract {
    /// Stable identifier — used in violation messages.
    pub dataset_id: String,
    /// Version of the contract.
    pub version: String,
    /// Columns the CSV must carry, in declaration order.
    pub fields: Vec<ContractField>,
    /// Optional list of column names that form the primary key.
    #[serde(default)]
    pub primary_key: Vec<String>,
}

impl Contract {
    /// Parse a contract from JSON.
    pub fn from_json(raw: &str) -> Result<Self, CsvDataQualityError> {
        let contract: Self = serde_json::from_str(raw)?;
        contract.validate()?;
        Ok(contract)
    }

    /// Check the fields that make a CSV contract unambiguous.
    pub fn validate(&self) -> Result<(), CsvDataQualityError> {
        if self.dataset_id.trim().is_empty()
            || self.dataset_id.len() > 256
            || self.version.trim().is_empty()
            || self.version.len() > 128
        {
            return Err(CsvDataQualityError::InvalidContract(
                "dataset_id and version must be nonempty and within length limits".to_string(),
            ));
        }
        if self.fields.is_empty() || self.fields.len() > 4096 {
            return Err(CsvDataQualityError::InvalidContract(
                "field count must be between 1 and 4096".to_string(),
            ));
        }
        let mut names = HashSet::new();
        for field in &self.fields {
            if field.name.trim().is_empty()
                || field.name.len() > 256
                || !names.insert(field.name.as_str())
            {
                return Err(CsvDataQualityError::InvalidContract(
                    "field names must be nonempty, unique, and at most 256 bytes".to_string(),
                ));
            }
            if let Some(values) = &field.r#enum {
                if values.is_empty() {
                    return Err(CsvDataQualityError::InvalidContract(
                        "enum declarations must contain at least one value".to_string(),
                    ));
                }
                if values.iter().any(|value| !match field.field_type {
                    FieldType::String | FieldType::Timestamp => value.is_string(),
                    FieldType::Integer => value.as_i64().is_some(),
                    FieldType::Number => value.is_number(),
                    FieldType::Boolean => value.is_boolean(),
                    FieldType::Json => true,
                }) {
                    return Err(CsvDataQualityError::InvalidContract(
                        "enum values must match their field type".to_string(),
                    ));
                }
            }
        }
        let mut keys = HashSet::new();
        for key in &self.primary_key {
            if !names.contains(key.as_str()) || !keys.insert(key.as_str()) {
                return Err(CsvDataQualityError::InvalidContract(
                    "primary_key entries must name distinct declared fields".to_string(),
                ));
            }
            if self
                .fields
                .iter()
                .any(|field| field.name == *key && !field.required)
            {
                return Err(CsvDataQualityError::InvalidContract(
                    "primary_key fields must be required".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Field name lookup.
    pub fn field(&self, name: &str) -> Option<&ContractField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Field-name set for header-matching.
    pub fn field_names(&self) -> Vec<&str> {
        self.fields.iter().map(|f| f.name.as_str()).collect()
    }
}
