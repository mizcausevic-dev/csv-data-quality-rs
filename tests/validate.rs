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

struct Chunked<'a>(&'a [u8], usize);

impl std::io::Read for Chunked<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let count = self.0.len().min(buf.len()).min(self.1);
        buf[..count].copy_from_slice(&self.0[..count]);
        self.0 = &self.0[count..];
        Ok(count)
    }
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
fn timestamps_reject_invalid_dates_and_unicode_without_panicking() {
    let csv = "user_id,active_date,plan,ltv,verified\n\
u1,2026-02-29,pro,,true\n\
u2,é026-05-15,pro,,true\n\
u3,2026-05-15T99:00:00Z,pro,,true\n\
u4,2024-02-29T23:59:59.123+01:00,pro,,true\n";
    let report = validator().validate_bytes(csv.as_bytes()).unwrap();
    assert_eq!(report.rows_scanned, 4);
    assert_eq!(report.violation_count, 3);
    assert!(report
        .samples
        .iter()
        .all(|v| v.kind == ViolationKind::BadType));
}

#[test]
fn nonfinite_numbers_are_rejected() {
    let csv = b"user_id,active_date,plan,ltv,verified\n\
u1,2026-05-15,pro,NaN,true\n\
u2,2026-05-15,pro,inf,true\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 2);
    assert!(report
        .samples
        .iter()
        .all(|v| v.kind == ViolationKind::BadType));
}

#[test]
fn diagnostics_do_not_echo_csv_values_or_headers() {
    let value = "private@example.test";
    let csv = format!("user_id,active_date,plan,ltv,verified\nu1,2026-05-15,{value},,true\n");
    let report = validator().validate_bytes(csv.as_bytes()).unwrap();
    assert!(!report.samples[0].message.contains(value));
    let bad_header = format!("{value},active_date,plan,ltv,verified\n");
    let err = validator()
        .validate_bytes(bad_header.as_bytes())
        .unwrap_err();
    assert!(!err.to_string().contains(value));

    let invalid_utf8 =
        b"user_id,active_date,plan,ltv,verified\nprivate@example.test\xff,2026-05-15,pro,,true\n";
    let err = validator().validate_bytes(invalid_utf8).unwrap_err();
    assert!(!err.to_string().contains("private@example.test"));
}

#[test]
fn string_enum_keeps_meaningful_surrounding_spaces() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15, pro ,,true\n";
    let report = validator().validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 1);
    assert_eq!(report.samples[0].kind, ViolationKind::EnumMismatch);
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

#[test]
fn zero_samples_keeps_only_the_total() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,bogus,,true\n";
    let report = validator().max_samples(0).validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 1);
    assert!(report.samples.is_empty());
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

#[tokio::test]
async fn larger_file_keeps_only_capped_samples() {
    use std::io::Write;

    let file = tempfile::NamedTempFile::new().unwrap();
    {
        let mut out = std::io::BufWriter::new(file.reopen().unwrap());
        out.write_all(b"user_id,active_date,plan,ltv,verified\n")
            .unwrap();
        for _ in 0..100_000 {
            out.write_all(b"u,2026-05-15,bogus,,true\n").unwrap();
        }
        out.flush().unwrap();
    }
    let report = validator()
        .max_samples(3)
        .validate_file(file.path())
        .await
        .unwrap();
    assert_eq!(report.rows_scanned, 100_000);
    assert_eq!(report.violation_count, 100_000);
    assert_eq!(report.samples.len(), 3);
}

#[test]
fn validates_a_chunked_reader_without_collecting_the_file() {
    let csv = b"user_id,active_date,plan,ltv,verified\nu1,2026-05-15,pro,,true\n";
    let report = validator().validate_reader(Chunked(csv, 3)).unwrap();
    assert!(report.valid);
    assert_eq!(report.rows_scanned, 1);
}

#[test]
fn oversized_logical_record_is_rejected_before_parsing() {
    let contract = Contract::from_json(
        r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"value","type":"string"}]}"#,
    )
    .unwrap();
    let csv = b"value\n\"abcd\nefgh\nijkl\"\n";
    let err = Validator::new(contract)
        .max_record_bytes(12)
        .validate_bytes(csv)
        .unwrap_err();
    assert!(err.to_string().contains("byte limit"));
}

#[test]
fn quoted_multiline_and_escaped_quotes_count_as_one_record() {
    let contract = Contract::from_json(
        r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"value","type":"string"}]}"#,
    )
    .unwrap();
    let csv = b"value\r\n\"a,b\r\nc\"\"d\"\r\n";
    let report = Validator::new(contract.clone())
        .max_record_bytes(13)
        .validate_reader(Chunked(csv, 2))
        .unwrap();
    assert!(report.valid);
    assert_eq!(report.rows_scanned, 1);
    let err = Validator::new(contract)
        .max_record_bytes(10)
        .validate_reader(Chunked(csv, 2))
        .unwrap_err();
    assert!(err.to_string().contains("byte limit"));
}

