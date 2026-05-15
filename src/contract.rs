//! Slim Rust view of a `data-contract-registry` contract.
//!
//! We don't depend on the registry crate — the JSON travels independently.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        Ok(serde_json::from_str(raw)?)
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
