//! `cargo run --example validate`
//!
//! Validates a tiny in-memory CSV against an in-memory contract and prints
//! the report.

use csv_data_quality::{Contract, Validator};

const CONTRACT: &str = r#"{
  "dataset_id": "users.daily_active",
  "version": "1.0.0",
  "fields": [
    {"name": "user_id",     "type": "string"},
    {"name": "active_date", "type": "timestamp"},
    {"name": "plan",        "type": "string", "enum": ["free", "pro", "enterprise"]},
    {"name": "ltv",         "type": "number", "required": false}
  ]
}"#;

const CSV: &[u8] = b"\
user_id,active_date,plan,ltv
u1,2026-05-15,pro,42.5
u2,2026-05-15,startup,not-a-number
u3,not-a-date,free,1.0
,2026-05-15,pro,1.0
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let contract = Contract::from_json(CONTRACT)?;
    let validator = Validator::new(contract);
    let report = validator.validate_bytes(CSV)?;

    println!("dataset:     {}", report.dataset_id);
    println!("version:     {}", report.contract_version);
    println!("rows:        {}", report.rows_scanned);
    println!("violations:  {}", report.violation_count);
    println!("valid:       {}", report.valid);
    println!();
    println!("samples:");
    for v in &report.samples {
        println!("  row {} col {:<14} kind={:?}", v.row, v.column, v.kind);
        println!("    {}", v.message);
    }
    Ok(())
}
