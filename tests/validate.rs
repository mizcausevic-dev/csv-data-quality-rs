use csv_data_quality::{Contract, Validator, ViolationKind};

const CONTRACT: &str = r#"{
  "dataset_id": "users.daily_active",
  "version": "1.0.0",
  "fields": [
    {"name": "user_id",     "type": "string"},
    {"name": "active_date", "type": "timestamp"},
    {"name": "plan",        "type": "string", "enum": ["free", "pro", "enterprise"]},
    {"name": "ltv",         "type": "number", "required": false},
    {"name": "verified",    "type": "boolean"}
  ],
  "primary_key": ["user_id", "active_date"]
}"#;

fn validator() -> Validator {
    Validator::new(Contract::from_json(CONTRACT).unwrap())
}

#[test]
fn happy_path_no_violations() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,42.5,true\nu2,2026-05-15,free,,false\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert!(report.valid);
    assert_eq!(report.rows_scanned, 2);
    assert_eq!(report.violation_count, 0);
}

#[test]
fn required_cell_violation() {
    let csv = b"user_id,active_date,plan,ltv,verified\n,2026-05-15,pro,1.0,true\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert!(!report.valid);
    assert_eq!(report.samples[0].kind, ViolationKind::Required);
}

#[test]
fn integer_type_violation() {
    let contract = r#"{
      "dataset_id": "d",
      "version": "1.0.0",
      "fields": [{"name": "count", "type": "integer"}]
    }"#;
    let v = Validator::new(Contract::from_json(contract).unwrap());
    let csv = b"count\n42\nhello\n";
    let report = v.validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 1);
    assert_eq!(report.samples[0].kind, ViolationKind::BadType);
    assert_eq!(report.samples[0].row, 2);
}

#[test]
fn number_type_violation() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,not-a-number,true\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 1);
    assert_eq!(report.samples[0].column, "ltv");
}

#[test]
fn boolean_accepts_multiple_truthy_forms() {
    for cell in ["true", "false", "1", "0", "yes", "no", "TRUE", "Yes"] {
        let csv = format!("user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,,{cell}\n");
        let report = validator().validate_bytes(csv.as_bytes()).unwrap();
        assert!(report.valid, "cell {cell:?} should be a valid boolean");
    }
}

#[test]
fn boolean_rejects_arbitrary_strings() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,,maybe\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert_eq!(report.samples[0].kind, ViolationKind::BadType);
}

#[test]
fn timestamp_basic_shape_check() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,not-a-date,pro,,true\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert_eq!(report.samples[0].kind, ViolationKind::BadType);
    assert_eq!(report.samples[0].column, "active_date");
}

#[test]
fn enum_mismatch_for_unknown_plan() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,startup,,true\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert_eq!(report.samples[0].kind, ViolationKind::EnumMismatch);
}

#[test]
fn column_count_mismatch_reports_whole_row() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,1.0\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert!(report
        .samples
        .iter()
        .any(|v| v.kind == ViolationKind::ColumnCountMismatch));
}

#[test]
fn header_mismatch_returns_error() {
    let csv = b"wrong,header,order,here,today\nu1,2026-05-15,pro,1.0,true\n";
    let err = validator().validate_bytes(csv).unwrap_err();
    assert!(matches!(
        err,
        csv_data_quality::CsvDataQualityError::HeaderMismatch(_)
    ));
}

#[test]
fn optional_empty_cell_is_fine() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,,true\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert!(report.valid);
}

#[test]
fn json_field_validates_payload() {
    let contract = r#"{
      "dataset_id": "x",
      "version": "1.0.0",
      "fields": [{"name": "meta", "type": "json"}]
    }"#;
    let v = Validator::new(Contract::from_json(contract).unwrap());
    let csv = b"meta\n{\"k\":1}\nnot-json\n";
    let report = v.validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 1);
    assert_eq!(report.samples[0].kind, ViolationKind::InvalidJson);
    assert_eq!(report.samples[0].row, 2);
}

#[test]
fn max_samples_caps_the_sample_list() {
    let mut rows = String::from("user_id,active_date,plan,ltv,verified\n");
    // 5 rows all with bad plan.
    for _ in 0..5 {
        rows.push_str("u,2026-05-15,bogus,,true\n");
    }
    let v = Validator::new(Contract::from_json(CONTRACT).unwrap()).max_samples(2);
    let report = v.validate_bytes(rows.as_bytes()).unwrap();
    assert_eq!(report.violation_count, 5);
    assert_eq!(report.samples.len(), 2);
}

#[tokio::test]
async fn validate_file_round_trip() {
    use tempfile::NamedTempFile;
    use tokio::fs;

    let f = NamedTempFile::new().unwrap();
    fs::write(
        f.path(),
        b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,1.0,true\n",
    )
    .await
    .unwrap();

    let report = validator().validate_file(f.path()).await.unwrap();
    assert!(report.valid);
    assert_eq!(report.rows_scanned, 1);
}