#[test]
fn numeric_and_boolean_enums_use_typed_values() {
    let contract = Contract::from_json(
        r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"score","type":"number","enum":[1,2]},{"name":"pass","type":"boolean","enum":[true]}]}"#,
    )
    .unwrap();
    let csv = b"score,pass\n1,yes\n2,false\n3,true\n";
    let report = Validator::new(contract).validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 2);
    assert!(report
        .samples
        .iter()
        .all(|v| v.kind == ViolationKind::EnumMismatch));
}

#[test]
fn large_integer_enum_does_not_match_a_neighbor_after_float_rounding() {
    let contract = Contract::from_json(
        r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"score","type":"number","enum":[9007199254740993]}]}"#,
    )
    .unwrap();
    let csv = b"score\n9007199254740993\n9007199254740992\n";
    let report = Validator::new(contract).validate_bytes(csv).unwrap();
    assert_eq!(report.violation_count, 1);
    assert_eq!(report.samples[0].row, 2);
}

#[test]
fn number_enum_rejects_ambiguous_float_boundary() {
    let integer_contract = Contract::from_json(
        r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"score","type":"number","enum":[9007199254740992]}]}"#,
    )
    .unwrap();
    let float_contract = Contract::from_json(
        r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"score","type":"number","enum":[9007199254740992.0]}]}"#,
    )
    .unwrap();
    let neighbor = b"score\n9007199254740993\n";
    assert_eq!(
        Validator::new(integer_contract)
            .validate_bytes(neighbor)
            .unwrap()
            .violation_count,
        1
    );
    assert_eq!(
        Validator::new(float_contract)
            .validate_bytes(neighbor)
            .unwrap()
            .violation_count,
        1
    );
}

#[test]
fn fractional_registry_number_enum_matches() {
    let contract = Contract::from_json(
        r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"score","type":"number","enum":[1.5]}]}"#,
    )
    .unwrap();
    let report = Validator::new(contract)
        .validate_bytes(b"score\n1.5\n2.0\n")
        .unwrap();
    assert_eq!(report.violation_count, 1);
    assert_eq!(report.samples[0].row, 2);
}

#[test]
fn ambiguous_contracts_fail_closed() {
    for fields in [
        "[]",
        "[{\"name\":\"x\",\"type\":\"string\"},{\"name\":\"x\",\"type\":\"string\"}]",
    ] {
        let raw = format!(r#"{{"dataset_id":"d","version":"1.0.0","fields":{fields}}}"#);
        assert!(matches!(
            Contract::from_json(&raw),
            Err(csv_data_quality::CsvDataQualityError::InvalidContract(_))
        ));
    }
    let optional_key = r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"x","type":"string","required":false}],"primary_key":["x"]}"#;
    assert!(matches!(
        Contract::from_json(optional_key),
        Err(csv_data_quality::CsvDataQualityError::InvalidContract(_))
    ));
    let wrong_enum_type = r#"{"dataset_id":"d","version":"1.0.0","fields":[{"name":"x","type":"integer","enum":["one"]}]}"#;
    assert!(matches!(
        Contract::from_json(wrong_enum_type),
        Err(csv_data_quality::CsvDataQualityError::InvalidContract(_))
    ));
}

#[test]
fn consumes_the_registry_exported_contract_shape() {
    // Copied from data-contract-registry/examples/contract.json, generated from
    // examples/contract.yaml with DataContract.model_validate(...).model_dump(mode="json").
    let contract = Contract::from_json(include_str!("fixtures/registry-contract.json")).unwrap();
    assert_eq!(contract.fields.len(), 6);
    assert_eq!(contract.primary_key, ["user_id", "active_date"]);
    let validator = Validator::new(contract);
    let csv = b"user_id,active_date,plan,country,ltv,session_count\n\
u1,2026-05-15,pro,US,42.5,3\n\
u2,2026-05-15,unknown,CA,not-a-number,oops\n";
    let report = validator.validate_bytes(csv).unwrap();
    assert_eq!(report.rows_scanned, 2);
    assert_eq!(report.violation_count, 3);
    assert_eq!(report.samples[0].kind, ViolationKind::EnumMismatch);
    assert_eq!(report.samples[1].kind, ViolationKind::BadType);
    assert_eq!(report.samples[2].kind, ViolationKind::BadType);
}
